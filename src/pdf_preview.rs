//! State and routing for the in-app PDF viewer.
//!
//! Pages render through Pdfium in a background thread when a Pdfium library
//! is available; without one, PDFs keep opening in the desktop application.

use std::path::{Path, PathBuf};

use egui::{Event, Key, Modifiers};

/// Whether a downloaded file can open in the built-in viewer.
pub fn can_preview(path: &Path) -> bool {
    crate::safety::can_preview_pdf(path)
}

/// Whether the local Pdfium library loaded. The first check runs once per
/// process and caches the outcome; callers use it to decide between the
/// built-in viewer and the desktop application.
pub fn pdfium_available() -> bool {
    crate::pdf_render::available()
}

/// Keyboard command the viewer handles while it owns the window.
pub fn preview_action(key: Key, modifiers: Modifiers) -> Option<crate::model::Action> {
    let command = modifiers.command || modifiers.ctrl;
    match (command, key) {
        (true, Key::Plus) | (true, Key::Equals) => Some(crate::model::Action::ZoomPdfIn),
        (true, Key::Minus) => Some(crate::model::Action::ZoomPdfOut),
        (true, Key::Num0) => Some(crate::model::Action::FitPdf),
        (false, Key::Plus) | (false, Key::Equals) if !modifiers.any() => {
            Some(crate::model::Action::ZoomPdfIn)
        }
        (false, Key::Minus) if !modifiers.any() => Some(crate::model::Action::ZoomPdfOut),
        (false, Key::Num0) if !modifiers.any() => Some(crate::model::Action::FitPdf),
        (false, Key::PageDown) if !modifiers.any() => {
            Some(crate::model::Action::PdfPage(usize::MAX))
        }
        (false, Key::PageUp) if !modifiers.any() => {
            Some(crate::model::Action::PdfPage(usize::MAX - 1))
        }
        _ => None,
    }
}

/// The viewer swallows keys that would type into or edit the chat behind it,
/// while leaving navigation and activation keys (Tab, Enter, Space, arrows)
/// for the modal's own controls.
pub fn consumes_key(key: &Event) -> bool {
    match key {
        Event::Text(_) | Event::Paste(_) | Event::Copy | Event::Cut => true,
        Event::Key { key, .. } => !is_modal_navigation(*key),
        _ => false,
    }
}

/// Keys the viewer modal needs for focus traversal, activation, and
/// scrolling its own controls.
fn is_modal_navigation(key: Key) -> bool {
    matches!(
        key,
        Key::Tab
            | Key::Enter
            | Key::Space
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::Home
            | Key::End
    )
}

/// Fitted page size for a canvas, keeping the aspect ratio. `(0, 0)` page
/// sizes get `(0, 0)`.
pub fn fit_size(width: f32, height: f32, canvas_width: f32, canvas_height: f32) -> (f32, f32) {
    if width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0);
    }
    let scale = (canvas_width / width).min(canvas_height / height).min(1.0);
    (width * scale, height * scale)
}

/// Page size for the zoom level, relative to the fitted size.
pub fn zoomed_size(width: f32, height: f32, zoom: f32) -> (f32, f32) {
    (width * zoom, height * zoom)
}

#[derive(Clone, Debug, PartialEq)]
pub struct PdfState {
    path: PathBuf,
    /// One-based page number while reading; zero before the document loads.
    pub page: usize,
    /// Total pages, learned when the first page renders.
    pub pages: usize,
    zoom: f32,
    fit: bool,
}

impl PdfState {
    const MIN_ZOOM: f32 = 0.5;
    const MAX_ZOOM: f32 = 4.0;
    const ZOOM_STEP: f32 = 1.25;

    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            page: 1,
            pages: 0,
            zoom: 1.0,
            fit: true,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn is_fit(&self) -> bool {
        self.fit
    }

    pub fn zoom_in(&mut self) {
        self.fit = false;
        self.zoom = (self.zoom * Self::ZOOM_STEP).min(Self::MAX_ZOOM);
    }

    pub fn zoom_out(&mut self) {
        self.fit = false;
        self.zoom = (self.zoom / Self::ZOOM_STEP).max(Self::MIN_ZOOM);
    }

    pub fn fit(&mut self) {
        self.fit = true;
        self.zoom = 1.0;
    }

    /// Moves to a page, clamped into the document. `usize::MAX` means one
    /// page forward, `usize::MAX - 1` one back; PageUp/PageDown carry these
    /// because the total is unknown to the keyboard router.
    pub fn goto(&mut self, page: usize) {
        if self.pages == 0 {
            return;
        }
        let target = match page {
            usize::MAX => self.page + 1,
            size if size == usize::MAX - 1 => self.page.saturating_sub(1),
            size => size,
        };
        self.page = target.clamp(1, self.pages);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn preview_accepts_only_pdf_files() {
        assert!(can_preview(Path::new("document.pdf")));
        assert!(can_preview(Path::new("DOCUMENT.PDF")));
        for name in ["invoice.pdf.exe", "scan.pd", "document", "photo.png"] {
            assert!(!can_preview(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn viewer_keys_map_to_commands_including_shifted_equals() {
        use egui::{Key, Modifiers};

        assert_eq!(
            preview_action(Key::Equals, Modifiers::COMMAND),
            Some(crate::model::Action::ZoomPdfIn)
        );
        assert_eq!(
            preview_action(Key::Minus, Modifiers::NONE),
            Some(crate::model::Action::ZoomPdfOut)
        );
        assert_eq!(
            preview_action(Key::Num0, Modifiers::COMMAND),
            Some(crate::model::Action::FitPdf)
        );
        assert_eq!(
            preview_action(Key::PageDown, Modifiers::NONE),
            Some(crate::model::Action::PdfPage(usize::MAX))
        );
        assert_eq!(
            preview_action(Key::PageUp, Modifiers::NONE),
            Some(crate::model::Action::PdfPage(usize::MAX - 1))
        );
        assert_eq!(preview_action(Key::Escape, Modifiers::NONE), None);
    }

    #[test]
    fn the_viewer_swallows_chat_input_keys() {
        use egui::{Event, Key, Modifiers};

        assert!(consumes_key(&Event::Text("a".into())));
        assert!(consumes_key(&Event::Copy));
        assert!(consumes_key(&Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }));
        assert!(!consumes_key(&Event::PointerMoved(egui::pos2(1.0, 2.0))));
        for key in [
            Key::Tab,
            Key::Enter,
            Key::Space,
            Key::ArrowDown,
            Key::ArrowUp,
        ] {
            assert!(
                !consumes_key(&Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }),
                "modal navigation key {key:?} must reach the viewer"
            );
        }
    }

    #[test]
    fn fit_and_zoom_keep_the_page_ratio() {
        assert_eq!(fit_size(612.0, 792.0, 306.0, 792.0), (306.0, 396.0));
        assert_eq!(fit_size(0.0, 0.0, 800.0, 700.0), (0.0, 0.0));
        assert_eq!(zoomed_size(300.0, 400.0, 2.0), (600.0, 800.0));
    }

    #[test]
    fn viewer_starts_fitted_and_pages_are_clamped() {
        let mut viewer = PdfState::new(PathBuf::from("doc.pdf"));
        assert_eq!(viewer.path(), Path::new("doc.pdf"));
        assert!(viewer.is_fit());
        assert_eq!(viewer.page, 1);

        viewer.zoom_in();
        assert!(!viewer.is_fit());
        assert_eq!(viewer.zoom(), 1.25);
        for _ in 0..20 {
            viewer.zoom_in();
        }
        assert_eq!(viewer.zoom(), 4.0);
        for _ in 0..40 {
            viewer.zoom_out();
        }
        assert_eq!(viewer.zoom(), 0.5);
        viewer.fit();
        assert!(viewer.is_fit());

        viewer.pages = 5;
        viewer.goto(usize::MAX);
        assert_eq!(viewer.page, 2);
        viewer.goto(usize::MAX - 1);
        assert_eq!(viewer.page, 1);
        viewer.goto(usize::MAX - 1);
        assert_eq!(viewer.page, 1, "the first page is the floor");
        viewer.goto(4);
        assert_eq!(viewer.page, 4);
        viewer.goto(99);
        assert_eq!(viewer.page, 5, "the last page is the ceiling");
        viewer.pages = 0;
        viewer.goto(usize::MAX);
        assert_eq!(viewer.page, 5, "an unloaded document keeps its page");
    }
}
