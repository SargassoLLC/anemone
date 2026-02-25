//! Status bar — shows state, thought count, activity.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use anemone_core::types::BrainState;
use crate::app::AnemoneView;

pub fn draw(frame: &mut Frame, view: &AnemoneView, area: Rect) {
    let state_str = match view.state {
        BrainState::Idle => "idle",
        BrainState::Thinking => "thinking",
        BrainState::Reflecting => "reflecting",
        BrainState::Planning => "planning",
    };

    let state_color = match view.state {
        BrainState::Idle => Color::DarkGray,
        BrainState::Thinking => Color::Green,
        BrainState::Reflecting => Color::Magenta,
        BrainState::Planning => Color::Blue,
    };

    let mut spans = vec![
        Span::styled(
            format!(" {} ", state_str),
            Style::default().fg(Color::Black).bg(state_color),
        ),
        Span::raw(format!(" thoughts: {} ", view.thought_count)),
    ];

    if !view.activity.is_empty() {
        spans.push(Span::styled(
            format!(" {} ", view.activity),
            Style::default().fg(Color::Yellow),
        ));
    }

    let status = Paragraph::new(Line::from(spans));
    frame.render_widget(status, area);
}
