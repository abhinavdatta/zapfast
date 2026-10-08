//! The Communities page: parent groups with their linked topic groups.

use crate::app::App;
use crate::model::Chat;
use crate::theme::{self, Icon};
use crate::ui::widgets;

use super::{explainer, roster, showing_conversation};
use crate::ui::conversation;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    roster(app, ui, "Communities", body);
    super::central(app, ui, |app, ui| {
        if showing_conversation(app) {
            conversation::show(app, ui);
            return;
        }
        explainer(
            app,
            ui,
            Icon::Users,
            "Bring members together",
            "Communities group related chats in one place. Create or link them on your phone; linked groups appear here.",
        );
    });
}

fn body(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let communities: Vec<Chat> = app
        .chats
        .iter()
        .filter(|chat| chat.is_community() && !chat.locked)
        .cloned()
        .collect();
    if communities.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::Users,
            "No communities",
            "Communities you belong to appear here with their groups.",
        );
        return;
    }
    let now = crate::util::now();
    for community in communities {
        let mut groups: Vec<Chat> = app
            .chats
            .iter()
            .filter(|chat| {
                chat.community_parent()
                    .map(|parent| parent == &community.id)
                    .unwrap_or(false)
                    && !chat.locked
            })
            .cloned()
            .collect();
        groups.sort_by(|a, b| b.last_activity.cmp(&a.last_activity).then(a.id.cmp(&b.id)));
        let unread: u32 = groups.iter().map(|chat| chat.unread).sum();
        community_header(app, ui, &community, groups.len(), unread);
        for group in groups {
            let subtitle = group
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
                .unwrap_or_else(|| "No messages yet".to_owned());
            let timestamp =
                (group.last_activity > 0).then(|| crate::util::list_time(group.last_activity, now));
            let title = app.chat_title(&group);
            super::row(
                app,
                ui,
                &group.id,
                &title,
                &subtitle,
                timestamp.as_deref(),
                group.unread,
                group.muted(now),
            );
        }
        ui.add_space(10.0);
    }
}

/// A community's own header row, opening its announcement group.
fn community_header(
    app: &mut App,
    ui: &mut egui::Ui,
    community: &Chat,
    groups: usize,
    unread: u32,
) {
    let palette = app.palette;
    let height = theme::ROW_HEIGHT + 6.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, palette.surface_hover);
        }
        let picture = app.avatar(&community.id);
        widgets::paint_avatar(
            ui,
            &palette,
            egui::Rect::from_center_size(
                egui::pos2(rect.left() + 36.0, rect.center().y),
                egui::Vec2::splat(38.0),
            ),
            &community.name,
            &community.id,
            picture.as_deref(),
        );
        ui.painter().text(
            egui::pos2(rect.left() + 72.0, rect.center().y - 8.0),
            egui::Align2::LEFT_CENTER,
            app.chat_title(community),
            theme::semibold(15.0),
            palette.text,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 72.0, rect.center().y + 12.0),
            egui::Align2::LEFT_CENTER,
            match groups {
                0 => "Community".to_owned(),
                1 => "1 group".to_owned(),
                groups => format!("{groups} groups"),
            },
            theme::regular(12.5),
            palette.secondary,
        );
        if unread > 0 {
            let center = egui::pos2(rect.right() - 20.0, rect.center().y);
            ui.painter().circle_filled(center, 9.0, palette.accent);
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                if unread > 99 {
                    "99+".to_owned()
                } else {
                    unread.to_string()
                },
                theme::medium(10.5),
                palette.on_accent,
            );
        }
        Icon::Users.image(palette.secondary, 14.0).paint_at(
            ui,
            egui::Rect::from_center_size(
                egui::pos2(rect.right() - 44.0, rect.center().y),
                egui::Vec2::splat(16.0),
            ),
        );
    }
    if response.clicked() {
        super::open_chat(app, &community.id);
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(("community-row", &community.id)), rect);
    });
}
