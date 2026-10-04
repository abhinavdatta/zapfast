//! The Channels page: followed newsletters with their newest posts.

use crate::app::App;
use crate::model::Chat;
use crate::theme::Icon;
use crate::ui::widgets;

use super::{explainer, roster};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    roster(app, ui, "Channels", body);
    super::central(app, ui, |app, ui| {
        explainer(
            app,
            ui,
            Icon::Megaphone,
            "Stay in the loop",
            "Followed channels post updates here. Channel publishing is not supported in ZapFast.",
        );
    });
}

fn body(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let mut channels: Vec<Chat> = app
        .chats
        .iter()
        .filter(|chat| chat.is_channel() && !chat.locked)
        .cloned()
        .collect();
    if channels.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::Megaphone,
            "No channels yet",
            "Channels you follow appear here, out of the main chat list.",
        );
        return;
    }
    channels.sort_by(|a, b| b.last_activity.cmp(&a.last_activity).then(a.id.cmp(&b.id)));
    let now = crate::util::now();
    for chat in channels {
        let subtitle = chat
            .last
            .as_ref()
            .map(|last| {
                let who = if last.from_me {
                    "You: ".to_owned()
                } else {
                    last.sender_name
                        .as_deref()
                        .map(|name| format!("{name}: "))
                        .unwrap_or_default()
                };
                format!("{who}{}", last.summary)
            })
            .unwrap_or_else(|| "No posts yet".to_owned());
        let timestamp =
            (chat.last_activity > 0).then(|| crate::util::list_time(chat.last_activity, now));
        let title = app.chat_title(&chat);
        super::row(
            app,
            ui,
            &chat.id,
            &title,
            &subtitle,
            timestamp.as_deref(),
            chat.unread,
            chat.muted(now),
        );
    }
}
