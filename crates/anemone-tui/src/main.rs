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

use app::App;

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
    let config = Config::load(&config_path).unwrap_or_default();

    let mut app = App::new(&project_root, &config);

    if app.anemones.is_empty() {
        eprintln!("No anemones found. Create a *_box/ directory with identity.json first.");
        return Ok(());
    }

    info!("Starting TUI with {} anemone(s)", app.anemones.len());

    // Start all brains and collect event receivers
    let mut event_receivers = Vec::new();
    for (idx, view) in app.anemones.iter().enumerate() {
        let brain_arc = view.brain.clone();
        let mut rx = {
            let brain = brain_arc.read().await;
            brain.subscribe()
        };

        // Spawn brain task
        let brain_for_task = brain_arc.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let mut brain = brain_for_task.write().await;
            brain.run().await;
        });

        // Spawn event forwarder — sends (index, event) to a unified channel
        let (fwd_tx, fwd_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
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

    // Merge all event receivers into a single channel
    let (merged_tx, mut merged_rx) =
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
    drop(merged_tx); // Drop the original sender so the channel closes when all forwarders stop

    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // Main event loop
    loop {
        // Draw
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Handle brain events (non-blocking)
        while let Ok(item) = merged_rx.try_recv() {
            app.handle_event(item.0, item.1);
        }

        // Handle terminal events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
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
                    (KeyCode::PageUp, _) => app.scroll_up(),
                    (KeyCode::PageDown, _) => app.scroll_down(),
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    // Stop all brains
    for view in &app.anemones {
        let _ = view
            .command_tx
            .send(anemone_core::brain::BrainCommand::Stop)
            .await;
    }

    Ok(())
}
