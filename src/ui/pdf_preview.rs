//! Native viewer for downloaded PDF attachments.

use egui::{Align, Align2, CornerRadius, Frame, Layout, Margin, Stroke, Vec2, vec2};

use crate::app::App;
use crate::model::Action;
use crate::theme::{self, Icon};

pub fn show(app: &mut App, ctx: &egui::Context) {
    // The page count is learned when the first render lands.
    if let Some(preview) = app.pdf_preview.as_mut()
        && preview.pages == 0
    {
        preview.pages = crate::pdf_render::page_count(preview.path()).unwrap_or(0);
    }
    let Some(preview) = app.pdf_preview.clone() else {
        return;
    };
    let palette = app.palette;
    let frame = Frame::new()
        .fill(palette.overlay)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(theme::RADIUS + 4))
        .inner_margin(Margin::same(14));
    let viewport = ctx.content_rect().size();
    let response = egui::Modal::new(egui::Id::new("pdf-preview"))
        .frame(frame)
        .backdrop_color(palette.shadow)
        .show(ctx, |ui| {
            ui.set_width((viewport.x * 0.92).clamp(viewport.x.min(340.0), 1240.0));
            ui.set_height((viewport.y * 0.9).clamp(viewport.y.min(280.0), 940.0));
            let name = preview
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Document");
            ui.horizontal(|ui| {
                crate::ui::widgets::rich_text(ui, name, theme::semibold(14.0), palette.text);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(
                        ui,
                        Icon::X,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Close viewer (Esc)",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::ClosePdfPreview);
                    }
                    if ui.button("Open externally").clicked() {
                        app.actions.push(Action::OpenFile(preview.path().to_owned()));
                    }
                    if ui.button("Fit").clicked() {
                        app.actions.push(Action::FitPdf);
                    }
                    if theme::icon_button(
                        ui,
                        Icon::Plus,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Zoom in",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::ZoomPdfIn);
                    }
                    if theme::icon_button(
                        ui,
                        Icon::Minus,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Zoom out",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::ZoomPdfOut);
                    }
                    // Page navigation once the page count is known.
                    if preview.pages > 0 {
                        if theme::icon_button(
                            ui,
                            Icon::ChevronRight,
                            16.0,
                            palette.secondary,
                            palette.text,
                            "Next page",
                        )
                        .clicked()
                        {
                            app.actions.push(Action::PdfPage((preview.page + 1).min(preview.pages)));
                        }
                        ui.label(format!("{} / {}", preview.page, preview.pages));
                        if theme::icon_button(
                            ui,
                            Icon::ChevronLeft,
                            16.0,
                            palette.secondary,
                            palette.text,
                            "Previous page",
                        )
                        .clicked()
                        {
                            app.actions
                                .push(Action::PdfPage(preview.page.saturating_sub(1).max(1)));
                        }
                    }
                });
            });
            ui.separator();

            let canvas = vec2(
                ui.available_width().max(0.0),
                ui.available_height().max(0.0),
            );
            if crate::pdf_render::unavailable() {
                // No pdfium on this machine: the attachment still opens
                // externally through the toolbar button.
                let (rect, _) = ui.allocate_exact_size(canvas, egui::Sense::hover());
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "The PDF viewer needs the pdfium library.\nUse “Open externally” to read this document.",
                    theme::regular(14.0),
                    palette.secondary,
                );
            } else {
                let zoom = if preview.is_fit() {
                    1.0
                } else {
                    preview.zoom()
                };
                crate::pdf_render::request(
                    preview.path(),
                    preview.page.saturating_sub(1),
                    zoom,
                    ctx.clone(),
                );
                if crate::pdf_render::failed(preview.path()) {
                    let (rect, _) = ui.allocate_exact_size(canvas, egui::Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "This document could not be opened.\nTry “Open externally”.",
                        theme::regular(14.0),
                        palette.secondary,
                    );
                } else {
                    match crate::pdf_render::page(preview.path(), preview.page - 1, zoom) {
                        Some(page) => {
                            // Upload each rendered page once and reuse the texture
                            // while the same page stays open.
                            let size_px = [page.width as usize, page.height as usize];
                            let image =
                                egui::ColorImage::from_rgba_unmultiplied(size_px, &page.rgba);
                            let texture = ctx.load_texture(
                                format!("pdf-page-{}", preview.page),
                                image,
                                egui::TextureOptions::LINEAR,
                            );
                            let size = display_size(
                                Vec2::new(page.width as f32, page.height as f32),
                                canvas,
                                preview.is_fit(),
                                preview.zoom(),
                            );
                            egui::ScrollArea::both()
                                .id_salt("pdf-preview-scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.allocate_ui_with_layout(
                                        canvas.max(size),
                                        Layout::centered_and_justified(egui::Direction::TopDown),
                                        |ui| {
                                            ui.add(
                                                egui::Image::from_texture(
                                                    egui::load::SizedTexture::new(
                                                        texture.id(),
                                                        size,
                                                    ),
                                                )
                                                .fit_to_exact_size(size),
                                            );
                                        },
                                    );
                                });
                        }
                        None => {
                            let (rect, _) = ui.allocate_exact_size(canvas, egui::Sense::hover());
                            theme::paint_spinner(ui, rect, 28.0, palette.accent);
                        }
                    }
                }
            }
        });
    if response.should_close() {
        app.actions.push(Action::ClosePdfPreview);
    }
}

/// Size the page is drawn at from the rendered pixel dimensions: fitted into
/// the canvas, or scaled by the viewer's zoom factor.
fn display_size(original: Vec2, canvas: Vec2, fit: bool, zoom: f32) -> Vec2 {
    let (width, height) = if fit {
        crate::pdf_preview::fit_size(original.x, original.y, canvas.x, canvas.y)
    } else {
        crate::pdf_preview::zoomed_size(original.x, original.y, zoom)
    };
    vec2(width, height)
}
