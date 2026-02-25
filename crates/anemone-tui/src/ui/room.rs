//! ASCII/Unicode room visualization.
//! Maps the 12x12 tile grid to a compact text display.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use anemone_core::types::ROOM_LOCATIONS;
use crate::app::AnemoneView;

/// Room tiles represented as simple characters.
const ROOM_ART: [&str; 12] = [
    "############",
    "#..........#",
    "#.[]..[]...#",
    "#..........#",
    "#....##....#",
    "#....##....#",
    "#..........#",
    "#.[]..[]...#",
    "#..........#",
    "#..........#",
    "#.....D....#",
    "############",
];

pub fn draw(frame: &mut Frame, view: &AnemoneView, area: Rect) {
    let block = Block::default()
        .title(format!(" {} ", view.name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build room lines with character position
    let mut lines: Vec<Line> = Vec::new();
    let px = view.position.x as usize;
    let py = view.position.y as usize;

    for (y, row) in ROOM_ART.iter().enumerate() {
        let mut spans: Vec<Span> = Vec::new();
        // Add leading space for centering
        spans.push(Span::raw(" "));

        for (x, ch) in row.chars().enumerate() {
            if x == px && y == py {
                spans.push(Span::styled("@", Style::default().fg(Color::Yellow).bold()));
            } else {
                let style = match ch {
                    '#' => Style::default().fg(Color::DarkGray),
                    '.' => Style::default().fg(Color::Rgb(40, 40, 40)),
                    '[' | ']' => Style::default().fg(Color::Rgb(80, 60, 40)),
                    'D' => Style::default().fg(Color::Rgb(139, 90, 43)),
                    _ => Style::default().fg(Color::DarkGray),
                };
                spans.push(Span::styled(
                    if ch == '.' { " " } else { &row[x..x + 1] },
                    style,
                ));
            }
        }
        lines.push(Line::from(spans));
    }

    // Add location label
    let location = find_location(px, py);
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(" {}", location),
        Style::default().fg(Color::Cyan).italic(),
    ));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn find_location(x: usize, y: usize) -> &'static str {
    for &(name, lx, ly) in ROOM_LOCATIONS {
        if lx as usize == x && ly as usize == y {
            return name;
        }
    }
    // Find nearest named location
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
