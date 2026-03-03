//! Status bar — rich info with genome snippet, state, thought count.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use anemone_core::types::BrainState;
use crate::app::{AnemoneView, App};
use super::{ACCENT, TEXT_DIM, TEXT_MUTED, BG, GREEN, BLUE, YELLOW, RED};

pub fn draw(frame: &mut Frame, view: &AnemoneView, app: &App, area: Rect) {
    let (state_str, state_icon, state_color) = match view.state {
        BrainState::Idle => ("idle", "◌", TEXT_MUTED),
        BrainState::Thinking => ("thinking", "◉", GREEN),
        BrainState::Reflecting => ("reflecting", "◎", ACCENT),
        BrainState::Planning => ("planning", "◈", BLUE),
    };

    let mut spans = vec![
        Span::styled(" ", Style::default().bg(BG)),
        Span::styled(
            format!(" {} {} ", state_icon, state_str),
            Style::default().fg(Color::Black).bg(state_color).bold(),
        ),
        Span::styled(
            format!("  💭 {} ", view.thought_count),
            Style::default().fg(TEXT_DIM),
        ),
    ];

    // Show genome snippet
    let genome_preview: String = view.id.chars().take(8).collect();
    if !genome_preview.is_empty() {
        spans.push(Span::styled(
            format!(" 🧬 {} ", genome_preview),
            Style::default().fg(TEXT_MUTED),
        ));
    }

    // Activity
    if !view.activity.is_empty() {
        spans.push(Span::styled("│ ", Style::default().fg(TEXT_MUTED)));
        spans.push(Span::styled(
            format!("{} ", view.activity),
            Style::default().fg(YELLOW),
        ));
    }

    let status = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(BG));
    frame.render_widget(status, area);
}
