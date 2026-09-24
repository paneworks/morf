//! Handing finished image work back to the configuration.
//!
//! Every answer has one shape, `on_done(true, result)` or
//! `on_done(false, message)`, whichever call asked: a configuration writes
//! one kind of handler and learns it once.

use luna::{Context, Executor, StashedClosure, Table, Value as LuaValue, Variadic};

use crate::api_image_ops::rgba_color;
use crate::image_jobs::{CaptureSave, ImageJob, ImageOutcome, RawCapture};
use crate::{reactive_execute::drive_executor, surface_types::Screencopy, types::*};

impl Runtime {
    /// Runs the callbacks of every image job that has finished.
    ///
    /// Returns how many ran. Whether the scene changed is for the caller to
    /// ask the scene, as it does for every other service.
    pub(crate) fn poll_image_jobs(&mut self) -> usize {
        let done = self.reactive.borrow_mut().image_jobs.drain();
        let count = done.len();
        for (callback, result) in done {
            if let Err(message) = self
                .run_handler(|ctx, limits| execute_image_handler(ctx, &callback, result, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("image callback: {message}"));
            }
        }
        count
    }

    /// Whether a capture should be published where `ui.Image` can find it.
    ///
    /// Not one headed for a file: nothing would ever release it, and a
    /// screenshot bound for disk has no business holding a screen's worth of
    /// memory in the renderer as well.
    pub fn screencopy_publishes(&self, request_id: u64) -> bool {
        !self
            .reactive
            .borrow()
            .screencopy_saves
            .contains_key(&request_id)
    }

    /// Sends a capture meant for a file to the image workers.
    ///
    /// The result is handed back when the request was an ordinary capture,
    /// for the caller to deliver to Lua as before.
    pub(crate) fn save_screencopy(
        &mut self,
        request_id: u64,
        result: Result<Screencopy, String>,
    ) -> Result<bool, Result<Screencopy, String>> {
        let Some(save) = self
            .reactive
            .borrow_mut()
            .screencopy_saves
            .remove(&request_id)
        else {
            return Err(result);
        };
        let callback = self
            .reactive
            .borrow_mut()
            .screencopy_callbacks
            .remove(&request_id);
        let failure = match result {
            Err(error) => Some(error),
            Ok(frame) if frame.gpu || frame.pixels.is_empty() => {
                Some("the capture arrived with no pixels to write".to_owned())
            }
            Ok(frame) => {
                let CaptureSave {
                    path,
                    region,
                    format,
                    quality,
                } = save;
                let job = ImageJob::SaveCapture {
                    capture: RawCapture {
                        width: frame.width,
                        height: frame.height,
                        stride: frame.stride,
                        opaque: frame.format != "argb8888",
                        y_invert: frame.y_invert,
                        pixels: frame.pixels,
                    },
                    region,
                    path,
                    format,
                    quality,
                };
                self.reactive
                    .borrow_mut()
                    .image_jobs
                    .submit(job, callback.clone())
                    .err()
            }
        };
        if let (Some(message), Some(callback)) = (failure, callback) {
            let result = Err(message);
            if let Err(message) = self
                .run_handler(|ctx, limits| execute_image_handler(ctx, &callback, result, limits))
            {
                self.reactive.borrow_mut().log(
                    LogLevel::Warn,
                    format!("screencopy save callback: {message}"),
                );
            }
        }
        Ok(true)
    }
}

fn execute_image_handler(
    ctx: Context<'_>,
    closure: &StashedClosure,
    result: Result<ImageOutcome, String>,
    limits: Limits,
) -> Result<(), String> {
    let args = match result {
        Ok(outcome) => {
            let value = match outcome {
                ImageOutcome::Info(info, path) => {
                    let table = Table::new(&ctx);
                    table.set_field(ctx, "width", i64::from(info.width));
                    table.set_field(ctx, "height", i64::from(info.height));
                    table.set_field(ctx, "format", info.format.as_str());
                    table.set_field(ctx, "path", path.to_string_lossy().as_ref());
                    LuaValue::Table(table)
                }
                ImageOutcome::Pixel(rgba) => rgba_color(ctx, rgba),
                ImageOutcome::Palette(entries) => {
                    let list = Table::new(&ctx);
                    for (index, entry) in entries.iter().enumerate() {
                        let item = Table::new(&ctx);
                        item.set_field(ctx, "color", rgba_color(ctx, entry.rgba));
                        item.set_field(ctx, "fraction", entry.fraction);
                        list.set(ctx, index as i64 + 1, item)
                            .map_err(|error| error.to_string())?;
                    }
                    LuaValue::Table(list)
                }
            };
            Variadic(vec![LuaValue::Boolean(true), value])
        }
        Err(message) => Variadic(vec![
            LuaValue::Boolean(false),
            LuaValue::String(ctx.intern(message.as_bytes())),
        ]),
    };
    let executor = Executor::start(ctx, ctx.fetch(closure).into(), args);
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
