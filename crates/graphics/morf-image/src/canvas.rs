//! Compose on native RGBA pixels without parsing a placeholder SVG.
use std::path::PathBuf;

use image::{DynamicImage, Rgba, RgbaImage};

use crate::{
    ImageError,
    ops::{self, ImageInfo, ImageOp, OutputFormat},
};

pub struct Request {
    pub width: u32,
    pub height: u32,
    pub background: [u8; 4],
    pub ops: Vec<ImageOp>,
    pub output: PathBuf,
    pub format: OutputFormat,
    pub quality: u8,
}

/// The caller runs this on the image worker pool, including allocation.
pub fn compose(request: &Request) -> Result<ImageInfo, ImageError> {
    ops::check_size(request.width, request.height)?;
    let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
        request.width,
        request.height,
        Rgba(request.background),
    ));
    let image = ops::apply_ops(image, &request.ops)?;
    ops::encode_to_file(&image, &request.output, request.format, request.quality)?;
    Ok(ImageInfo {
        width: image.width(),
        height: image.height(),
        format: request.format.name().to_owned(),
    })
}
