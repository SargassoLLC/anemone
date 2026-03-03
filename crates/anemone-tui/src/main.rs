//! anemone-tui — Terminal UI for watching anemones think.
//! Uses Ratatui + Crossterm for rendering.

mod app;
mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use tracing::info;

use anemone_core::config::Config;

use app::{App, AppMode};

#[tokio::main]
async fn main() -> io::Result<()> {
    // Initialize tracing to a file (not stdout, since we own the terminal)
    let _guard = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(|| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("anemone-tui.log")
                .unwrap_or_else(|_| {
                    // Fallback: /dev/null
                    std::fs::File::open("/dev/null").unwrap()
                })
        })
        .try_init();

    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config_path = project_root.join("config.yaml");
    let mut config = Config::load(&config_path).unwrap_or_default();

    // ── First-run detection ────────────────────────────────────────────────────
    // Show the setup wizard when no API key is configured — either in config.yaml
    // or via common environment variables.
    let needs_setup = config.api_key.as_ref().map_or(true, |k| k.trim().is_empty())
        && std::env::var("OPENAI_API_KEY").map_or(true, |k| k.trim().is_empty())
        && std::env::var("OPENROUTER_API_KEY").map_or(true, |k| k.trim().is_empty());

    let initial_mode = if needs_setup {
        info!("No API key found — entering setup wizard");
        AppMode::Setup
    } else {
        AppMode::Running
    };

    let mut app = App::new(&project_root, &config, initial_mode);

    // In Running mode with no anemones, bail early with a helpful message.
    if app.mode == AppMode::Running && app.anemones.is_empty() {
        eprintln!("No anemones found. Create a *_box/ directory with identity.json first.");
        return Ok(());
    }

    info!(
        "Starting TUI (mode={:?}, anemones={})",
        app.mode,
        app.anemones.len()
    );

    // ── Setup terminal ─────────────────────────────────────────────────────────
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // ── Brain startup (deferred until Running mode is active) ─────────────────
    // We keep track of whether brains have been started so we can start them
    // lazily after setup completes.
    let mut brains_started = false;
    let mut merged_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(usize, anemone_core::events::BrainEvent)>> = None;

    let start_brains = |app: &App| -> tokio::sync::mpsc::UnboundedReceiver<(usize, anemone_core::events::BrainEvent)> {
        let mut event_receivers = Vec::new();

        for (idx, view) in app.anemones.iter().enumerate() {
            let brain_arc = view.brain.clone();

            // Spawn brain task
            let brain_for_task = brain_arc.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let mut brain = brain_for_task.write().await;
                brain.run().await;
            });

            // Subscribe to events
            let rx = {
                // Use try_read to avoid blocking_read panic inside tokio runtime
                let brain = brain_arc.try_read().expect("brain lock should be free at startup");
                brain.subscribe()
            };

            let (fwd_tx, fwd_rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move {
                let mut rx = rx;
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            if fwd_tx.send((idx, event)).is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            event_receivers.push(fwd_rx);
        }

        let (merged_tx, merged_rx) =
            tokio::sync::mpsc::unbounded_channel::<(usize, anemone_core::events::BrainEvent)>();

        for mut rx in event_receivers {
            let tx = merged_tx.clone();
            tokio::spawn(async move {
                while let Some(item) = rx.recv().await {
                    if tx.send(item).is_err() {
                        break;
                    }
                }
            });
        }
        drop(merged_tx); // Drop original so channel closes when all forwarders stop

        merged_rx
    };

    // If we're already in Running mode, start brains immediately.
    if app.mode == AppMode::Running {
        merged_rx = Some(start_brains(&app));
        brains_started = true;
    }

    // ── Main event loop ────────────────────────────────────────────────────────
    let mut last_file_refresh = std::time::Instant::now();
    loop {
        // Refresh file listings every 5 seconds
        if last_file_refresh.elapsed() > Duration::from_secs(5) {
            app.refresh_files();
            last_file_refresh = std::time::Instant::now();
        }

        // ── Draw ──────────────────────────────────────────────────────────────
        terminal.draw(|frame| {
            match app.mode {
                AppMode::Setup => {
                    ui::setup::draw_setup(frame, frame.area(), &app.setup_state);
                }
                AppMode::Running => {
                    ui::draw(frame, &app);
                }
            }
        })?;

        // ── Brain events (Running mode only) ───────────────────────────────
        if let Some(ref mut rx) = merged_rx {
            while let Ok(item) = rx.try_recv() {
                app.handle_event(item.0, item.1);
            }
        }

        // ── Terminal input ─────────────────────────────────────────────────
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    // ── Setup input ────────────────────────────────────────
                    AppMode::Setup => {
                        match (key.code, key.modifiers) {
                            // Global quit in setup
                            (KeyCode::Char('c'), KeyModifiers::CONTROL)
                            | (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                                app.should_quit = true;
                            }
                            // All other keys go to the setup state machine
                            (code, _) => {
                                let done = app.handle_setup_key(code);
                                if done {
                                    // ── Transition: Setup → Running ────────
                                    info!("Setup complete — saving config and starting TUI");

                                    // Persist the updated config
                                    let new_config = app.finish_setup(&project_root, &config);
                                    if let Err(e) = new_config.save(&config_path) {
                                        // Non-fatal: warn but continue
                                        info!("Could not save config: {e}");
                                    } else {
                                        config = new_config;
                                    }

                                    // Guard: if no anemones found after setup, bail gracefully
                                    if app.anemones.is_empty() {
                                        // Show a quick message then exit cleanly
                                        app.should_quit = true;
                                    } else if !brains_started {
                                        merged_rx = Some(start_brains(&app));
                                        brains_started = true;
                                    }
                                }
                            }
                        }
                    }

                    // ── Running input ──────────────────────────────────────
                    AppMode::Running => {
                        match (key.code, key.modifiers) {
                            // Quit
                            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                app.should_quit = true;
                            }
                            (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                                app.should_quit = true;
                            }
                            // Tab switching
                            (KeyCode::Right, KeyModifiers::ALT) => app.next_tab(),
                            (KeyCode::Left, KeyModifiers::ALT) => app.prev_tab(),
                            // Focus toggle
                            (KeyCode::Tab, _) => {
                                app.input_focused = !app.input_focused;
                            }
                            // Input handling
                            (KeyCode::Enter, _) if app.input_focused => {
                                app.send_message().await;
                            }
                            (KeyCode::Char(c), _) if app.input_focused => {
                                app.input.push(c);
                            }
                            (KeyCode::Backspace, _) if app.input_focused => {
                                app.input.pop();
                            }
                            // Scroll
                            (KeyCode::Up, _) if !app.input_focused => app.scroll_up(),
                            (KeyCode::Down, _) if !app.input_focused => app.scroll_down(),
                            (KeyCode::PageUp, _) => app.page_up(),
                            (KeyCode::PageDown, _) => app.page_down(),
                            _ => {}
                        }
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // ── Cleanup ────────────────────────────────────────────────────────────────
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    // Stop all brains
    for view in &app.anemones {
        let _ = view
            .command_tx
            .send(anemone_core::brain::BrainCommand::Stop)
            .await;
    }

    // Print helpful message if setup finished with no anemones
    if app.mode == AppMode::Running && app.anemones.is_empty() {
        eprintln!("Setup complete! Create a *_box/ directory with identity.json, then run anemone-tui again.");
    }

    Ok(())
}
