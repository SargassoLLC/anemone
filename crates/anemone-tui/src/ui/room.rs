//! ASCII room visualization — cozy, minimal, with personality.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use anemone_core::types::ROOM_LOCATIONS;
use crate::app::AnemoneView;
use super::{ACCENT, ACCENT_DIM, BORDER, TEXT_DIM, TEXT_MUTED, BG, YELLOW, CYAN};

/// Room layout — compact and cozy
const ROOM_ART: [&str; 12] = [
    "╭──────────╮",
    "│          │",
    "│ ▫▫  ▫▫  │",
    "│          │",
    "│   ░░░░   │",
    "│   ░░░░   │",
    "│          │",
    "│ ▫▫  ▫▫  │",
    "│          │",
    "│          │",
    "│     ◇    │",
    "╰──────────╯",
];

pub fn draw(frame: &mut Frame, view: &AnemoneView, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", view.name),
            Style::default().fg(ACCENT).bold(),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let px = view.position.x as usize;
    let py = view.position.y as usize;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from("")); // top padding

    for (y, row) in ROOM_ART.iter().enumerate() {
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::raw("  ")); // left padding

        for (x, ch) in row.chars().enumerate() {
            if x == px && y == py {
                // The anemone! 🪸
                spans.push(Span::styled("@", Style::default().fg(ACCENT).bold()));
            } else {
                let style = match ch {
                    '╭' | '╮' | '╰' | '╯' | '│' | '─' => Style::default().fg(TEXT_MUTED),
                    '░' => Style::default().fg(Color::Rgb(35, 35, 45)), // rug
                    '▫' => Style::default().fg(Color::Rgb(70, 55, 40)), // furniture
                    '◇' => Style::default().fg(Color::Rgb(100, 70, 40)), // door
                    _ => Style::default().fg(Color::Rgb(25, 25, 32)), // floor
                };
                spans.push(Span::styled(
                    ch.to_string(),
                    style,
                ));
            }
        }
        lines.push(Line::from(spans));
    }

    // Location label
    let location = find_location(px, py);
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  📍 ", Style::default().fg(TEXT_MUTED)),
        Span::styled(location, Style::default().fg(CYAN).italic()),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn find_location(x: usize, y: usize) -> &'static str {
    for &(name, lx, ly) in ROOM_LOCATIONS {
        if lx as usize == x && ly as usize == y {
            return name;
        }
    }
    let mut best = "room";
    let mut best_dist = u32::MAX;
    for &(name, lx, ly) in ROOM_LOCATIONS {
        let dx = (x as i32 - lx as i32).unsigned_abs();
        let dy = (y as i32 - ly as i32).unsigned_abs();
        let dist = dx + dy;
        if dist < best_dist {
            best_dist = dist;
            best = name;
        }
    }
    best
}
