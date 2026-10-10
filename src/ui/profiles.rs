//! The profile gate: a wall of profiles shown at app start, like browser or
//! streaming profiles. Each linked account is a profile; picking a pinned one
//! asks for its password before showing its chats.

use egui::{Align, Layout, Vec2};

use crate::app::App;
use crate::i18n::gettext;
use crate::model::{AccountId, Action};
use crate::theme::{self, Icon};

use super::widgets;

/// The password field's id, which takes the keyboard as the ask appears.
pub const PASSWORD_ID: &str = "profile-password";

/// One profile on the wall, as the switcher shows it.
struct Entry {
    id: AccountId,
    name: String,
    me: String,
    picture: Option<std::path::PathBuf>,
    pinned: bool,
    unread: u32,
}

fn entries(app: &mut App) -> Vec<Entry> {
    app.accounts
        .iter_mut()
        .map(|account| {
            let me = account.me.clone().unwrap_or_default();
            let picture = (!me.is_empty()).then(|| account.avatar(&me)).flatten();
            Entry {
                id: account.id.clone(),
                name: account.display_label(app.locale),
                me,
                picture,
                pinned: account.settings.profile_pin_hash.is_some(),
                unread: account.unread_chat_count(),
            }
        })
        .collect()
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    keys(app, ui.ctx());
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(palette.window))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let top = theme::blend(palette.window, palette.accent, 0.10);
            widgets::paint_vertical_gradient(ui, rect, top, palette.window);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(rect.height() * 0.12);
                theme::text(ui, "WAVO", theme::semibold(30.0), palette.text);
                let label = match (
                    app.profile_gate.as_ref().map(|gate| gate.asking.is_some()),
                    app.accounts.len(),
                ) {
                    (Some(true), _) => gettext(app.locale, "Enter the profile password"),
                    (_, 1) => gettext(app.locale, "This profile is protected"),
                    _ => gettext(app.locale, "Who's watching?"),
                };
                ui.add_space(8.0);
                theme::paragraph(ui, label, theme::regular(14.0), palette.secondary);
                ui.add_space(20.0);
                match app.profile_gate.as_ref().map(|gate| gate.asking.clone()) {
                    Some(Some(id)) => password_ask(app, ui, &id),
                    _ => wall(app, ui),
                }
            });
        });
}

fn keys(app: &mut App, ctx: &egui::Context) {
    let asking = app
        .profile_gate
        .as_ref()
        .is_some_and(|gate| gate.asking.is_some());
    ctx.input_mut(|input| {
        if asking {
            if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                app.actions.push(Action::LeaveProfileGate);
            }
        } else if input
            .consume_key(egui::Modifiers::NONE, egui::Key::Escape)
            .then_some(())
            .is_some()
        {
            // Nothing to leave: the wall is the top of the app at start.
        }
    });
}

fn wall(app: &mut App, ui: &mut egui::Ui) {
    let accounts = entries(app);
    let size = 96.0;
    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        for entry in &accounts {
            profile_tile(app, ui, entry, size);
        }
        add_tile(app, ui, size);
    });
}

fn profile_tile(app: &mut App, ui: &mut egui::Ui, entry: &Entry, size: f32) {
    let palette = app.palette;
    ui.vertical(|ui| {
        ui.set_max_width(size + 24.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!(
                    "{} ({})",
                    entry.name,
                    if entry.pinned { "pinned" } else { "open" }
                ),
            )
        });
        if response.hovered() {
            ui.painter()
                .rect_filled(rect.expand(4.0), 12.0, palette.surface_hover);
        }
        let center = egui::pos2(rect.center().x, rect.center().y - 2.0);
        widgets::paint_avatar(
            ui,
            &palette,
            egui::Rect::from_center_size(center, Vec2::splat(size - 8.0)),
            &entry.name,
            if entry.me.is_empty() {
                entry.id.as_str()
            } else {
                &entry.me
            },
            entry.picture.as_deref(),
        );
        if entry.pinned {
            // A lock badge on the bottom right says this one asks a password.
            let badge = 20.0;
            let at = egui::pos2(rect.right() - badge / 2.0, rect.bottom() - badge / 2.0);
            let badge_rect = egui::Rect::from_center_size(at, Vec2::splat(badge + 6.0));
            ui.painter()
                .circle_filled(at, badge / 2.0 + 2.0, palette.panel);
            ui.painter().circle_filled(
                at,
                badge / 2.0,
                theme::blend(palette.panel, palette.text, 0.06),
            );
            theme::paint_icon(ui, Icon::Lock, badge_rect, 12.0, palette.secondary);
        }
        if entry.unread > 0 {
            let at = egui::pos2(rect.right() - 4.0, rect.top() + 4.0);
            ui.painter().circle_filled(at, 7.0, palette.accent);
        }
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.horizontal(|ui| {
                if widgets::rich_text(ui, &entry.name, theme::semibold(13.5), palette.text)
                    .hovered()
                {}
            });
            ui.add_space(6.0);
        });
        if response.clicked() {
            app.actions.push(Action::PickProfile(entry.id.clone()));
        }
    });
}

fn add_tile(app: &mut App, ui: &mut egui::Ui, size: f32) {
    let palette = app.palette;
    ui.vertical(|ui| {
        ui.set_max_width(size + 24.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Add profile")
        });
        theme::reveal_focus(&response);
        let fill = if response.hovered() {
            palette.surface_hover
        } else {
            palette.surface
        };
        ui.painter().rect_filled(rect.shrink(4.0), 10.0, fill);
        theme::paint_icon(
            ui,
            Icon::Plus,
            rect,
            size * 0.4,
            if response.hovered() {
                palette.text
            } else {
                palette.secondary
            },
        );
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            theme::text(
                ui,
                gettext(app.locale, "Add profile"),
                theme::semibold(13.5),
                palette.secondary,
            );
        });
        if response.clicked() {
            app.actions.push(Action::AddAccount);
        }
    });
}

/// The password ask for a picked, pinned profile.
fn password_ask(app: &mut App, ui: &mut egui::Ui, id: &AccountId) {
    let palette = app.palette;
    let locale = app.locale;
    let Some(account) = app.accounts.iter_mut().find(|account| account.id == *id) else {
        app.actions.push(Action::LeaveProfileGate);
        return;
    };
    let name = account.display_label(locale);
    let me = account.me.clone().unwrap_or_default();
    let picture = (!me.is_empty()).then(|| account.avatar(&me)).flatten();
    ui.vertical(|ui| {
        ui.set_max_width(360.0);
        widgets::paint_avatar(
            ui,
            &palette,
            egui::Rect::from_center_size(
                egui::pos2(ui.available_width() / 2.0, 48.0),
                Vec2::splat(72.0),
            ),
            &name,
            if me.is_empty() { id.as_str() } else { &me },
            picture.as_deref(),
        );
        ui.add_space(4.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            theme::text(ui, &name, theme::semibold(15.0), palette.text);
        });
        ui.add_space(12.0);
        let gate = app.profile_gate.as_mut().expect("gate open");
        let busy = gate.busy;
        let id_field = egui::Id::new(PASSWORD_ID);
        let mut submit = ui.memory(|memory| memory.has_focus(id_field))
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let field = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut gate.entry)
                .id(id_field)
                .password(true)
                .hint_text(gettext(locale, "Profile password"))
                .desired_width(240.0_f32.min(ui.available_width())),
        );
        if ui.memory(|memory| memory.focused().is_none()) && !busy {
            field.request_focus();
        }
        if let Some(wait) = gate.wait_left() {
            let seconds = wait.as_secs_f32().ceil().max(1.0) as u64;
            theme::paragraph(
                ui,
                gettext(
                    locale,
                    "Too many wrong passwords. Try again in {seconds} s.",
                )
                .replace("{seconds}", &seconds.to_string()),
                theme::regular(13.0),
                palette.danger,
            );
            ui.ctx()
                .request_repaint_after(wait.min(std::time::Duration::from_millis(250)));
        } else if gate.wrong {
            theme::paragraph(
                ui,
                gettext(locale, "Wrong password. Try again."),
                theme::regular(13.0),
                palette.danger,
            );
        }
        ui.add_space(4.0);
        if busy {
            theme::spinner(ui, 20.0, palette.accent);
        } else {
            submit |= ui
                .add_enabled_ui(gate.can_try(), |ui| {
                    theme::pill_button(ui, &palette, &gettext(locale, "Open"), true)
                })
                .inner
                .clicked();
        }
        if submit {
            app.actions.push(Action::SubmitProfilePassword);
        }
        ui.add_space(8.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            if theme::link(
                ui,
                gettext(locale, "Back to profiles"),
                theme::regular(13.0),
                palette.accent,
            )
            .clicked()
            {
                app.actions.push(Action::LeaveProfileGate);
            }
        });
    });
}
