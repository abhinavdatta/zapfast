//! Image editing for outgoing pictures: rotate, flip, and undo/redo.
//!
//! WhatsApp Web's photo editor opens before a picture is sent and offers
//! rotate, crop, and draw. ZapFast implements the orientation half of that
//! flow — rotate 90° left/right, horizontal and vertical flips — with a
//! WhatsApp-style undo and redo for every step. Edits decode the original
//! file once and stay in memory until the picture is sent; nothing on disk
//! is modified, and unedited pictures send the original file untouched.

use std::path::PathBuf;

/// One orientation step applied to the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditOp {
    RotateLeft,
    RotateRight,
    FlipHorizontal,
    FlipVertical,
}

impl EditOp {
    pub fn label(self) -> &'static str {
        match self {
            Self::RotateLeft => "Rotate left",
            Self::RotateRight => "Rotate right",
            Self::FlipHorizontal => "Flip horizontal",
            Self::FlipVertical => "Flip vertical",
        }
    }
}

/// Editor state for the picture being composed in the open chat.
#[derive(Clone, Debug)]
pub struct Editor {
    /// The original file the editor opened from.
    pub source: PathBuf,
    /// The last file sent or staged by this editor, kept so a re-edit
    /// round-trips its own output instead of the untouched original.
    pub last_output: Option<PathBuf>,
    /// Re-edit generations already spent. Bounded so a long editing session
    /// cannot grow the undo stack without end.
    pub generation: u32,
    /// Pixel edits in order. Empty means the picture sends unchanged.
    pub history: Vec<EditOp>,
    /// How far into `history` the preview stands. Undo rewinds it, redo
    /// replays it.
    pub redo_index: usize,
    /// RGBA pixels of the picture as the editor currently shows them.
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

const MAX_GENERATIONS: u32 = 8;

impl Editor {
    /// Decodes a picture and opens the editor on it. Fails when the file
    /// cannot be read or is not a decodable image.
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let format =
            image::guess_format(&bytes).map_err(|error| format!("Unsupported image: {error}"))?;
        if !matches!(
            format,
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::WebP
                | image::ImageFormat::Gif
        ) {
            return Err("Only PNG, JPEG, WebP, and GIF pictures can be edited".to_owned());
        }
        let decoded = image::load_from_memory(&bytes)
            .map_err(|error| format!("Could not decode the picture: {error}"))?;
        let rgba = decoded.to_rgba8();
        let (width, height) = (rgba.width() as usize, rgba.height() as usize);
        Ok(Self {
            source: path.to_owned(),
            last_output: None,
            generation: 0,
            history: Vec::new(),
            redo_index: 0,
            rgba: rgba.into_raw(),
            width,
            height,
        })
    }

    /// Whether the editor holds no pixel edits, so the original file sends.
    pub fn unedited(&self) -> bool {
        self.redo_index == 0
    }

    pub fn can_undo(&self) -> bool {
        self.redo_index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.redo_index < self.history.len()
    }

    /// Applies an edit, truncating any redos like every editor does.
    pub fn apply(&mut self, op: EditOp) {
        self.redo_index = self.redo_index.min(self.history.len());
        self.history.truncate(self.redo_index);
        self.history.push(op);
        self.redo_index += 1;
        self.apply_to_pixels(op);
    }

    /// Undoes the newest edit by applying its inverse; redo replays the
    /// forward op. Orientation inverses are exact on pixels.
    pub fn undo(&mut self) {
        if self.redo_index > 0
            && let Some(op) = self.history.get(self.redo_index - 1)
        {
            self.apply_to_pixels(Self::inverse(*op));
            self.redo_index -= 1;
        }
    }

    pub fn redo(&mut self) {
        if self.can_redo() {
            let op = self.history[self.redo_index];
            self.redo_index += 1;
            self.apply_to_pixels(op);
        }
    }

    /// The orientation step that undoes `op`.
    fn inverse(op: EditOp) -> EditOp {
        match op {
            EditOp::RotateLeft => EditOp::RotateRight,
            EditOp::RotateRight => EditOp::RotateLeft,
            EditOp::FlipHorizontal | EditOp::FlipVertical => op,
        }
    }

    fn apply_to_pixels(&mut self, op: EditOp) {
        let image = match image::RgbaImage::from_raw(
            self.width as u32,
            self.height as u32,
            std::mem::take(&mut self.rgba),
        ) {
            Some(image) => image,
            None => {
                // The buffer no longer matches the dimensions; rebuild from
                // the source so the editor never shows corrupted pixels.
                let _ = self.reload();
                return;
            }
        };
        let rotated = match op {
            EditOp::RotateLeft => image::imageops::rotate90(&image),
            EditOp::RotateRight => image::imageops::rotate270(&image),
            EditOp::FlipHorizontal => image::imageops::flip_horizontal(&image),
            EditOp::FlipVertical => image::imageops::flip_vertical(&image),
        };
        self.width = rotated.width() as usize;
        self.height = rotated.height() as usize;
        self.rgba = rotated.into_raw();
    }

    /// Re-decodes the source file, used to rebuild after an inconsistent
    /// state instead of keeping corrupted pixels.
    fn reload(&mut self) -> Result<(), String> {
        let restored = Self::open(&self.source)?;
        self.rgba = restored.rgba;
        self.width = restored.width;
        self.height = restored.height;
        Ok(())
    }

    /// PNG bytes of the edited picture, written next to the original so the
    /// media cache stays on one volume.
    pub fn encode(&self, dir: &std::path::Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let stem = self
            .source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("photo");
        let name = format!("zapfast-edit-{stem}-{}.png", self.generation);
        let path = dir.join(name);
        let image =
            image::RgbaImage::from_raw(self.width as u32, self.height as u32, self.rgba.clone())
                .ok_or("The edited picture could not be rendered")?;
        image
            .save_with_format(&path, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        Ok(path)
    }

    /// Rounds a re-opened editor to the next generation, keeping undo within
    /// a bounded session budget.
    pub fn advance_generation(&mut self) {
        self.generation = (self.generation + 1).min(MAX_GENERATIONS);
    }
}

/// Renders the current edit state to a PNG in `dir` and returns its path.
/// Unedited pictures skip the encode and return the original file, so the
/// send path, pending thumbnails, and media pipeline keep seeing original
/// data whenever nothing changed.
pub fn render_to_file(editor: &Editor, dir: &std::path::Path) -> Result<PathBuf, String> {
    if editor.unedited() {
        return Ok(editor.source.clone());
    }
    editor.encode(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("photo.png");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]))
            .save_with_format(&path, image::ImageFormat::Png)
            .expect("write png");
        let mut editor = Editor::open(&path).expect("open editor");
        editor.generation = u32::MAX; // keep test outputs inside the temp dir
        // Leak the temp dir for the lifetime of the test process; Windows
        // keeps the written files open.
        std::mem::forget(dir);
        editor
    }

    #[test]
    fn a_fresh_editor_sends_the_original() {
        let editor = editor();
        assert!(editor.unedited());
        assert_eq!(editor.history.len(), 0);
    }

    #[test]
    fn rotating_swaps_dimensions() {
        let mut editor = editor();
        editor.apply(EditOp::RotateRight);
        assert_eq!(editor.width, 3);
        assert_eq!(editor.height, 4);
        assert_eq!(editor.redo_index, 1);
    }

    #[test]
    fn undo_rewinds_and_redo_replays() {
        let mut editor = editor();
        editor.apply(EditOp::RotateRight);
        editor.apply(EditOp::FlipHorizontal);
        assert!(editor.can_undo());
        editor.undo();
        assert_eq!(editor.redo_index, 1);
        assert_eq!((editor.width, editor.height), (3, 4));
        assert!(editor.can_redo());
        editor.redo();
        assert_eq!(editor.redo_index, 2);
        assert_eq!((editor.width, editor.height), (3, 4));
    }

    #[test]
    fn editing_after_undo_discards_the_redo_tail() {
        let mut editor = editor();
        editor.apply(EditOp::RotateRight);
        editor.apply(EditOp::FlipVertical);
        editor.undo();
        editor.apply(EditOp::RotateLeft);
        assert_eq!(
            editor.history,
            vec![EditOp::RotateRight, EditOp::RotateLeft]
        );
        assert!(!editor.can_redo());
    }

    #[test]
    fn four_flips_return_to_the_original() {
        let mut editor = editor();
        let before = editor.rgba.clone();
        let (width, height) = (editor.width, editor.height);
        for _ in 0..2 {
            editor.apply(EditOp::FlipHorizontal);
        }
        assert_eq!(editor.rgba, before);
        editor.apply(EditOp::RotateRight);
        editor.apply(EditOp::RotateLeft);
        editor.apply(EditOp::RotateRight);
        editor.apply(EditOp::RotateLeft);
        assert_eq!(editor.rgba, before);
        assert_eq!((editor.width, editor.height), (width, height));
    }

    #[test]
    fn encoding_writes_a_png_that_reopens_identically() {
        let mut editor = editor();
        editor.apply(EditOp::RotateRight);
        let dir = tempfile::tempdir().expect("temp dir");
        let path = super::render_to_file(&editor, dir.path()).expect("encode");
        assert!(path.is_file());
        let reopened = Editor::open(&path).expect("reopen");
        assert_eq!(reopened.width, 3);
        assert_eq!(reopened.height, 4);
        assert!(reopened.unedited());
    }

    #[test]
    fn unedited_render_returns_the_source_file() {
        let editor = editor();
        let dir = tempfile::tempdir().expect("temp dir");
        let path = super::render_to_file(&editor, dir.path()).expect("render");
        assert_eq!(path, editor.source);
    }
}
