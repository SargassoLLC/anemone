//! Multi-anemone tab switcher with accent styling.

use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Tabs};

use crate::app::App;
use super::{ACCENT, ACCENT_DIM, BORDER, TEXT_DIM, TEXT_MUTED, BG};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<String> = app
        .anemones
        .iter()
        .enumerate()
        .map(|(_i, v)| {
            let indicator = match v.state {
                anemone_core::types::BrainState::Thinking => "●",
                anemone_core::types::BrainState::Reflecting => "◎",
                anemone_core::types::BrainState::Planning => "◈",
                anemone_core::types::BrainState::Idle => "○",
            };
            format!(" {} {} ", indicator, v.name)
        })
        .collect();

    if titles.is_empty() {
        let empty = Paragraph::new(" 🪸 anemone")
            .style(Style::default().fg(ACCENT_DIM).bg(BG));
        frame.render_widget(empty, area);
        return;
    }

    let tabs = Tabs::new(titles)
        .select(app.active_tab)
        .style(Style::default().fg(TEXT_MUTED).bg(BG))
        .highlight_style(Style::default().fg(ACCENT).bold())
        .divider(Span::styled(" │ ", Style::default().fg(BORDER)));

    frame.render_widget(tabs, area);
}
