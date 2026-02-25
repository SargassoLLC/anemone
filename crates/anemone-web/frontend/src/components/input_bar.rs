//! Input bar component — text input + send button.

use dioxus::prelude::*;

#[derive(Clone, PartialEq, Props)]
pub struct InputBarProps {
    active_id: String,
    conversing: bool,
    countdown: u32,
}

pub fn InputBar(props: InputBarProps) -> Element {
    let mut input_text = use_signal(|| String::new());

    let placeholder = if props.conversing {
        format!("Reply... ({}s)", props.countdown)
    } else {
        "Say something...".to_string()
    };

    let send = move |_| {
        let text = input_text();
        if text.trim().is_empty() {
            return;
        }
        let id = props.active_id.clone();
        input_text.set(String::new());
        spawn(async move {
            let _ = crate::api::send_message(&id, &text).await;
        });
    };

    rsx! {
        div { class: "input-bar",
            input {
                r#type: "text",
                placeholder: "{placeholder}",
                value: "{input_text}",
                oninput: move |e| input_text.set(e.value()),
                onkeypress: move |e| {
                    if e.key() == Key::Enter {
                        let text = input_text();
                        if text.trim().is_empty() {
                            return;
                        }
                        let id = props.active_id.clone();
                        input_text.set(String::new());
                        spawn(async move {
                            let _ = crate::api::send_message(&id, &text).await;
                        });
                    }
                },
            }
            button {
                onclick: send,
                "Send"
            }
        }
    }
}
