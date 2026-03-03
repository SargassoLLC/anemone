//! Text input bar with placeholder and focus styling.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::App;
use super::{ACCENT, BORDER, BORDER_ACTIVE, TEXT_PRIMARY, TEXT_MUTED, BG};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let (border_color, title_style) = if app.input_focused {
        (BORDER_ACTIVE, Style::default().fg(ACCENT))
    } else {
        (BORDER, Style::default().fg(TEXT_MUTED))
    };

    let block = Block::default()
        .title(Span::styled(" ❯ say something ", title_style))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.input.is_empty() && !app.input_focused {
        let placeholder = Paragraph::new("  Tab to focus, type to talk to your anemone...")
            .style(Style::default().fg(TEXT_MUTED));
        frame.render_widget(placeholder, inner);
    } else {
        let input = Paragraph::new(format!(" {}", app.input))
            .style(Style::default().fg(TEXT_PRIMARY));
        frame.render_widget(input, inner);
    }

    // Show cursor
    if app.input_focused {
        frame.set_cursor_position(Position::new(
            inner.x + 1 + app.input.len() as u16,
            inner.y,
        ));
    }
}
