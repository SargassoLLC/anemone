//! TUI layout compositing — assembles all UI panels.

mod chat;
mod input;
mod room;
mod status;
mod switcher;

use ratatui::prelude::*;
use ratatui::widgets::Block;

use crate::app::App;

/// Render the full TUI layout.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // ┌──────────────────────────────────┐
    // │ Tabs (switcher)                  │
    // ├────────────┬─────────────────────┤
    // │   Room     │     Chat feed       │
    // │  (ASCII)   │                     │
    // │            │                     │
    // ├────────────┴─────────────────────┤
    // │ Status bar                       │
    // ├──────────────────────────────────┤
    // │ Input                            │
    // └──────────────────────────────────┘

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),   // tabs
            Constraint::Min(10),     // content
            Constraint::Length(1),   // status
            Constraint::Length(3),   // input
        ])
        .split(area);

    // Tabs
    switcher::draw(frame, app, main_layout[0]);

    // Content: Room | Chat
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28), // room
            Constraint::Min(30),   // chat
        ])
        .split(main_layout[1]);

    if let Some(view) = app.active_view() {
        room::draw(frame, view, content_layout[0]);
        chat::draw(frame, view, content_layout[1]);
        status::draw(frame, view, main_layout[2]);
    } else {
        let empty = Block::default().title(" No anemones found ");
        frame.render_widget(empty, main_layout[1]);
    }

    // Input
    input::draw(frame, app, main_layout[3]);
}
