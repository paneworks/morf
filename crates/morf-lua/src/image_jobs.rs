//! Image work, off the thread that draws.
//!
//! Decoding a photograph, scaling it and encoding the result takes tens to
//! hundreds of milliseconds — several frames — and the Lua thread is the one
//! producing the frames. So `morf.image` hands the work to a small pool of
//! threads and the answer comes back the way every other service's does:
//! through a channel the runtime drains in `poll_services`, with a poke at
//! the loop's [`morf_io::wake_all`] so it is collected at once rather than
//! at the next unrelated wake-up.
//!
//! Bounded twice. Two workers, because this is a shell and not a batch
//! converter, and a configuration that queues a hundred thumbnails should
//! not take every core from the compositor. And a cap on work in flight, so
//! a loop that queues faster than the workers drain is told no instead of
//! growing a queue without end.
//!
//! The same pool reads files that may block: `morf.fs.read_async`. A
//! sensor under `/sys/class/hwmon` can take tens of milliseconds to answer
//! a read -- the firmware is asked, not a cache -- and a shell that samples
//! a dozen of them on the drawing thread stalls every animation behind it.
//!
//! Teardown-safe by construction: the callbacks live here, in the runtime's
//! state, and the workers only ever hold job data and a sender. When the
//! runtime goes, the callbacks go with it; a worker that finishes afterwards
//! finds nobody listening and stops.

use luna::StashedClosure;
use morf_image::PaletteEntry;
use morf_image::ops::{self, ImageInfo, OutputFormat, ProcessRequest};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// How many jobs may be queued or running at once.
pub(crate) const MAX_IN_FLIGHT: usize = 32;
/// How many threads do the work.
const WORKERS: usize = 2;

/// A capture's pixels as the compositor handed them over.
pub(crate) struct RawCapture {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    /// `xrgb8888`: the fourth byte is padding, not alpha.
    pub(crate) opaque: bool,
    pub(crate) y_invert: bool,
    pub(crate) pixels: Vec<u8>,
}

/// A rectangle of a picture, in its pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Region {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Where a capture asked for by `morf.screencopy.save` is to be written.
pub(crate) struct CaptureSave {
    pub(crate) path: PathBuf,
    pub(crate) region: Option<Region>,
    pub(crate) format: OutputFormat,
    pub(crate) quality: u8,
}

pub(crate) enum ImageJob {
    Process(ProcessRequest),
    Pixel {
        source: PathBuf,
        x: u32,
        y: u32,
    },
    Palette {
        source: PathBuf,
        count: usize,
    },
    SaveCapture {
        capture: RawCapture,
        region: Option<Region>,
        path: PathBuf,
        format: OutputFormat,
        quality: u8,
    },
    /// Files read whole, each up to `limit` bytes, in order.
    ReadFiles {
        paths: Vec<PathBuf>,
        limit: u64,
    },
}

/// What a finished job hands its callback.
pub(crate) enum ImageOutcome {
    Info(ImageInfo, PathBuf),
    Pixel([u8; 4]),
    Palette(Vec<PaletteEntry>),
    /// Each file's bytes, or `None` where it could not be read.
    Files(Vec<Option<Vec<u8>>>),
}

type Finished = (u64, Result<ImageOutcome, String>);

/// The pool, and who is owed each answer.
pub(crate) struct ImageJobs {
    jobs: Option<Sender<(u64, ImageJob)>>,
    results: Receiver<Finished>,
    results_sender: Sender<Finished>,
    /// `None` for a job nobody asked to hear back from; kept all the same,
    /// because it still counts against the cap.
    callbacks: HashMap<u64, Option<StashedClosure>>,
    next_id: u64,
}

impl Default for ImageJobs {
    fn default() -> Self {
        let (results_sender, results) = mpsc::channel();
        Self {
            jobs: None,
            results,
            results_sender,
            callbacks: HashMap::new(),
            next_id: 0,
        }
    }
}

impl ImageJobs {
    /// Queues a job; its callback is owed the answer.
    ///
    /// The workers are started on the first job, so a configuration that
    /// never touches `morf.image` never has the threads.
    pub(crate) fn submit(
        &mut self,
        job: ImageJob,
        callback: Option<StashedClosure>,
    ) -> Result<(), String> {
        if self.callbacks.len() >= MAX_IN_FLIGHT {
            return Err(format!(
                "image work queue is full ({MAX_IN_FLIGHT} jobs in flight)"
            ));
        }
        let sender = match &self.jobs {
            Some(sender) => sender.clone(),
            None => {
                let sender = self.start()?;
                self.jobs = Some(sender.clone());
                sender
            }
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        sender
            .send((id, job))
            .map_err(|_| "image workers have stopped".to_owned())?;
        self.callbacks.insert(id, callback);
        Ok(())
    }

    fn start(&self) -> Result<Sender<(u64, ImageJob)>, String> {
        let (sender, receiver) = mpsc::channel::<(u64, ImageJob)>();
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..WORKERS {
            let receiver = Arc::clone(&receiver);
            let results = self.results_sender.clone();
            std::thread::Builder::new()
                .name(format!("morf-image-{index}"))
                .spawn(move || work(&receiver, &results))
                .map_err(|error| format!("cannot start an image worker: {error}"))?;
        }
        Ok(sender)
    }

    /// Takes every finished job, paired with the callback owed it.
    pub(crate) fn drain(&mut self) -> Vec<(StashedClosure, Result<ImageOutcome, String>)> {
        let mut done = Vec::new();
        while let Ok((id, result)) = self.results.try_recv() {
            if let Some(Some(callback)) = self.callbacks.remove(&id) {
                done.push((callback, result));
            }
        }
        done
    }

    /// How many jobs are queued or running.
    #[cfg(test)]
    pub(crate) fn in_flight(&self) -> usize {
        self.callbacks.len()
    }
}

fn work(jobs: &Mutex<Receiver<(u64, ImageJob)>>, results: &Sender<Finished>) {
    loop {
        // The lock is held only while waiting, never while working, so the
        // other worker can take the next job meanwhile.
        let next = jobs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .recv();
        let Ok((id, job)) = next else {
            return;
        };
        // A decoder that panics on a hostile file takes down this job, not
        // the worker and not the shell.
        let result = catch_unwind(AssertUnwindSafe(|| run(job)))
            .unwrap_or_else(|_| Err("the image decoder failed on this file".to_owned()));
        if results.send((id, result)).is_err() {
            return;
        }
        morf_io::wake_all();
    }
}

fn run(job: ImageJob) -> Result<ImageOutcome, String> {
    match job {
        ImageJob::Process(request) => ops::process(&request)
            .map(|info| ImageOutcome::Info(info, request.output))
            .map_err(|error| error.to_string()),
        ImageJob::Pixel { source, x, y } => ops::pixel_at(&source, x, y, u64::MAX)
            .map(ImageOutcome::Pixel)
            .map_err(|error| error.to_string()),
        ImageJob::Palette { source, count } => ops::palette(&source, count)
            .map(ImageOutcome::Palette)
            .map_err(|error| error.to_string()),
        ImageJob::ReadFiles { paths, limit } => Ok(ImageOutcome::Files(
            paths
                .iter()
                .map(|path| morf_io::fs::read(path, limit).ok())
                .collect(),
        )),
        ImageJob::SaveCapture {
            capture,
            region,
            path,
            format,
            quality,
        } => {
            let (width, height, rgba) = capture_rgba(&capture, region)?;
            ops::save_rgba(width, height, rgba, &path, format, quality)
                .map(|info| ImageOutcome::Info(info, path))
                .map_err(|error| error.to_string())
        }
    }
}

/// A capture as straight, top-down RGBA, cut to `region` if one was asked.
///
/// Compositors hand captures over as `argb8888`/`xrgb8888`, which in memory
/// is blue, green, red, alpha — little-endian — with rows padded to `stride`
/// and, on some, stored bottom row first. `xrgb` has no alpha, and taking its
/// padding byte as one makes a screenshot transparent.
pub(crate) fn capture_rgba(
    capture: &RawCapture,
    region: Option<Region>,
) -> Result<(u32, u32, Vec<u8>), String> {
    let full = Region {
        x: 0,
        y: 0,
        width: capture.width,
        height: capture.height,
    };
    let region = region.unwrap_or(full);
    let right = region.x.saturating_add(region.width).min(capture.width);
    let bottom = region.y.saturating_add(region.height).min(capture.height);
    if region.x >= right || region.y >= bottom {
        return Err(format!(
            "region {},{} {}x{} is outside the {}x{} capture",
            region.x, region.y, region.width, region.height, capture.width, capture.height
        ));
    }
    let (width, height) = (right - region.x, bottom - region.y);
    let stride = capture.stride as usize;
    if stride < capture.width as usize * 4 {
        return Err("the capture's stride is shorter than its rows".to_owned());
    }
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for row in region.y..bottom {
        let source_row = if capture.y_invert {
            capture.height - 1 - row
        } else {
            row
        } as usize;
        let start = source_row * stride + region.x as usize * 4;
        let line = capture
            .pixels
            .get(start..start + width as usize * 4)
            .ok_or("the capture has fewer bytes than its size says")?;
        for pixel in line.chunks_exact(4) {
            rgba.extend_from_slice(&[
                pixel[2],
                pixel[1],
                pixel[0],
                if capture.opaque { 255 } else { pixel[3] },
            ]);
        }
    }
    Ok((width, height, rgba))
}
