//! Scrollable chat feed — shows thoughts, tool calls, and results.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{AnemoneView, ChatSide, Phase};

pub fn draw(frame: &mut Frame, view: &AnemoneView, area: Rect) {
    let block = Block::default()
        .title(" Thoughts ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if view.messages.is_empty() {
        let empty = Paragraph::new("Waiting for thoughts...")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, inner);
        return;
    }

    // Build display lines from messages (bottom-up with scroll offset)
    let visible_height = inner.height as usize;
    let total = view.messages.len();
    let end = total.saturating_sub(view.scroll_offset);
    let start = end.saturating_sub(visible_height * 2); // overshoot for wrapping

    let mut lines: Vec<Line> = Vec::new();
    for msg in &view.messages[start..end] {
        let (fg, prefix) = match (&msg.side, &msg.phase) {
            (ChatSide::Right, Phase::Reflection) => (Color::Magenta, "~ "),
            (ChatSide::Right, Phase::Planning) => (Color::Blue, "? "),
            (ChatSide::Right, _) => (Color::Green, "> "),
            (ChatSide::Left, _) => (Color::Yellow, "< "),
            (ChatSide::System, _) => (Color::DarkGray, "  "),
        };

        // Truncate long messages for display
        let display: String = msg.text.chars().take(500).collect();
        for line in display.lines() {
            lines.push(Line::styled(
                format!("{}{}", prefix, line),
                Style::default().fg(fg),
            ));
        }
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}
