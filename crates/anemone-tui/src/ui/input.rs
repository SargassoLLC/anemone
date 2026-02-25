//! Text input bar.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let border_color = if app.input_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Message (Enter to send, Tab to switch focus) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White));
    frame.render_widget(input, inner);

    // Show cursor
    if app.input_focused {
        frame.set_cursor_position(Position::new(
            inner.x + app.input.len() as u16,
            inner.y,
        ));
    }
}
