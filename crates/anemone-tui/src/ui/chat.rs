//! Scrollable chat feed — shows thoughts, tool calls, and results.
//! Design: word-wrapped, colored by phase, with subtle separators.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::app::{AnemoneView, ChatSide, Phase};
use super::{ACCENT, BORDER, TEXT_PRIMARY, TEXT_DIM, TEXT_MUTED, BG, GREEN, BLUE, YELLOW, CYAN, RED};

pub fn draw(frame: &mut Frame, view: &AnemoneView, area: Rect) {
    let block = Block::default()
        .title(Span::styled(" Thoughts ", Style::default().fg(ACCENT).bold()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if view.messages.is_empty() {
        let empty = Paragraph::new("  Waiting for first thought... 🪸")
            .style(Style::default().fg(TEXT_MUTED));
        frame.render_widget(empty, inner);
        return;
    }

    // Build display lines from messages (bottom-up with scroll offset)
    let visible_height = inner.height as usize;
    let total = view.messages.len();
    let end = total.saturating_sub(view.scroll_offset);
    let start = end.saturating_sub(visible_height * 3); // overshoot for wrapping

    let mut lines: Vec<Line> = Vec::new();
    for (i, msg) in view.messages[start..end].iter().enumerate() {
        let (fg, prefix, label) = match (&msg.side, &msg.phase) {
            (ChatSide::Right, Phase::Reflection) => (ACCENT, "  ~ ", Some("reflect")),
            (ChatSide::Right, Phase::Planning) => (BLUE, "  ? ", Some("plan")),
            (ChatSide::Right, _) => (GREEN, "  ❯ ", None),
            (ChatSide::Left, _) => (TEXT_DIM, "  ◁ ", None),  // tool results: dim, no label
            (ChatSide::System, _) => (TEXT_MUTED, "  · ", None),
        };

        // Tool results: collapse to single truncated line
        if msg.side == ChatSide::Left {
            let first_line = msg.text.lines().next().unwrap_or("");
            let max = inner.width.saturating_sub(8) as usize;
            let display: String = if first_line.len() > max {
                format!("{}…", &first_line[..max.min(first_line.len())])
            } else {
                first_line.to_string()
            };
            lines.push(Line::from(vec![
                Span::styled("  ◁ ", Style::default().fg(TEXT_MUTED)),
                Span::styled(display, Style::default().fg(TEXT_DIM)),
            ]));
            continue;
        }

        // Phase label on first line
        if let Some(label) = label {
            lines.push(Line::from(vec![
                Span::styled(format!("  ┌ {}", label), Style::default().fg(fg).dim()),
            ]));
        }

        // Word-wrap content
        let width = inner.width.saturating_sub(5) as usize;
        for line in msg.text.lines() {
            let prefixed = format!("{}{}", prefix, line);
            if prefixed.len() <= width + prefix.len() || width == 0 {
                lines.push(Line::from(vec![
                    Span::styled(prefix.to_string(), Style::default().fg(fg).dim()),
                    Span::styled(line.to_string(), Style::default().fg(fg)),
                ]));
            } else {
                // Manual word wrap
                let words: Vec<&str> = line.split_whitespace().collect();
                let mut current = String::new();
                let mut first = true;
                for word in words {
                    if current.len() + word.len() + 1 > width && !current.is_empty() {
                        if first {
                            lines.push(Line::from(vec![
                                Span::styled(prefix.to_string(), Style::default().fg(fg).dim()),
                                Span::styled(current, Style::default().fg(fg)),
                            ]));
                            first = false;
                        } else {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default()),
                                Span::styled(current, Style::default().fg(fg)),
                            ]));
                        }
                        current = word.to_string();
                    } else {
                        if !current.is_empty() {
                            current.push(' ');
                        }
                        current.push_str(word);
                    }
                }
                if !current.is_empty() {
                    if first {
                        lines.push(Line::from(vec![
                            Span::styled(prefix.to_string(), Style::default().fg(fg).dim()),
                            Span::styled(current, Style::default().fg(fg)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default()),
                            Span::styled(current, Style::default().fg(fg)),
                        ]));
                    }
                }
            }
        }

        // Separator between messages
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
