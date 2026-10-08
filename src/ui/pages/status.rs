//! The Status page: one story per contact, from `status@broadcast` updates.
//!
//! The round-up chat never opens as a conversation; its newest update per
//! author is listed here and clicking plays that author's story.

use crate::app::App;
use crate::model::{Action, StatusEntry};
use crate::theme::{self, Icon};
use crate::ui::widgets;

use super::{central, explainer, roster, showing_conversation};
use crate::ui::conversation;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    roster(app, ui, "Status", body);
    central(app, ui, |app, ui| {
        // An opened conversation from another roster overlays the central
        // pane, like WhatsApp Web keeps the roster beside the opened chat.
        // A story being played takes precedence over that overlay, so a
        // stale open chat can never hide the Status updates.
        if app.status_story.is_none() && showing_conversation(app) {
            conversation::show(app, ui);
            return;
        }
        let entries = app.status_entries();
        match app.status_story.as_ref().map(|story| story.author.clone()) {
            Some(author) => story_view(app, ui, &author, &entries),
            None => {
                let mine = app
                    .me
                    .as_deref()
                    .map(|me| entries.iter().any(|entry| entry.id == me))
                    .unwrap_or(false);
                if mine {
                    explainer(
                        app,
                        ui,
                        Icon::CircleDashed,
                        "Share statuses",
                        "Share photos, videos and text that disappear after 24 hours.",
                    );
                } else {
                    explainer(
                        app,
                        ui,
                        Icon::CircleDashed,
                        "No recent updates",
                        "Status updates from your contacts appear here.",
                    );
                }
            }
        }
    });
}

fn body(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let entries = app.status_entries();
    if entries.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::CircleDashed,
            "No recent updates",
            "Status updates from your contacts appear here.",
        );
        return;
    }
    let me = app.me.clone().unwrap_or_default();
    let mut mine = None;
    let mut others = Vec::new();
    for entry in entries {
        if entry.id == me {
            mine = Some(entry);
        } else {
            others.push(entry);
        }
    }
    if let Some(entry) = mine {
        section_label(ui, &palette, "My status");
        status_row(app, ui, &entry);
    }
    if !others.is_empty() {
        section_label(ui, &palette, "Recent");
        for entry in others {
            status_row(app, ui, &entry);
        }
    }
}

fn section_label(ui: &mut egui::Ui, palette: &crate::theme::Palette, label: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        theme::text(ui, label, theme::semibold(13.0), palette.secondary);
    });
    ui.add_space(2.0);
}

fn status_row(app: &mut App, ui: &mut egui::Ui, entry: &crate::model::StatusEntry) {
    let palette = app.palette;
    let name = entry
        .name
        .clone()
        .unwrap_or_else(|| crate::util::phone_or_id(&entry.id));
    let height = theme::ROW_HEIGHT;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, palette.surface_hover);
        }
        // Story ring: seen updates draw a gray ring, unseen ones accent.
        let center = egui::pos2(rect.left() + 36.0, rect.center().y);
        let ring = if entry.seen {
            palette.outline
        } else {
            palette.accent
        };
        ui.painter()
            .circle_stroke(center, 22.0, egui::Stroke::new(2.5, ring));
        let picture = app.avatar(&entry.id);
        widgets::paint_avatar(
            ui,
            &palette,
            egui::Rect::from_center_size(center, egui::Vec2::splat(36.0)),
            &name,
            &entry.id,
            picture.as_deref(),
        );
        ui.painter().text(
            egui::pos2(rect.left() + 72.0, rect.top() + 20.0),
            egui::Align2::LEFT_CENTER,
            name,
            theme::medium(14.5),
            palette.text,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 72.0, rect.bottom() - 18.0),
            egui::Align2::LEFT_CENTER,
            crate::util::relative_time(entry.timestamp),
            theme::regular(12.5),
            palette.secondary,
        );
    }
    if response.clicked() {
        let already_playing = app
            .status_story
            .as_ref()
            .is_some_and(|story| story.author == entry.id);
        if already_playing {
            // Clicking the author currently playing closes their story,
            // like WhatsApp's tap-to-dismiss.
            app.close_story();
        } else {
            app.open_story(entry.id.clone());
        }
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(("story-row", &entry.id)), rect);
    });
}

/// Plays one author's story in the central panel.
fn story_view(app: &mut App, ui: &mut egui::Ui, author: &str, entries: &[StatusEntry]) {
    let palette = app.palette;
    let messages: Vec<crate::model::Message> =
        app.status_messages(author).into_iter().cloned().collect();
    if messages.is_empty() {
        app.status_story = None;
        return;
    }
    let index = app
        .status_story
        .as_ref()
        .map(|story| story.index)
        .unwrap_or_default();
    let page = index.min(messages.len() - 1);
    // A click anywhere steps forward and closes at the end, like tapping a
    // story; ctrl+click steps back.
    let (cover, cover_response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
    if cover_response.clicked() {
        if ui.input(|input| input.modifiers.command) {
            app.actions.push(Action::StoryPrev);
        } else {
            app.actions.push(Action::StoryNext);
        }
    }
    let _ = cover;
    let message = &messages[page];
    // Header: back to the list, author, time, page dots.
    ui.horizontal(|ui| {
        if theme::icon_button(
            ui,
            Icon::ChevronLeft,
            18.0,
            palette.secondary,
            palette.text,
            "Back to status updates",
        )
        .clicked()
        {
            app.status_story = None;
            app.actions.push(Action::MarkStatusSeen);
        }
        let name = entries
            .iter()
            .find(|entry| entry.id == author)
            .and_then(|entry| entry.name.clone())
            .unwrap_or_else(|| crate::util::phone_or_id(author));
        theme::text(ui, name, theme::semibold(15.0), palette.text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            theme::text(
                ui,
                crate::util::relative_time(message.timestamp),
                theme::regular(12.0),
                palette.secondary,
            );
            ui.add_space(12.0);
            if messages.len() > 1 {
                ui.horizontal(|ui| {
                    for (index, _) in messages.iter().enumerate() {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                        let color = if index == page {
                            palette.accent
                        } else {
                            palette.dim
                        };
                        ui.painter().rect_filled(rect, 3.0, color);
                    }
                });
            }
        });
    });
    ui.separator();
    // The story body, drawn like a message bubble in the wallpaper view.
    egui::ScrollArea::vertical()
        .id_salt("status-story")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.add_space(24.0);
                crate::ui::conversation::draw_story_message(ui, app, message);
            });
        });
    // Prev/next across an author's updates.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if page + 1 < messages.len()
                && theme::icon_button(
                    ui,
                    Icon::ChevronRight,
                    18.0,
                    palette.secondary,
                    palette.text,
                    "Next update",
                )
                .clicked()
            {
                app.status_story_index(page + 1);
            }
            if page > 0
                && theme::icon_button(
                    ui,
                    Icon::ChevronLeft,
                    18.0,
                    palette.secondary,
                    palette.text,
                    "Previous update",
                )
                .clicked()
            {
                app.status_story_index(page.saturating_sub(1));
            }
        });
    });
}
