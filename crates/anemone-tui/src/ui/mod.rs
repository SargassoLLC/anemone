//! TUI layout compositing — assembles all UI panels.
//! Design: Claude Code CLI aesthetic — rounded borders, magenta accent, dim grays.

mod chat;
mod input;
mod room;
mod status;
mod switcher;
pub mod setup;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};

use crate::app::App;

/// Color palette — consistent across all panels
pub const ACCENT: Color = Color::Magenta;
pub const ACCENT_DIM: Color = Color::Rgb(140, 80, 160);
pub const BORDER: Color = Color::Rgb(60, 60, 70);
pub const BORDER_ACTIVE: Color = Color::Rgb(120, 80, 160);
pub const TEXT_PRIMARY: Color = Color::Rgb(220, 220, 230);
pub const TEXT_DIM: Color = Color::Rgb(100, 100, 115);
pub const TEXT_MUTED: Color = Color::Rgb(65, 65, 75);
pub const BG: Color = Color::Rgb(15, 15, 20);
pub const BG_SURFACE: Color = Color::Rgb(22, 22, 30);
pub const GREEN: Color = Color::Rgb(80, 200, 120);
pub const BLUE: Color = Color::Rgb(100, 149, 237);
pub const YELLOW: Color = Color::Rgb(240, 200, 80);
pub const CYAN: Color = Color::Rgb(100, 210, 230);
pub const RED: Color = Color::Rgb(220, 80, 80);

/// Render the full TUI layout.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Fill background
    let bg_block = Block::default().style(Style::default().bg(BG));
    frame.render_widget(bg_block, area);

    // ┌──────────────────────────────────┐
    // │ Header (name + tabs)             │
    // ├────────────┬─────────────────────┤
    // │  Sidebar   │     Chat feed       │
    // │  Room +    │                     │
    // │  Files     │                     │
    // ├────────────┴─────────────────────┤
    // │ Status bar                       │
    // ├──────────────────────────────────┤
    // │ Input                            │
    // └──────────────────────────────────┘

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),   // header
            Constraint::Min(10),     // content
            Constraint::Length(1),   // status
            Constraint::Length(3),   // input
        ])
        .split(area);

    // Header
    switcher::draw(frame, app, main_layout[0]);

    // Content: Sidebar | Chat
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30), // sidebar
            Constraint::Min(30),   // chat
        ])
        .split(main_layout[1]);

    if let Some(view) = app.active_view() {
        // Sidebar: Room on top, file tree below
        let sidebar_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(16), // room
                Constraint::Min(5),    // files
            ])
            .split(content_layout[0]);

        room::draw(frame, view, sidebar_layout[0]);
        draw_files(frame, view, sidebar_layout[1]);
        chat::draw(frame, view, content_layout[1]);
        status::draw(frame, view, app, main_layout[2]);
    } else {
        let empty = Paragraph::new("  No anemones found. Run setup again.")
            .style(Style::default().fg(TEXT_DIM));
        frame.render_widget(empty, main_layout[1]);
    }

    // Input
    input::draw(frame, app, main_layout[3]);
}

/// File tree panel — shows what the anemone has created
fn draw_files(frame: &mut Frame, view: &crate::app::AnemoneView, area: Rect) {
    let block = Block::default()
        .title(Span::styled(" Files ", Style::default().fg(TEXT_DIM)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show files from the anemone's box directory
    let mut lines: Vec<Line> = Vec::new();

    if view.files.is_empty() {
        lines.push(Line::styled("  (empty)", Style::default().fg(TEXT_MUTED)));
    } else {
        for file in &view.files {
            let (icon, color) = file_icon(file);
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), Style::default().fg(color)),
                Span::styled(file.as_str(), Style::default().fg(TEXT_DIM)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn file_icon(name: &str) -> (&'static str, Color) {
    if name.ends_with('/') {
        ("📁", YELLOW)
    } else if name.ends_with(".md") {
        ("📝", BLUE)
    } else if name.ends_with(".py") {
        ("🐍", GREEN)
    } else if name.ends_with(".json") {
        ("⚙", CYAN)
    } else if name.ends_with(".jsonl") {
        ("⚙", CYAN)
    } else {
        ("  ", TEXT_DIM)
    }
}
