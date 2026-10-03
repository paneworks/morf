//! Native canvases share the bounded asynchronous image queue.
use crate::{
    api_color::color_of,
    api_image_ops::{callback_of, format_of, parse_ops, quality_of, queue, source_of, uint},
    image_jobs::ImageJob,
    scene_bindings::HostError,
    state::ReactiveState,
};
use luna::{Callback, CallbackReturn, Context, Table, Value};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn install<'gc>(
    ctx: Context<'gc>,
    image: Table<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
) {
    let state = Rc::clone(state);
    image.set_field(
        ctx,
        "compose",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let options: Value = stack.consume(ctx)?;
            let Value::Table(options) = options else {
                return Err(HostError("image.compose takes a table of options".into()).into());
            };
            let width = uint(options.get_value(ctx, "width"), "canvas width")?;
            let height = uint(options.get_value(ctx, "height"), "canvas height")?;
            morf_image::ops::check_size(width, height).map_err(|e| HostError(e.to_string()))?;
            let background = match options.get_value(ctx, "background") {
                Value::Nil => [0, 0, 0, 255],
                value => {
                    let color = color_of(ctx, value).map_err(HostError)?.to_rgba();
                    [
                        color.r,
                        color.g,
                        color.b,
                        (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                    ]
                }
            };
            let output = source_of(options.get_value(ctx, "output"), "image.compose output")?;
            let request = morf_image::canvas::Request {
                width,
                height,
                background,
                ops: parse_ops(ctx, options.get_value(ctx, "ops"))?,
                format: format_of(options.get_value(ctx, "format"), &output)?,
                quality: quality_of(options.get_value(ctx, "quality"))?,
                output,
            };
            let callback = callback_of(options.get_value(ctx, "on_done"), "on_done")?;
            stack.replace(
                ctx,
                queue(ctx, &state, ImageJob::Compose(request), callback),
            );
            Ok(CallbackReturn::Return)
        }),
    );
}
