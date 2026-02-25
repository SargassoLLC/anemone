//! Multi-anemone switcher component.

use dioxus::prelude::*;
use crate::AnemoneInfo;

#[derive(Clone, PartialEq, Props)]
pub struct SwitcherProps {
    anemones: Vec<AnemoneInfo>,
    active_id: String,
    on_switch: EventHandler<String>,
}

pub fn Switcher(props: SwitcherProps) -> Element {
    rsx! {
        div { class: "switcher",
            for anemone in &props.anemones {
                button {
                    class: if anemone.id == props.active_id { "active" } else { "" },
                    onclick: {
                        let id = anemone.id.clone();
                        let on_switch = props.on_switch.clone();
                        move |_| on_switch.call(id.clone())
                    },
                    "{anemone.name}"
                    match anemone.state.as_str() {
                        "thinking" => rsx! { span { " *" } },
                        "reflecting" => rsx! { span { " ~" } },
                        "planning" => rsx! { span { " ?" } },
                        _ => rsx! {},
                    }
                }
            }
        }
    }
}
