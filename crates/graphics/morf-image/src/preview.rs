//! An editing preview keeps its immutable decoded source until it closes.
//! Workers return pixels; no preview is encoded to disk or decoded by the UI.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::{ImageData, ImageError, ops, published};
use image::DynamicImage;

/// The retained source of a preview must fit this budget (128 MiB).
pub const MAX_PREVIEW_BYTES: u64 = 128 * 1024 * 1024;
/// All retained preview sources and displayed frames share one process budget.
pub const MAX_TOTAL_PREVIEW_BYTES: u64 = 256 * 1024 * 1024;
static HELD_BYTES: AtomicU64 = AtomicU64::new(0);
fn reserve(bytes: u64) -> Result<(), ImageError> {
    HELD_BYTES
        .try_update(Ordering::AcqRel, Ordering::Acquire, |held| {
            held.checked_add(bytes)
                .filter(|total| *total <= MAX_TOTAL_PREVIEW_BYTES)
        })
        .map(|_| ())
        .map_err(|_| ImageError::Refused("native preview memory budget exceeded".into()))
}
fn release(bytes: u64) {
    HELD_BYTES.fetch_sub(bytes, Ordering::AcqRel);
}
fn size(image: &DynamicImage) -> u64 {
    u64::from(image.width()) * u64::from(image.height()) * 4
}

pub struct Preview {
    source: PathBuf,
    base: Mutex<Option<Arc<DynamicImage>>>,
    displayed: Mutex<Option<(String, u64)>>,
    closed: AtomicBool,
    scope: String,
}

impl Preview {
    pub fn new(source: PathBuf) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            source,
            base: Mutex::new(None),
            displayed: Mutex::new(None),
            closed: AtomicBool::new(false),
            scope: format!(
                "preview-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn cancelled(&self) -> Result<(), ImageError> {
        if self.closed.load(Ordering::Acquire) {
            Err(ImageError::Refused("image preview is closed".into()))
        } else {
            Ok(())
        }
    }

    /// Blocking image work. No lock is held while decoding or compositing,
    /// so closing from the UI thread never waits for a worker.
    pub fn render(&self, edits: &[ops::ImageOp]) -> Result<ImageData, ImageError> {
        self.cancelled()?;
        let held = self.base.lock().unwrap().clone();
        let base = match held {
            Some(base) => base,
            None => {
                let info = ops::image_info(&self.source)?;
                if u64::from(info.width) * u64::from(info.height) * 4 > MAX_PREVIEW_BYTES {
                    return Err(ImageError::Refused("preview source exceeds 128 MiB".into()));
                }
                let decoded = Arc::new(ops::decode_bounded(&self.source)?);
                let mut held = self.base.lock().unwrap();
                self.cancelled()?;
                if held.is_none() {
                    reserve(size(&decoded))?;
                    *held = Some(decoded);
                }
                held.as_ref().unwrap().clone()
            }
        };
        self.cancelled()?;
        let image = ops::apply_ops((*base).clone(), edits)?.into_rgba8();
        self.cancelled()?;
        Ok(ImageData {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        })
    }

    /// Called only when a worker's result is delivered, not when it finishes.
    /// A discarded result never publishes anything into the shared registry.
    pub fn publish(&self, image: ImageData) -> Result<String, ImageError> {
        let mut displayed = self.displayed.lock().unwrap();
        self.cancelled()?;
        let bytes = image.rgba.len() as u64;
        if bytes > MAX_PREVIEW_BYTES {
            return Err(ImageError::Refused("preview frame exceeds 128 MiB".into()));
        }
        let old_bytes = displayed.as_ref().map_or(0, |(_, bytes)| *bytes);
        if bytes > old_bytes {
            reserve(bytes - old_bytes)?;
        }
        let source = published::publish(&self.scope, "frame", image);
        if let Some((old, _)) = displayed.replace((source.clone(), bytes)) {
            published::release(&old);
        }
        if old_bytes > bytes {
            release(old_bytes - bytes);
        }
        Ok(source)
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Some(base) = self.base.lock().unwrap().take() {
            release(size(&base));
        }
        if let Some((old, bytes)) = self.displayed.lock().unwrap().take() {
            published::release(&old);
            release(bytes);
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_is_decoded_once_and_published_pixels_have_a_bounded_lifetime() {
        let dir = std::env::temp_dir().join(format!("morf-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("source.png");
        ops::save_rgba(
            8,
            8,
            vec![255; 8 * 8 * 4],
            &path,
            ops::OutputFormat::Png,
            100,
        )
        .unwrap();
        let preview = Preview::new(path.clone());
        let first = preview.publish(preview.render(&[]).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap(); // A second render must use the resident source.
        let second = preview
            .publish(
                preview
                    .render(&[ops::ImageOp::Crop {
                        x: 1,
                        y: 2,
                        width: 3,
                        height: 4,
                    }])
                    .unwrap(),
            )
            .unwrap();
        assert!(published::published(first.strip_prefix("memory:").unwrap()).is_none());
        let image = published::published(second.strip_prefix("memory:").unwrap()).unwrap();
        assert_eq!((image.width, image.height), (3, 4));
        preview.close();
        assert!(published::published(second.strip_prefix("memory:").unwrap()).is_none());
        assert!(preview.render(&[]).is_err());
        assert!(
            preview
                .publish(ImageData {
                    width: 1,
                    height: 1,
                    rgba: vec![0; 4]
                })
                .is_err()
        );
        std::fs::remove_dir(dir).unwrap();
    }
}
