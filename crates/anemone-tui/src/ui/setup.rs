//! Setup wizard UI — Claude Code CLI–inspired aesthetic.
//!
//! Visual style: rounded borders (╭╮╰╯), magenta/purple accent,
//! minimal chrome, clean whitespace, `❯` selection indicator.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType, List, ListItem, Paragraph, Wrap};

// ─── Palette (Claude Code–inspired) ──────────────────────────────────────────

const ACCENT: Color = Color::Magenta;        // primary highlight
const ACCENT_DIM: Color = Color::Rgb(150, 100, 200); // softer purple
const TEXT: Color = Color::White;
const DIM: Color = Color::DarkGray;
const SUCCESS: Color = Color::Green;
const ERROR: Color = Color::Red;
const WARN: Color = Color::Yellow;

// ─── Data model ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SetupStep {
    ProviderSelect,
    ApiKeyInput,
    CustomUrlInput,
    Complete,
}

#[derive(Debug, Clone)]
pub enum ValidationStatus {
    None,
    Validating,
    Valid { model: String, latency_ms: u64 },
    Invalid { error: String },
}

#[derive(Debug, Clone)]
pub struct ProviderOption {
    pub name: String,
    pub provider_id: String,
    pub default_model: String,
    pub description: String,
    pub needs_key: bool,
    pub needs_url: bool,
}

#[derive(Debug, Clone)]
pub struct SetupState {
    pub step: SetupStep,
    pub selected_provider: usize,
    pub providers: Vec<ProviderOption>,
    pub key_input: String,
    pub url_input: String,
    pub key_masked: bool,
    pub validation_status: ValidationStatus,
}

impl SetupState {
    pub fn new() -> Self {
        Self {
            step: SetupStep::ProviderSelect,
            selected_provider: 0,
            providers: vec![
                ProviderOption {
                    name: "OpenAI".into(),
                    provider_id: "openai".into(),
                    default_model: "gpt-4.1".into(),
                    description: "GPT-4.1, best general reasoning".into(),
                    needs_key: true,
                    needs_url: false,
                },
                ProviderOption {
                    name: "OpenRouter".into(),
                    provider_id: "openrouter".into(),
                    default_model: "openai/gpt-4o".into(),
                    description: "Multi-model, pay-per-token".into(),
                    needs_key: true,
                    needs_url: false,
                },
                ProviderOption {
                    name: "Ollama".into(),
                    provider_id: "ollama".into(),
                    default_model: "llama3".into(),
                    description: "Run locally, completely free".into(),
                    needs_key: false,
                    needs_url: true,
                },
                ProviderOption {
                    name: "Custom".into(),
                    provider_id: "custom".into(),
                    default_model: "gpt-4o".into(),
                    description: "Any OpenAI-compatible endpoint".into(),
                    needs_key: true,
                    needs_url: true,
                },
            ],
            key_input: String::new(),
            url_input: String::new(),
            key_masked: true,
            validation_status: ValidationStatus::None,
        }
    }

    pub fn selected(&self) -> Option<&ProviderOption> {
        self.providers.get(self.selected_provider)
    }
}

impl Default for SetupState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Layout helpers ──────────────────────────────────────────────────────────

/// Centered sub-rect.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}

/// Standard outer block with rounded borders and magenta accent.
fn wizard_block(title: &str) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_DIM))
        .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
}

// ─── Top-level dispatcher ────────────────────────────────────────────────────

pub fn draw_setup(frame: &mut Frame, area: Rect, state: &Option<SetupState>) {
    // Fill background
    frame.render_widget(Block::default().style(Style::default().bg(Color::Black)), area);

    let state = match state {
        Some(s) => s,
        None => return,
    };
    match state.step {
        SetupStep::ProviderSelect => draw_provider_select(frame, area, state),
        SetupStep::ApiKeyInput => draw_api_key_input(frame, area, state),
        SetupStep::CustomUrlInput => draw_custom_url_input(frame, area, state),
        SetupStep::Complete => draw_setup_complete(frame, area, state),
    }
}

// ─── Screen 1: Provider picker ───────────────────────────────────────────────

pub fn draw_provider_select(frame: &mut Frame, area: Rect, state: &SetupState) {
    let popup = centered_rect(55, 65, area);
    let outer = wizard_block("🪸 anemone");
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // welcome + subtitle
            Constraint::Length(1), // spacer
            Constraint::Min(6),    // list
            Constraint::Length(1), // separator
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Welcome
    let welcome = Paragraph::new(vec![
        Line::from(Span::styled(
            "Welcome! Let's set up your anemone.",
            Style::default().fg(TEXT),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Choose a provider:",
            Style::default().fg(DIM),
        )),
    ]);
    frame.render_widget(welcome, chunks[0]);

    // Provider list with ❯ indicator
    let items: Vec<ListItem> = state
        .providers
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let selected = i == state.selected_provider;
            let indicator = if selected { "❯" } else { " " };
            let line = Line::from(vec![
                Span::styled(
                    format!("{indicator} "),
                    Style::default().fg(if selected { ACCENT } else { DIM }),
                ),
                Span::styled(
                    &p.name,
                    Style::default()
                        .fg(if selected { TEXT } else { DIM })
                        .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(
                    format!("  {}", p.description),
                    Style::default().fg(DIM),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[2]);

    // Thin separator
    let sep = Paragraph::new("─".repeat(inner.width.saturating_sub(2) as usize))
        .style(Style::default().fg(Color::Rgb(60, 60, 60)));
    frame.render_widget(sep, chunks[3]);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", Style::default().fg(ACCENT)),
        Span::styled(" navigate  ", Style::default().fg(DIM)),
        Span::styled("enter", Style::default().fg(ACCENT)),
        Span::styled(" select  ", Style::default().fg(DIM)),
        Span::styled("q", Style::default().fg(ACCENT)),
        Span::styled(" quit", Style::default().fg(DIM)),
    ]));
    frame.render_widget(hint, chunks[4]);
}

// ─── Screen 2: API key input ─────────────────────────────────────────────────

pub fn draw_api_key_input(frame: &mut Frame, area: Rect, state: &SetupState) {
    let popup = centered_rect(55, 55, area);

    let provider_name = state.selected().map(|p| p.name.as_str()).unwrap_or("Unknown");
    let title = format!("🪸 anemone › {}", provider_name.to_lowercase());
    let outer = wizard_block(&title);
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // prompt
            Constraint::Length(1), // spacer
            Constraint::Length(3), // input box
            Constraint::Length(1), // spacer
            Constraint::Length(2), // validation status
            Constraint::Min(0),    // padding
            Constraint::Length(1), // separator
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Prompt
    let prompt = Paragraph::new(Line::from(vec![
        Span::styled("Paste your ", Style::default().fg(TEXT)),
        Span::styled(provider_name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(" API key:", Style::default().fg(TEXT)),
    ]));
    frame.render_widget(prompt, chunks[0]);

    // Masked input field
    let masked = mask_key(&state.key_input);
    let display = if masked.is_empty() {
        Span::styled("sk-...", Style::default().fg(Color::Rgb(60, 60, 60)))
    } else {
        Span::styled(masked, Style::default().fg(TEXT))
    };
    let input_box = Paragraph::new(Line::from(display)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(match &state.validation_status {
                ValidationStatus::Valid { .. } => SUCCESS,
                ValidationStatus::Invalid { .. } => ERROR,
                _ => ACCENT_DIM,
            })),
    );
    frame.render_widget(input_box, chunks[2]);

    // Validation status
    let (status_text, status_color) = match &state.validation_status {
        ValidationStatus::None => ("".into(), DIM),
        ValidationStatus::Validating => ("  ⠋ Validating...".into(), WARN),
        ValidationStatus::Valid { model, latency_ms } => (
            format!("  ✓ Connected — {model} ({latency_ms}ms)"),
            SUCCESS,
        ),
        ValidationStatus::Invalid { error } => (
            format!("  ✗ {error}"),
            ERROR,
        ),
    };
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(status_color))
        .wrap(Wrap { trim: true });
    frame.render_widget(status, chunks[4]);

    // Separator
    let sep = Paragraph::new("─".repeat(inner.width.saturating_sub(2) as usize))
        .style(Style::default().fg(Color::Rgb(60, 60, 60)));
    frame.render_widget(sep, chunks[6]);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("enter", Style::default().fg(ACCENT)),
        Span::styled(" validate  ", Style::default().fg(DIM)),
        Span::styled("esc", Style::default().fg(ACCENT)),
        Span::styled(" back", Style::default().fg(DIM)),
    ]));
    frame.render_widget(hint, chunks[7]);
}

/// Mask key — dots + last 4 chars.
fn mask_key(key: &str) -> String {
    let len = key.len();
    if len == 0 {
        return String::new();
    }
    if len <= 4 {
        return "•".repeat(len);
    }
    let tail: String = key.chars().skip(len - 4).collect();
    format!("{}{}", "•".repeat(len - 4), tail)
}

// ─── Screen 3: Base URL input ────────────────────────────────────────────────

pub fn draw_custom_url_input(frame: &mut Frame, area: Rect, state: &SetupState) {
    let popup = centered_rect(55, 50, area);

    let provider_name = state.selected().map(|p| p.name.as_str()).unwrap_or("Custom");
    let title = format!("🪸 anemone › {}", provider_name.to_lowercase());
    let outer = wizard_block(&title);
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // prompt
            Constraint::Length(1), // example
            Constraint::Length(1), // spacer
            Constraint::Length(3), // url input box
            Constraint::Min(0),    // padding
            Constraint::Length(1), // separator
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Prompt
    let prompt = Paragraph::new("Enter the base URL:")
        .style(Style::default().fg(TEXT));
    frame.render_widget(prompt, chunks[0]);

    // Example
    let example = match state.selected().map(|p| p.provider_id.as_str()) {
        Some("ollama") => "http://localhost:11434",
        _ => "https://api.example.com/v1",
    };
    let example_text = Paragraph::new(format!("e.g. {example}"))
        .style(Style::default().fg(Color::Rgb(80, 80, 80)));
    frame.render_widget(example_text, chunks[1]);

    // URL input
    let display = if state.url_input.is_empty() {
        Span::styled(example, Style::default().fg(Color::Rgb(60, 60, 60)))
    } else {
        Span::styled(&state.url_input, Style::default().fg(TEXT))
    };
    let url_box = Paragraph::new(Line::from(display)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT_DIM)),
    );
    frame.render_widget(url_box, chunks[3]);

    // Separator
    let sep = Paragraph::new("─".repeat(inner.width.saturating_sub(2) as usize))
        .style(Style::default().fg(Color::Rgb(60, 60, 60)));
    frame.render_widget(sep, chunks[5]);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("enter", Style::default().fg(ACCENT)),
        Span::styled(" confirm  ", Style::default().fg(DIM)),
        Span::styled("esc", Style::default().fg(ACCENT)),
        Span::styled(" back", Style::default().fg(DIM)),
    ]));
    frame.render_widget(hint, chunks[6]);
}

// ─── Screen 4: Setup complete ────────────────────────────────────────────────

pub fn draw_setup_complete(frame: &mut Frame, area: Rect, state: &SetupState) {
    let popup = centered_rect(55, 45, area);

    let outer = Block::default()
        .title(" 🪸 anemone ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SUCCESS))
        .title_style(Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD));

    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // checkmark
            Constraint::Length(1), // spacer
            Constraint::Length(1), // provider line
            Constraint::Length(1), // model line
            Constraint::Length(1), // spacer
            Constraint::Min(0),    // padding
            Constraint::Length(1), // separator
            Constraint::Length(1), // next step
        ])
        .split(inner);

    // Success
    let success = Paragraph::new(Line::from(vec![
        Span::styled("✓ ", Style::default().fg(SUCCESS)),
        Span::styled(
            "Configuration saved",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(success, chunks[0]);

    // Provider + model summary
    if let Some(p) = state.selected() {
        let provider_line = Paragraph::new(Line::from(vec![
            Span::styled("  provider  ", Style::default().fg(DIM)),
            Span::styled(&p.name, Style::default().fg(TEXT)),
        ]));
        frame.render_widget(provider_line, chunks[2]);

        let model_line = Paragraph::new(Line::from(vec![
            Span::styled("  model     ", Style::default().fg(DIM)),
            Span::styled(&p.default_model, Style::default().fg(TEXT)),
        ]));
        frame.render_widget(model_line, chunks[3]);
    }

    // Separator
    let sep = Paragraph::new("─".repeat(inner.width.saturating_sub(4) as usize))
        .style(Style::default().fg(Color::Rgb(60, 60, 60)));
    frame.render_widget(sep, chunks[6]);

    // Next step
    let next = Paragraph::new(Line::from(vec![
        Span::styled("Creating your first anemone", Style::default().fg(ACCENT)),
        Span::styled(" ...", Style::default().fg(DIM)),
    ]));
    frame.render_widget(next, chunks[7]);
}
