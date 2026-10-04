//! Pdfium page rendering for the built-in PDF viewer.
//!
//! The pdfium library is loaded at runtime, not linked or compiled in: the
//! executable stays small, no C toolchain is needed to build, and machines
//! without the library keep opening PDFs in their desktop application. Render
//! jobs run on background worker threads and finished pages reach the
//! interface through the shared cache and the regular event wake.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// One rendered page, ready to upload as a texture.
pub struct Page {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha RGBA rows.
    pub rgba: Vec<u8>,
}

/// A queued or in-flight render.
struct Job {
    path: std::path::PathBuf,
    page: usize,
    zoom: f32,
    ctx: egui::Context,
}

/// Shared render cache, keyed by `path|page|zoom`.
#[derive(Default)]
struct Cache {
    pages: HashMap<String, Arc<Page>>,
    order: Vec<String>,
    /// Documents that failed to open, to avoid retry storms.
    failed: Vec<std::path::PathBuf>,
    /// Page counts of opened documents.
    counts: HashMap<std::path::PathBuf, usize>,
}

impl Cache {
    fn evict(&mut self) {
        const CAP: usize = 12;
        while self.order.len() > CAP {
            let oldest = self.order.remove(0);
            self.pages.remove(&oldest);
        }
    }
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

fn queue() -> &'static Mutex<Vec<Job>> {
    static QUEUE: OnceLock<Mutex<Vec<Job>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Serializes all pdfium calls: the library keeps global state.
fn pdfium_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Whether the pdfium library could be loaded on this machine. Probed once.
pub fn available() -> bool {
    library().is_some()
}

/// Inverse of [`available`], for the viewer's fallback message.
pub fn unavailable() -> bool {
    !available()
}

/// The loaded pdfium library, opened once for the process.
fn library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY
        .get_or_init(|| {
            let _guard = pdfium_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Library::open()
        })
        .as_ref()
}

/// The number of pages of a document seen so far, if it opened.
pub fn page_count(path: &Path) -> Option<usize> {
    cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .counts
        .get(path)
        .copied()
}

/// Whether the document failed to open.
pub fn failed(path: &Path) -> bool {
    cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .failed
        .iter()
        .any(|failed| failed == path)
}

/// The finished page, when its render has landed.
pub fn page(path: &Path, index: usize, zoom: f32) -> Option<Arc<Page>> {
    let key = format!("{}|{index}|{zoom:.3}", path.display());
    cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .pages
        .get(&key)
        .cloned()
}

/// Queues a page for rendering unless it is done or already queued.
pub fn request(path: &Path, index: usize, zoom: f32, ctx: egui::Context) {
    if unavailable() || failed(path) || page(path, index, zoom).is_some() {
        return;
    }
    let path = path.to_owned();
    let mut queue = queue()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if queue
        .iter()
        .any(|job| job.path == path && job.page == index && job.zoom == zoom)
    {
        return;
    }
    queue.push(Job {
        path,
        page: index,
        zoom,
        ctx,
    });
    drop(queue);
    spawn_worker();
}

/// Forgets cached pages and failure marks for a document.
pub fn forget(path: &Path) {
    let mut cache = cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prefix = format!("{}|", path.display());
    cache.order.retain(|key| !key.starts_with(&prefix));
    cache.pages.retain(|key, _| !key.starts_with(&prefix));
    cache.failed.retain(|failed| failed != path);
    cache.counts.remove(path);
}

/// Renders one page to RGBA at `zoom` device scale.
fn render(path: &Path, index: usize, zoom: f32) -> Result<(Arc<Page>, usize), String> {
    let Some(lib) = library() else {
        return Err("pdfium is not installed".to_owned());
    };
    let _guard = pdfium_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    unsafe {
        lib.init_once();
        let data = std::fs::read(path).map_err(|error| error.to_string())?;
        let document = lib.load_document(&data)?;
        let pages = (lib.get_page_count)(document).max(0) as usize;
        let page = (lib.load_page)(document, index as i32);
        if page.is_null() {
            (lib.close_document)(document);
            return Err("could not open the page".to_owned());
        }
        let width_pt = (lib.page_width)(page);
        let height_pt = (lib.page_height)(page);
        let width = ((width_pt * zoom) as u32).clamp(1, 8_000);
        let height = ((height_pt * zoom) as u32).clamp(1, 8_000);
        // FPDFBitmap_BGR: 3 bytes per pixel, rows padded to 4 bytes.
        let bitmap = (lib.bitmap_create)(width as i32, height as i32, 0);
        let rendered = if bitmap.is_null() {
            Err("could not allocate the page bitmap".to_owned())
        } else {
            (lib.bitmap_fill_rect)(bitmap, 0, 0, width as i32, height as i32, 0x00FF_FFFF);
            const FPDF_ANNOT: i32 = 0x0001;
            const FPDF_LCD_TEXT: i32 = 0x0002;
            (lib.render_page_bitmap)(
                bitmap,
                page,
                0,
                0,
                width as i32,
                height as i32,
                0,
                FPDF_ANNOT | FPDF_LCD_TEXT,
            );
            let stride = (lib.bitmap_stride)(bitmap) as usize;
            let buffer = (lib.bitmap_buffer)(bitmap);
            let mut rgba = vec![0u8; width as usize * height as usize * 4];
            for row in 0..height as usize {
                let source =
                    std::slice::from_raw_parts(buffer.add(row * stride), width as usize * 3);
                let target = &mut rgba[row * width as usize * 4..(row + 1) * width as usize * 4];
                let (source, _) = source.as_chunks::<3>();
                let (target, _) = target.as_chunks_mut::<4>();
                for (bgr, rgba) in source.iter().zip(target) {
                    *rgba = [bgr[2], bgr[1], bgr[0], 0xFF];
                }
            }
            (lib.bitmap_destroy)(bitmap);
            Ok(Arc::new(Page {
                width,
                height,
                rgba,
            }))
        };
        (lib.close_page)(page);
        (lib.close_document)(document);
        rendered.map(|page| (page, pages))
    }
}

/// Runs one queued job, publishing its result.
fn run_job(job: Job) {
    match render(&job.path, job.page, job.zoom) {
        Ok((page, pages)) => {
            let mut cache = cache()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let key = format!("{}|{}|{:.3}", job.path.display(), job.page, job.zoom);
            cache.pages.insert(key.clone(), page);
            cache.order.push(key);
            cache.evict();
            cache.counts.insert(job.path.clone(), pages);
            cache.failed.retain(|failed| failed != &job.path);
            drop(cache);
            job.ctx.request_repaint();
        }
        Err(error) => {
            log::debug!(
                "pdf render failed for {} page {}: {error}",
                job.path.display(),
                job.page
            );
            let mut cache = cache()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !cache.failed.contains(&job.path) {
                cache.failed.push(job.path.clone());
            }
            drop(cache);
            job.ctx.request_repaint();
        }
    }
}

/// Ensures a worker thread is draining the queue.
fn spawn_worker() {
    static WORKERS: OnceLock<Mutex<u32>> = OnceLock::new();
    static MAX_WORKERS: u32 = 2;
    let mut alive = WORKERS
        .get_or_init(|| Mutex::new(0))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *alive >= MAX_WORKERS {
        return;
    }
    *alive += 1;
    drop(alive);
    let _ = std::thread::Builder::new()
        .name("pdf-render".into())
        .spawn(move || {
            loop {
                let job = queue()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .pop();
                let Some(job) = job else {
                    let mut alive = WORKERS
                        .get_or_init(|| Mutex::new(0))
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    *alive -= 1;
                    return;
                };
                run_job(job);
            }
        });
}

/// The handful of pdfium entry points page rendering needs.
struct Library {
    // Held so the mapped library outlives every function pointer below.
    #[allow(dead_code)]
    lib: libloading::Library,
    init_library: unsafe extern "C" fn(),
    load_mem_document: unsafe extern "C" fn(
        data: *const u8,
        size: i32,
        password: *const u8,
    ) -> *mut std::ffi::c_void,
    get_page_count: unsafe extern "C" fn(document: *mut std::ffi::c_void) -> i32,
    load_page:
        unsafe extern "C" fn(document: *mut std::ffi::c_void, index: i32) -> *mut std::ffi::c_void,
    page_width: unsafe extern "C" fn(page: *mut std::ffi::c_void) -> f32,
    page_height: unsafe extern "C" fn(page: *mut std::ffi::c_void) -> f32,
    close_page: unsafe extern "C" fn(page: *mut std::ffi::c_void),
    close_document: unsafe extern "C" fn(document: *mut std::ffi::c_void),
    bitmap_create:
        unsafe extern "C" fn(width: i32, height: i32, alpha: i32) -> *mut std::ffi::c_void,
    bitmap_fill_rect: unsafe extern "C" fn(
        bitmap: *mut std::ffi::c_void,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
        color: u32,
    ),
    render_page_bitmap: unsafe extern "C" fn(
        bitmap: *mut std::ffi::c_void,
        page: *mut std::ffi::c_void,
        start_x: i32,
        start_y: i32,
        size_x: i32,
        size_y: i32,
        rotate: i32,
        flags: i32,
    ),
    bitmap_stride: unsafe extern "C" fn(bitmap: *mut std::ffi::c_void) -> i32,
    bitmap_buffer: unsafe extern "C" fn(bitmap: *mut std::ffi::c_void) -> *mut u8,
    bitmap_destroy: unsafe extern "C" fn(bitmap: *mut std::ffi::c_void),
    initialized: std::sync::atomic::AtomicBool,
}

impl Library {
    /// Opens the first pdfium library the platform's loader can find.
    fn open() -> Option<Self> {
        #[cfg(windows)]
        let names = ["pdfium.dll"];
        #[cfg(target_os = "macos")]
        let names = ["libpdfium.dylib"];
        #[cfg(all(unix, not(target_os = "macos")))]
        let names = ["libpdfium.so"];
        for name in names {
            let Ok(lib) = (unsafe { libloading::Library::new(name) }) else {
                continue;
            };
            // Typed lookups; a missing symbol means this is not pdfium.
            let lookup = |name: &'static [u8]| -> Option<*mut std::ffi::c_void> {
                unsafe { lib.get::<*mut std::ffi::c_void>(name) }
                    .ok()
                    .map(|symbol| *symbol)
            };
            let required = [
                b"FPDF_InitLibrary\0".as_slice(),
                b"FPDF_LoadMemDocument\0".as_slice(),
                b"FPDF_GetPageCount\0".as_slice(),
                b"FPDF_LoadPage\0".as_slice(),
                b"FPDF_GetPageWidthF\0".as_slice(),
                b"FPDF_GetPageHeightF\0".as_slice(),
                b"FPDF_ClosePage\0".as_slice(),
                b"FPDF_CloseDocument\0".as_slice(),
                b"FPDFBitmap_Create\0".as_slice(),
                b"FPDFBitmap_FillRect\0".as_slice(),
                b"FPDF_RenderPageBitmap\0".as_slice(),
                b"FPDFBitmap_GetStride\0".as_slice(),
                b"FPDFBitmap_GetBuffer\0".as_slice(),
                b"FPDFBitmap_Destroy\0".as_slice(),
            ];
            if required.iter().any(|name| lookup(name).is_none()) {
                continue;
            }
            macro_rules! fetch {
                ($name:literal, $signature:ty) => {
                    unsafe { *lib.get::<$signature>($name).ok()? }
                };
            }
            let init_library: unsafe extern "C" fn() =
                fetch!(b"FPDF_InitLibrary\0", unsafe extern "C" fn());
            let load_mem_document: unsafe extern "C" fn(
                *const u8,
                i32,
                *const u8,
            ) -> *mut std::ffi::c_void = fetch!(
                b"FPDF_LoadMemDocument\0",
                unsafe extern "C" fn(*const u8, i32, *const u8) -> *mut std::ffi::c_void
            );
            let get_page_count: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32 = fetch!(
                b"FPDF_GetPageCount\0",
                unsafe extern "C" fn(*mut std::ffi::c_void) -> i32
            );
            let load_page: unsafe extern "C" fn(
                *mut std::ffi::c_void,
                i32,
            ) -> *mut std::ffi::c_void = fetch!(
                b"FPDF_LoadPage\0",
                unsafe extern "C" fn(*mut std::ffi::c_void, i32) -> *mut std::ffi::c_void
            );
            let page_width: unsafe extern "C" fn(*mut std::ffi::c_void) -> f32 = fetch!(
                b"FPDF_GetPageWidthF\0",
                unsafe extern "C" fn(*mut std::ffi::c_void) -> f32
            );
            let page_height: unsafe extern "C" fn(*mut std::ffi::c_void) -> f32 = fetch!(
                b"FPDF_GetPageHeightF\0",
                unsafe extern "C" fn(*mut std::ffi::c_void) -> f32
            );
            let close_page: unsafe extern "C" fn(*mut std::ffi::c_void) = fetch!(
                b"FPDF_ClosePage\0",
                unsafe extern "C" fn(*mut std::ffi::c_void)
            );
            let close_document: unsafe extern "C" fn(*mut std::ffi::c_void) = fetch!(
                b"FPDF_CloseDocument\0",
                unsafe extern "C" fn(*mut std::ffi::c_void)
            );
            let bitmap_create: unsafe extern "C" fn(i32, i32, i32) -> *mut std::ffi::c_void = fetch!(
                b"FPDFBitmap_Create\0",
                unsafe extern "C" fn(i32, i32, i32) -> *mut std::ffi::c_void
            );
            let bitmap_fill_rect: unsafe extern "C" fn(
                *mut std::ffi::c_void,
                i32,
                i32,
                i32,
                i32,
                u32,
            ) = fetch!(
                b"FPDFBitmap_FillRect\0",
                unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32, i32, i32, u32)
            );
            let render_page_bitmap: unsafe extern "C" fn(
                *mut std::ffi::c_void,
                *mut std::ffi::c_void,
                i32,
                i32,
                i32,
                i32,
                i32,
                i32,
            ) = fetch!(
                b"FPDF_RenderPageBitmap\0",
                unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    *mut std::ffi::c_void,
                    i32,
                    i32,
                    i32,
                    i32,
                    i32,
                    i32,
                )
            );
            let bitmap_stride: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32 = fetch!(
                b"FPDFBitmap_GetStride\0",
                unsafe extern "C" fn(*mut std::ffi::c_void) -> i32
            );
            let bitmap_buffer: unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut u8 = fetch!(
                b"FPDFBitmap_GetBuffer\0",
                unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut u8
            );
            let bitmap_destroy: unsafe extern "C" fn(*mut std::ffi::c_void) = fetch!(
                b"FPDFBitmap_Destroy\0",
                unsafe extern "C" fn(*mut std::ffi::c_void)
            );
            return Some(Self {
                lib,
                init_library,
                load_mem_document,
                get_page_count,
                load_page,
                page_width,
                page_height,
                close_page,
                close_document,
                bitmap_create,
                bitmap_fill_rect,
                render_page_bitmap,
                bitmap_stride,
                bitmap_buffer,
                bitmap_destroy,
                initialized: std::sync::atomic::AtomicBool::new(false),
            });
        }
        None
    }

    /// Initializes the pdfium library once for the process.
    fn init_once(&self) {
        if !self
            .initialized
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            unsafe { (self.init_library)() }
        }
    }

    fn load_document(&self, data: &[u8]) -> Result<*mut std::ffi::c_void, String> {
        // SAFETY: `data` outlives the call; pdfium copies what it keeps.
        let document =
            unsafe { (self.load_mem_document)(data.as_ptr(), data.len() as i32, std::ptr::null()) };
        if document.is_null() {
            return Err("could not open the document".to_owned());
        }
        Ok(document)
    }
}

// The raw function pointers are plain C entry points; the library handle keeps
// them alive for the process lifetime.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}
