//! Multi-anemone tab switcher.

use ratatui::prelude::*;
use ratatui::widgets::Tabs;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<String> = app
        .anemones
        .iter()
        .enumerate()
        .map(|(_i, v)| {
            let indicator = match v.state {
                anemone_core::types::BrainState::Thinking => "*",
                anemone_core::types::BrainState::Reflecting => "~",
                anemone_core::types::BrainState::Planning => "?",
                anemone_core::types::BrainState::Idle => " ",
            };
            format!(" {}{} ", v.name, indicator)
        })
        .collect();

    if titles.is_empty() {
        return;
    }

    let tabs = Tabs::new(titles)
        .select(app.active_tab)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(Style::default().fg(Color::Cyan).bold())
        .divider("|");

    frame.render_widget(tabs, area);
}
