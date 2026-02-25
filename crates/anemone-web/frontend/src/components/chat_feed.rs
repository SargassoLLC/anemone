//! Chat feed component — displays messages.

use dioxus::prelude::*;
use crate::ChatMsg;

#[derive(Clone, PartialEq, Props)]
pub struct ChatFeedProps {
    messages: Vec<ChatMsg>,
}

pub fn ChatFeed(props: ChatFeedProps) -> Element {
    rsx! {
        div { class: "messages",
            for (i, msg) in props.messages.iter().enumerate() {
                div {
                    key: "{i}",
                    class: format_args!("msg {} {}", msg.side, msg.phase),
                    "{msg.text}"
                }
            }
            div { id: "chat-bottom" }
        }
    }
}
