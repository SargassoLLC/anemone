//! Canvas-based pixel-art game world.
//! Uses web_sys for Canvas 2D drawing, matching the React GameWorld.tsx.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::Position;

const COLS: u32 = 12;
const ROWS: u32 = 12;
const TILE: u32 = 32;
const W: u32 = COLS * TILE;
const H: u32 = ROWS * TILE;

#[derive(Clone, PartialEq, Props)]
pub struct GameWorldProps {
    position: Position,
    state: String,
    activity: String,
    name: String,
}

pub fn GameWorld(props: GameWorldProps) -> Element {
    let canvas_id = "game-canvas";

    // Redraw whenever position or state changes
    use_effect({
        let position = props.position.clone();
        let state = props.state.clone();
        let activity = props.activity.clone();
        let name = props.name.clone();
        move || {
            draw_room(canvas_id, &position, &state, &activity, &name);
        }
    });

    rsx! {
        div { class: "game-panel",
            canvas {
                id: canvas_id,
                width: "{W}",
                height: "{H}",
                style: "width: {W}px; height: {H}px;",
            }
            div { class: "status-bar",
                span { "{props.name} " }
                span {
                    style: "color: {state_color(&props.state)};",
                    "[{props.state}]"
                }
                if !props.activity.is_empty() {
                    span { " {props.activity}" }
                }
            }
        }
    }
}

fn state_color(state: &str) -> &'static str {
    match state {
        "thinking" => "#22c55e",
        "reflecting" => "#a855f7",
        "planning" => "#3b82f6",
        _ => "#64748b",
    }
}

/// Collision map rows (1=blocked)
const COLLISION: [[u8; 12]; 12] = [
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
];

fn draw_room(canvas_id: &str, pos: &Position, state: &str, activity: &str, name: &str) {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let document = match window.document() {
        Some(d) => d,
        None => return,
    };
    let canvas: HtmlCanvasElement = match document.get_element_by_id(canvas_id) {
        Some(el) => match el.dyn_into() {
            Ok(c) => c,
            Err(_) => return,
        },
        None => return,
    };
    let ctx: CanvasRenderingContext2d = match canvas
        .get_context("2d")
        .ok()
        .flatten()
    {
        Some(c) => c.dyn_into().unwrap(),
        None => return,
    };

    // Clear
    ctx.set_fill_style_str("#0f172a");
    ctx.fill_rect(0.0, 0.0, W as f64, H as f64);

    // Draw floor tiles
    for row in 0..ROWS {
        for col in 0..COLS {
            let x = (col * TILE) as f64;
            let y = (row * TILE) as f64;

            if COLLISION[row as usize][col as usize] == 1 {
                // Wall
                ctx.set_fill_style_str("#1e293b");
                ctx.fill_rect(x, y, TILE as f64, TILE as f64);
                ctx.set_stroke_style_str("#334155");
                ctx.set_line_width(0.5);
                ctx.stroke_rect(x, y, TILE as f64, TILE as f64);
            } else {
                // Floor
                ctx.set_fill_style_str("#0f172a");
                ctx.fill_rect(x, y, TILE as f64, TILE as f64);
                ctx.set_stroke_style_str("#1e293b");
                ctx.set_line_width(0.5);
                ctx.stroke_rect(x, y, TILE as f64, TILE as f64);
            }
        }
    }

    // Draw furniture placeholders
    draw_furniture(&ctx);

    // Draw character
    let cx = pos.x as f64 * TILE as f64 + TILE as f64 / 2.0;
    let cy = pos.y as f64 * TILE as f64 + TILE as f64 / 2.0;

    // Character body (circle)
    let color = match state {
        "thinking" => "#22c55e",
        "reflecting" => "#a855f7",
        "planning" => "#3b82f6",
        _ => "#f59e0b",
    };
    ctx.begin_path();
    let _ = ctx.arc(cx, cy, 12.0, 0.0, std::f64::consts::PI * 2.0);
    ctx.set_fill_style_str(color);
    ctx.fill();

    // Eyes
    ctx.set_fill_style_str("#0f172a");
    ctx.fill_rect(cx - 4.0, cy - 3.0, 3.0, 3.0);
    ctx.fill_rect(cx + 1.0, cy - 3.0, 3.0, 3.0);

    // Name label
    ctx.set_fill_style_str("#94a3b8");
    ctx.set_font("10px monospace");
    ctx.set_text_align("center");
    let _ = ctx.fill_text(name, cx, cy - 18.0);

    // Activity indicator
    if !activity.is_empty() {
        ctx.set_fill_style_str("#475569");
        ctx.set_font("9px monospace");
        let _ = ctx.fill_text(activity, cx, cy + 22.0);
    }
}

fn draw_furniture(ctx: &CanvasRenderingContext2d) {
    // Desk (top-right area)
    ctx.set_fill_style_str("#78350f");
    ctx.fill_rect(10.0 * 32.0, 1.0 * 32.0, 28.0, 28.0);
    ctx.set_fill_style_str("#92400e");
    ctx.fill_rect(10.0 * 32.0 + 2.0, 1.0 * 32.0 + 2.0, 24.0, 10.0);

    // Bookshelf (left side, row 2)
    ctx.set_fill_style_str("#78350f");
    ctx.fill_rect(1.0 * 32.0 + 2.0, 2.0 * 32.0 + 2.0, 28.0, 28.0);
    // Book spines
    let colors = ["#ef4444", "#3b82f6", "#22c55e", "#f59e0b"];
    for (i, c) in colors.iter().enumerate() {
        ctx.set_fill_style_str(c);
        ctx.fill_rect(
            1.0 * 32.0 + 5.0 + i as f64 * 6.0,
            2.0 * 32.0 + 4.0,
            4.0,
            24.0,
        );
    }

    // Table (center, rows 4-5)
    ctx.set_fill_style_str("#44403c");
    ctx.fill_rect(4.0 * 32.0, 4.0 * 32.0, 64.0, 64.0);
    ctx.set_fill_style_str("#57534e");
    ctx.fill_rect(4.0 * 32.0 + 4.0, 4.0 * 32.0 + 4.0, 56.0, 56.0);

    // Bed (left side, row 7)
    ctx.set_fill_style_str("#1e3a5f");
    ctx.fill_rect(1.0 * 32.0 + 2.0, 7.0 * 32.0 + 2.0, 28.0, 28.0);
    ctx.set_fill_style_str("#e2e8f0");
    ctx.fill_rect(1.0 * 32.0 + 4.0, 7.0 * 32.0 + 4.0, 24.0, 12.0);

    // Window (right side, rows 7)
    ctx.set_fill_style_str("#0ea5e9");
    ctx.fill_rect(5.0 * 32.0 + 2.0, 7.0 * 32.0 + 2.0, 28.0, 28.0);
    ctx.set_stroke_style_str("#334155");
    ctx.set_line_width(2.0);
    ctx.stroke_rect(5.0 * 32.0 + 2.0, 7.0 * 32.0 + 2.0, 28.0, 28.0);
    // Window cross
    ctx.begin_path();
    ctx.move_to(5.0 * 32.0 + 16.0, 7.0 * 32.0 + 2.0);
    ctx.line_to(5.0 * 32.0 + 16.0, 7.0 * 32.0 + 30.0);
    ctx.move_to(5.0 * 32.0 + 2.0, 7.0 * 32.0 + 16.0);
    ctx.line_to(5.0 * 32.0 + 30.0, 7.0 * 32.0 + 16.0);
    ctx.stroke();

    // Door (bottom, col 5)
    ctx.set_fill_style_str("#92400e");
    ctx.fill_rect(5.0 * 32.0 + 4.0, 10.0 * 32.0 + 2.0, 24.0, 28.0);
    ctx.set_fill_style_str("#fbbf24");
    ctx.begin_path();
    let _ = ctx.arc(5.0 * 32.0 + 22.0, 10.0 * 32.0 + 16.0, 3.0, 0.0, std::f64::consts::PI * 2.0);
    ctx.fill();
}
