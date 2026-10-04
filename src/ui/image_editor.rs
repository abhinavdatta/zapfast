//! The photo editor shown before a picture is sent, like WhatsApp's.
//!
//! WhatsApp Web opens an editor over the chat the moment a picture is
//! attached: the photo in the middle, one-tap rotate and crop tools beside
//! it, and a caption bar with the send button at the bottom. ZapFast
//! implements the orientation half of that editor — rotate left and right,
//! flip horizontally and vertically — with a WhatsApp-style undo of every
//! step. Nothing touches the disk until Send; an unedited picture sends the
//! original file.

use egui::{Align, CornerRadius, Frame, Layout, Margin, Rect, Sense, Vec2, vec2};

use crate::app::App;
use crate::image_edit::EditOp;
use crate::model::Action;
use crate::theme::{self, Icon};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(editor) = app.image_editor.clone() else {
        return;
    };
    let palette = app.palette;
    let viewport = ctx.content_rect().size();
    let response = egui::Modal::new(egui::Id::new("image-editor"))
        .frame(
            Frame::new()
                .fill(palette.overlay)
                .corner_radius(CornerRadius::same(theme::RADIUS + 4))
                .inner_margin(Margin::same(10)),
        )
        .backdrop_color(palette.shadow)
        .show(ctx, |ui| {
            ui.set_width((viewport.x * 0.86).clamp(viewport.x.min(360.0), 1000.0));
            ui.set_height((viewport.y * 0.88).clamp(viewport.y.min(300.0), 860.0));
            ui.horizontal(|ui| {
                let name = editor
                    .source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Photo");
                crate::ui::widgets::rich_text(ui, name, theme::semibold(14.0), palette.text);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(
                        ui,
                        Icon::X,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Close without sending (Esc)",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::CloseImageEditor);
                    }
                    if theme::icon_button(
                        ui,
                        Icon::Refresh,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Redo (Ctrl+Shift+Z)",
                    )
                    .clicked()
                        && editor.can_redo()
                    {
                        app.actions.push(Action::EditorRedo);
                    }
                    if theme::icon_button(
                        ui,
                        Icon::Reply,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Undo (Ctrl+Z)",
                    )
                    .clicked()
                        && editor.can_undo()
                    {
                        app.actions.push(Action::EditorUndo);
                    }
                });
            });
            ui.separator();

            let canvas = vec2(
                ui.available_width().max(0.0),
                ui.available_height().max(0.0),
            );
            let texture = egui::ColorImage::from_rgba_unmultiplied(
                [editor.width.max(1), editor.height.max(1)],
                &editor.rgba,
            );
            let handle =
                ctx.load_texture("image-editor-preview", texture, egui::TextureOptions::LINEAR);
            let fitted = fit_into(handle.size_vec2(), canvas);
            let (body, _) = ui.allocate_exact_size(canvas.max(fitted), Sense::hover());
            egui::Image::from_texture((handle.id(), fitted))
                .corner_radius(6.0)
                .paint_at(ui, Rect::from_center_size(body.center(), fitted));

            // Side toolbar of orientation edits, like the phone app's row.
            let bar = Rect::from_min_size(
                egui::pos2(body.right() + 6.0, body.center().y - 110.0),
                vec2(44.0, 220.0),
            )
            .intersect(ui.max_rect());
            if bar.width() >= 44.0 {
                ui.allocate_rect(bar, Sense::hover());
                let tools = [
                    (EditOp::RotateLeft, "Rotate left (Ctrl+Shift+R)"),
                    (EditOp::RotateRight, "Rotate right (Ctrl+R)"),
                    (EditOp::FlipHorizontal, "Flip horizontal"),
                    (EditOp::FlipVertical, "Flip vertical"),
                ];
                let step = (bar.height() / tools.len() as f32).min(52.0);
                for (index, (op, tip)) in tools.iter().enumerate() {
                    let rect = Rect::from_center_size(
                        egui::pos2(bar.center().x, bar.top() + step * index as f32 + step * 0.5),
                        Vec2::splat(42.0),
                    );
                    let response = ui
                        .interact(rect, ui.id().with(("editor-tool", op)), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(*tip);
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 10.0, palette.surface_hover);
                    }
                    theme::paint_icon(ui, Icon::Refresh, rect.shrink(10.0), 20.0, palette.text);
                    // A small tick distinguishes the direction: a tick on the
                    // left means counterclockwise, on the right clockwise.
                    let side = match op {
                        EditOp::RotateLeft => -9.0,
                        _ => 9.0,
                    };
                    if !matches!(op, EditOp::FlipVertical) {
                        ui.painter().line_segment(
                            [
                                rect.center() + vec2(side, 0.0),
                                rect.center() + vec2(side * 0.4, 0.0),
                            ],
                            egui::Stroke::new(2.0, palette.secondary),
                        );
                    }
                    if response.clicked() {
                        app.actions.push(Action::EditorOp(*op));
                    }
                }
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let send = egui::Button::new(
                        egui::RichText::new("Send photo").font(theme::medium(13.5)),
                    )
                    .fill(palette.accent);
                    if ui
                        .add(send)
                        .on_hover_text("Send to the open chat (Ctrl+Enter)")
                        .clicked()
                    {
                        app.actions.push(Action::EditorSend);
                    }
                    if ui.button("Cancel").clicked() {
                        app.actions.push(Action::CloseImageEditor);
                    }
                    let applied = &editor.history[..editor.redo_index.min(editor.history.len())];
                    let summary = if applied.is_empty() {
                        "No edits yet".to_owned()
                    } else {
                        applied
                            .iter()
                            .map(|op| op.label())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    };
                    theme::text(ui, &summary, theme::regular(12.0), palette.secondary);
                });
            });
        });
    if response.should_close() {
        app.actions.push(Action::CloseImageEditor);
    }
}

/// Largest size keeping the aspect ratio inside the canvas, never enlarging.
fn fit_into(size: Vec2, canvas: Vec2) -> Vec2 {
    if size.x <= 0.0 || size.y <= 0.0 || canvas.x <= 0.0 || canvas.y <= 0.0 {
        return vec2(0.0, 0.0);
    }
    let scale = (canvas.x / size.x).min(canvas.y / size.y).min(1.0);
    size * scale
}
