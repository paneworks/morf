//! Prepare display-sized pixels on an image worker, including vector sources.
use super::{MAX_PREVIEW_BYTES, Preview};
use crate::{ImageCache, ImageData, ImageError};
use std::sync::Arc;

impl Preview {
    /// Rasterize the whole source at the resolution needed to cover a logical
    /// rectangle. The consumer still applies its centered aspect crop.
    /// Unlike an editing render, this does not retain a decoded original.
    pub fn cover(&self, width: u32, height: u32, scale_120: u32) -> Result<ImageData, ImageError> {
        self.cancelled()?;
        if width == 0 || height == 0 || scale_120 == 0 {
            return Err(ImageError::InvalidSize);
        }
        let mut cache = ImageCache::default();
        let (sw, sh) = cache.intrinsic_size(&self.source)?;
        check_bytes(sw, sh)?;
        let ratio = (f64::from(width) / f64::from(sw)).max(f64::from(height) / f64::from(sh));
        let logical = |size| (f64::from(size) * ratio).ceil().max(1.0) as u32;
        let (lw, lh) = (logical(sw), logical(sh));
        let physical = |size: u32| (u64::from(size) * u64::from(scale_120)).div_ceil(120);
        let pw = u32::try_from(physical(lw)).map_err(|_| ImageError::InvalidSize)?;
        let ph = u32::try_from(physical(lh)).map_err(|_| ImageError::InvalidSize)?;
        check_bytes(pw, ph)?;
        self.cancelled()?;
        let pixels = cache.load(&self.source, lw, lh, scale_120)?;
        drop(cache);
        self.cancelled()?;
        Ok(Arc::unwrap_or_clone(pixels))
    }
}

fn check_bytes(width: u32, height: u32) -> Result<(), ImageError> {
    if u64::from(width) * u64::from(height) > MAX_PREVIEW_BYTES / 4 {
        return Err(ImageError::Refused("preview image exceeds 128 MiB".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors_are_rasterized_at_cover_resolution_and_close_retires_pixels() {
        let source = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"8\"><path fill=\"red\" d=\"M0 0h8v8H0z\"/></svg>";
        let preview = Preview::new(source.into());
        let image = preview.cover(100, 100, 180).unwrap();
        assert_eq!((image.width, image.height), (300, 150));
        assert_eq!(&image.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&image.rgba[image.rgba.len() - 4..], &[0, 0, 0, 0]);
        let source = preview.publish(image).unwrap();
        preview.close();
        assert!(crate::published::published(source.strip_prefix("memory:").unwrap()).is_none());
        assert!(preview.cover(100, 100, 120).is_err());
    }

    #[test]
    fn invalid_and_over_budget_requests_are_refused_before_rasterization() {
        let preview = Preview::new(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"/>".into(),
        );
        for (w, h, scale) in [
            (0, 1, 120),
            (1, 0, 120),
            (1, 1, 0),
            (10000, 10000, 120),
            (1, 1, u32::MAX),
        ] {
            assert!(preview.cover(w, h, scale).is_err());
        }
    }
}
