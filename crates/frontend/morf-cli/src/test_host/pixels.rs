//! What the configuration drew, read back: a frame rendered on the GPU and
//! kept by the number it was handed out as, one pixel of it, or the colours
//! of a region of it summed up.

use morf_host::headless::render::{Picture, render_picture};
use morf_value::IpcValue;

use super::TestHost;
use super::values::{map, number, optional_text, string};

/// Renders the subject now and keeps the picture: `(true, id, width,
/// height)`, or `(false, why)` where there is no GPU.
pub(super) fn frame(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    if let Some(reason) = &host.no_gpu {
        return Ok(vec![IpcValue::Boolean(false), string(reason.clone())]);
    }
    let wanted = optional_text(arguments.first());
    let scale = host.defaults.scale;
    match render_picture(host.subject()?, wanted.as_deref(), scale) {
        Ok((picture, _)) => {
            let size = (picture.width, picture.height);
            host.frames.push(picture);
            Ok(vec![
                IpcValue::Boolean(true),
                IpcValue::Integer(host.frames.len() as i64),
                IpcValue::Integer(i64::from(size.0)),
                IpcValue::Integer(i64::from(size.1)),
            ])
        }
        Err(error) if error.starts_with("no GPU") => {
            host.notes.push(format!("frame skipped: {error}"));
            host.no_gpu = Some(error.clone());
            Ok(vec![IpcValue::Boolean(false), string(error)])
        }
        Err(error) => Err(error),
    }
}

fn kept<'a>(host: &'a TestHost, id: Option<&IpcValue>) -> Result<&'a Picture, String> {
    let id = number(id, "frame")? as usize;
    id.checked_sub(1)
        .and_then(|index| host.frames.get(index))
        .ok_or_else(|| format!("no frame {id}"))
}

/// A pixel's straight RGBA, after checking it lies inside the picture.
fn rgba(picture: &Picture, x: f64, y: f64) -> Result<[u8; 4], String> {
    if x < 0.0 || y < 0.0 || x >= f64::from(picture.width) || y >= f64::from(picture.height) {
        return Err(format!(
            "({x}, {y}) is outside the {}x{} frame",
            picture.width, picture.height
        ));
    }
    let offset = ((y as u32 * picture.width + x as u32) * 4) as usize;
    Ok(picture.pixels[offset..offset + 4]
        .try_into()
        .expect("four channels"))
}

fn hex([r, g, b, a]: [f64; 4]) -> String {
    let byte = |channel: f64| channel.round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b), byte(a))
}

/// Rec. 709 luma of an encoded pixel, from 0 to 1.
fn luma(pixel: [u8; 4]) -> f64 {
    (0.2126 * f64::from(pixel[0]) + 0.7152 * f64::from(pixel[1]) + 0.0722 * f64::from(pixel[2]))
        / 255.0
}

/// `(id, x, y)`: the pixel there as `#rrggbbaa`.
pub(super) fn pixel(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let picture = kept(host, arguments.first())?;
    let x = number(arguments.get(1), "x")?;
    let y = number(arguments.get(2), "y")?;
    let pixel = rgba(picture, x, y)?;
    Ok(vec![string(hex(pixel.map(f64::from)))])
}

/// `(id, x, y, width, height)`: the region's mean colour and its luma's
/// mean, variance, least and most.
pub(super) fn region(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let picture = kept(host, arguments.first())?;
    let x = number(arguments.get(1), "x")?;
    let y = number(arguments.get(2), "y")?;
    let width = number(arguments.get(3), "width")?.max(1.0);
    let height = number(arguments.get(4), "height")?.max(1.0);
    rgba(picture, x + width - 1.0, y + height - 1.0)?;
    let (mut sum, mut lumas) = ([0.0; 4], Vec::new());
    for row in y as u32..(y + height) as u32 {
        for column in x as u32..(x + width) as u32 {
            let pixel = rgba(picture, f64::from(column), f64::from(row))?;
            for (total, channel) in sum.iter_mut().zip(pixel) {
                *total += f64::from(channel);
            }
            lumas.push(luma(pixel));
        }
    }
    let count = lumas.len() as f64;
    let mean = lumas.iter().sum::<f64>() / count;
    let variance = lumas.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / count;
    let least = lumas.iter().copied().fold(f64::INFINITY, f64::min);
    let most = lumas.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok(vec![map(vec![
        ("mean", string(hex(sum.map(|total| total / count)))),
        ("luma", IpcValue::Number(mean)),
        ("variance", IpcValue::Number(variance)),
        ("min", IpcValue::Number(least)),
        ("max", IpcValue::Number(most)),
    ])])
}
