//! Plays a tour on a virtual clock and saves every frame as a PNG, so a video
//! can be assembled from them without recording the screen.

use anyhow::{Context, Result};
use egui::{ColorImage, Event};
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Where and how often to save frames.
#[derive(Clone, Debug)]
pub struct Capture {
    pub dir: PathBuf,
    pub fps: u32,
}

/// Frames drawn before the tour starts, uncaptured, so the window settles at
/// its final size and the first pictures finish loading.
const SETTLE: u64 = 45;
/// PNG encoders working in parallel.
const WRITERS: usize = 8;

type Job = (u64, Arc<ColorImage>);

pub(super) struct Frames {
    fps: u32,
    /// Frames begun, including the settling ones.
    count: u64,
    started: Option<Instant>,
    /// How far the capture fell behind real time in total.
    slipped: Duration,
    requested: u64,
    received: u64,
    queue: Option<mpsc::SyncSender<Job>>,
    writers: Vec<JoinHandle<Result<()>>>,
    done: bool,
}

impl Frames {
    pub(super) fn new(capture: Capture) -> Result<Self> {
        std::fs::create_dir_all(&capture.dir)
            .with_context(|| format!("creating {}", capture.dir.display()))?;
        let (queue, jobs) = mpsc::sync_channel::<Job>(WRITERS * 3);
        let jobs = Arc::new(Mutex::new(jobs));
        let writers = (0..WRITERS)
            .map(|index| {
                let jobs = Arc::clone(&jobs);
                let dir = capture.dir.clone();
                std::thread::Builder::new()
                    .name(format!("tour-frames-{index}"))
                    .spawn(move || -> Result<()> {
                        loop {
                            let job = jobs.lock().unwrap_or_else(|p| p.into_inner()).recv();
                            let Ok((frame, image)) = job else {
                                return Ok(());
                            };
                            save(&dir.join(format!("frame-{frame:05}.png")), &image)?;
                        }
                    })
                    .context("starting a frame writer")
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            fps: capture.fps.max(1),
            count: 0,
            started: None,
            slipped: Duration::ZERO,
            requested: 0,
            received: 0,
            queue: Some(queue),
            writers,
            done: false,
        })
    }

    /// Hands the screenshots that arrived to the writers, holds the frame
    /// until its moment in real time, and sets the virtual clock. Returns the
    /// tour time of this frame, or `None` while settling.
    ///
    /// Real time is kept in step because video playback, the recorder and
    /// typing indicators run on the wall clock. A frame that takes longer
    /// than its share pushes the rest back rather than hurrying them.
    pub(super) fn begin(&mut self, input: &mut egui::RawInput) -> Option<f32> {
        input.events.retain(|event| {
            let Event::Screenshot {
                user_data, image, ..
            } = event
            else {
                return true;
            };
            let Some(&frame) = user_data
                .data
                .as_ref()
                .and_then(|data| data.downcast_ref::<u64>())
            else {
                return true;
            };
            if let Some(queue) = &self.queue {
                let _ = queue.send((frame, Arc::clone(image)));
            }
            self.received += 1;
            false
        });
        let interval = Duration::from_secs(1) / self.fps;
        let now = Instant::now();
        let started = *self.started.get_or_insert(now);
        let due = started + self.slipped + interval * self.count as u32;
        if now < due {
            std::thread::sleep(due - now);
        } else if self.count > SETTLE {
            self.slipped += now - due;
        } else {
            self.slipped = now - started - interval * self.count as u32;
        }
        let fps = f64::from(self.fps);
        input.time = Some(self.count as f64 / fps);
        input.predicted_dt = (1.0 / fps) as f32;
        let frame = self.count.checked_sub(SETTLE);
        self.count += 1;
        frame.map(|frame| (frame as f64 / fps) as f32)
    }

    /// Asks for this frame's picture while the tour runs, and returns true
    /// once, when every picture asked for is saved.
    pub(super) fn end(
        &mut self,
        ctx: &egui::Context,
        at: Option<f32>,
        duration: f32,
        failed: bool,
    ) -> bool {
        if self.done {
            return false;
        }
        ctx.request_repaint();
        let Some(at) = at else {
            return false;
        };
        let frame = (f64::from(at) * f64::from(self.fps)).round() as u64;
        let last = (f64::from(duration) * f64::from(self.fps)) as u64;
        if !failed && frame < last {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                frame,
            )));
            self.requested += 1;
            return false;
        }
        // A picture comes back with the next frame; stop waiting after a
        // few seconds in case the window stopped drawing.
        let waited = frame.saturating_sub(last) > u64::from(self.fps) * 5;
        if self.received < self.requested && !waited {
            return false;
        }
        self.done = true;
        self.queue = None;
        for writer in self.writers.drain(..) {
            match writer.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log::error!("could not save a tour frame: {error:#}"),
                Err(_) => log::error!("a tour frame writer panicked"),
            }
        }
        log::info!(
            "saved {} of {} tour frames at {} fps, {:.1} s behind real time",
            self.received,
            self.requested,
            self.fps,
            self.slipped.as_secs_f32()
        );
        true
    }
}

fn save(path: &std::path::Path, image: &ColorImage) -> Result<()> {
    let [width, height] = image.size;
    let pixels: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|pixel| {
            let [r, g, b, _] = pixel.to_srgba_unmultiplied();
            [r, g, b]
        })
        .collect();
    let file = std::io::BufWriter::new(
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?,
    );
    PngEncoder::new_with_quality(file, CompressionType::Fast, FilterType::Sub)
        .write_image(
            &pixels,
            width as u32,
            height as u32,
            ExtendedColorType::Rgb8,
        )
        .with_context(|| format!("writing {}", path.display()))
}
