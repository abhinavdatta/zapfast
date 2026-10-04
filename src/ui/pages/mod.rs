//! Sidebar pages beyond the chat list: Status, Channels, and Communities.
//!
//! Each page keeps WhatsApp's two-pane rhythm: a roster beside the rail and
//! an explanation panel in the conversation's place. Roster rows open real
//! conversations; nothing here invents state the backend does not have.

pub mod channels;
pub mod communities;
pub mod status;

use egui::{Frame, Margin};

use crate::app::App;
use crate::model::Action;
use crate::theme::{self, Icon};
use crate::ui::focus::TabStop;
use crate::ui::{keys, widgets};

/// Shared roster panel: the phone's second column beside the rail.
pub(crate) fn roster(
    app: &mut App,
    ui: &mut egui::Ui,
    title: &str,
    body: fn(&mut App, &mut egui::Ui),
) {
    let palette = app.palette;
    let width = app.settings.sidebar_width.clamp(260.0, 520.0);
    egui::Panel::left("page-roster")
        .exact_size(width)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.panel).inner_margin(Margin::ZERO))
        .show(ui, |ui| {
            header(app, ui, title);
            egui::ScrollArea::vertical()
                .id_salt(egui::Id::new(format!("page-roster-{title}")))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    body(app, ui);
                });
        });
    let rect = ui.max_rect();
    ui.painter().vline(
        rect.left(),
        rect.y_range(),
        egui::Stroke::new(1.0, palette.outline),
    );
}

/// Draws the central pane beside the roster with the page's background.
pub(crate) fn central(
    app: &mut App,
    ui: &mut egui::Ui,
    body: impl FnOnce(&mut App, &mut egui::Ui),
) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(app.palette.panel))
        .show(ui, |ui| body(app, ui));
}

/// Page header with the back button and title, over the macOS title bar
/// when the rail leaves no room for traffic lights.
fn header(app: &mut App, ui: &mut egui::Ui, title: &str) {
    let palette = app.palette;
    if theme::macos_chrome(ui.ctx()) {
        let inset = theme::traffic_light_inset(ui.ctx());
        let mut drag = ui.max_rect();
        drag.min.x += inset;
        drag.max.y = drag.min.y + 60.0;
        super::titlebar_drag(ui, drag);
    }
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 14,
            top: 12,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if theme::icon_button(
                    ui,
                    Icon::ArrowLeft,
                    18.0,
                    palette.secondary,
                    palette.text,
                    "Back to chats",
                )
                .tab_stop(keys::Stop::Back)
                .clicked()
                {
                    app.actions.push(Action::Open(crate::model::Page::Chats));
                }
                theme::text(ui, title, theme::bold(20.0), palette.text);
            });
        });
    ui.add_space(4.0);
}

/// The explanation panel that fills the conversation's place.
pub(crate) fn explainer(app: &App, ui: &mut egui::Ui, icon: Icon, title: &str, body: &str) {
    let palette = app.palette;
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.28);
        theme::icon(ui, icon, 52.0, palette.dim);
        ui.add_space(14.0);
        theme::text(ui, title, theme::semibold(19.0), palette.text);
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(body)
                    .font(theme::regular(13.5))
                    .color(palette.secondary),
            )
            .wrap()
            .selectable(false),
        );
    });
}

/// Opens a chat from a roster row, keeping the page when the row is read-only.
pub(crate) fn open_chat(app: &mut App, chat_id: &str) {
    app.actions.push(Action::OpenChat(chat_id.to_owned()));
}

/// Lays a single-line subtitle out inside `max_width`, ending past the width
/// with an ellipsis, like the chat list's own summaries.
fn truncate_galley(
    ui: &egui::Ui,
    text: &str,
    max_width: f32,
    color: egui::Color32,
) -> (std::sync::Arc<egui::Galley>, bool) {
    let font = theme::regular(12.5);
    let mut candidate = text.to_owned();
    let graphemes: Vec<&str> =
        unicode_segmentation::UnicodeSegmentation::graphemes(text, true).collect();
    let mut width = ui
        .painter()
        .layout_no_wrap(candidate.clone(), font.clone(), color)
        .size()
        .x;
    while width > max_width && candidate.chars().count() > 1 {
        let keep = candidate.chars().count().saturating_sub(4).max(1);
        candidate = graphemes[..keep.min(graphemes.len())].concat();
        candidate.push('…');
        width = ui
            .painter()
            .layout_no_wrap(candidate.clone(), font.clone(), color)
            .size()
            .x;
    }
    (
        ui.painter().layout_no_wrap(candidate, font, color),
        width <= max_width,
    )
}

/// One roster row: avatar, title, subtitle, trailing timestamp or badge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn row(
    app: &mut App,
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    subtitle: &str,
    timestamp: Option<&str>,
    badge: u32,
    muted: bool,
) {
    let palette = app.palette;
    let height = theme::ROW_HEIGHT;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let hovered = response.hovered();
    if ui.is_rect_visible(rect) {
        if hovered {
            ui.painter().rect_filled(rect, 0.0, palette.surface_hover);
        }
        let name = title;
        let picture = app.avatar(id);
        widgets::paint_avatar(
            ui,
            &palette,
            egui::Rect::from_center_size(
                egui::pos2(rect.left() + 36.0, rect.center().y),
                egui::Vec2::splat(38.0),
            ),
            name,
            id,
            picture.as_deref(),
        );
        let text_left = rect.left() + 72.0;
        let text_right = rect.right() - 16.0;
        ui.painter().text(
            egui::pos2(text_left, rect.top() + 20.0),
            egui::Align2::LEFT_CENTER,
            name,
            theme::medium(14.5),
            palette.text,
        );
        let subtitle_color = if badge > 0 {
            palette.text
        } else {
            palette.secondary
        };
        let laid =
            ui.painter()
                .layout_no_wrap(subtitle.to_owned(), theme::regular(12.5), subtitle_color);
        let max_subtitle = (text_right - text_left - if badge > 0 { 34.0 } else { 0.0 }).max(20.0);
        let galley = if laid.size().x <= max_subtitle {
            laid
        } else {
            truncate_galley(ui, subtitle, max_subtitle, subtitle_color).0
        };
        ui.painter().galley(
            egui::pos2(text_left, rect.bottom() - 18.0 - galley.size().y / 2.0),
            galley,
            subtitle_color,
        );
        if let Some(timestamp) = timestamp {
            ui.painter().text(
                egui::pos2(text_right, rect.top() + 20.0),
                egui::Align2::RIGHT_CENTER,
                timestamp,
                theme::regular(12.0),
                if badge > 0 {
                    palette.accent
                } else {
                    palette.secondary
                },
            );
        }
        if muted {
            Icon::BellOff.image(palette.secondary, 14.0).paint_at(
                ui,
                egui::Rect::from_center_size(
                    egui::pos2(text_right, rect.bottom() - 18.0),
                    egui::Vec2::splat(16.0),
                ),
            );
        }
        if badge > 0 {
            let center = egui::pos2(
                text_right - if muted { 22.0 } else { 0.0 },
                rect.bottom() - 18.0,
            );
            ui.painter().circle_filled(center, 9.0, palette.accent);
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                if badge > 99 {
                    "99+".to_owned()
                } else {
                    badge.to_string()
                },
                theme::medium(10.5),
                palette.on_accent,
            );
        }
    }
    if response.clicked() {
        open_chat(app, id);
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(("page-row", id)), rect);
    });
}
