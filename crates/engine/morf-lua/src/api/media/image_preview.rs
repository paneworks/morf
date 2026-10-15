//! Resident, asynchronous previews with explicit close and teardown cleanup.
use crate::runtime::state::StateCell;
use crate::{
    api::media::image_ops::{parse_ops, queue, source_of, uint},
    api::ui::scene::HostError,
    runtime::image_jobs::ImageJob,
    runtime::state::ReactiveState,
};
use luna::{Callback, CallbackReturn, Closure, Context, Table, Value as LuaValue};
use morf_image::preview::Preview;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Weak};

pub(crate) fn install<'gc>(
    ctx: Context<'gc>,
    image: Table<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
) {
    let state = Rc::clone(state);
    let held = Rc::new(RefCell::new(Vec::<Weak<Preview>>::new()));
    image.set_field(
        ctx,
        "preview",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let source: LuaValue = stack.consume(ctx)?;
            let source = source_of(source, "preview source")?;
            let mut held = held.borrow_mut();
            held.retain(|p| p.upgrade().is_some_and(|p| !p.is_closed()));
            if held.len() >= 4 {
                return Err(
                    HostError("at most four native image previews may be open".into()).into(),
                );
            }
            let preview = Arc::new(Preview::new(source));
            held.push(Arc::downgrade(&preview));
            let result = Table::new(&ctx);
            let render = Arc::clone(&preview);
            let render_state = Rc::clone(&state);
            result.set_field(
                ctx,
                "render",
                Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    let (ops, cb): (LuaValue, Closure) = stack.consume(ctx)?;
                    let ops = parse_ops(ctx, ops)?;
                    stack.replace(
                        ctx,
                        queue(
                            ctx,
                            &render_state,
                            ImageJob::Preview {
                                preview: Arc::clone(&render),
                                ops,
                            },
                            Some(cb),
                        ),
                    );
                    Ok(CallbackReturn::Return)
                }),
            );
            let cover = Arc::clone(&preview);
            let cover_state = Rc::clone(&state);
            result.set_field(
                ctx,
                "cover",
                Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    let (width, height, scale, cb): (LuaValue, LuaValue, LuaValue, Closure) =
                        stack.consume(ctx)?;
                    let width = uint(width, "cover width")?;
                    let height = uint(height, "cover height")?;
                    let scale_120 = uint(scale, "cover scale in 120ths")?;
                    if width == 0 || height == 0 || scale_120 == 0 {
                        return Err(HostError(
                            "cover dimensions and scale must be positive".into(),
                        )
                        .into());
                    }
                    stack.replace(
                        ctx,
                        queue(
                            ctx,
                            &cover_state,
                            ImageJob::PreviewCover {
                                preview: Arc::clone(&cover),
                                width,
                                height,
                                scale_120,
                            },
                            Some(cb),
                        ),
                    );
                    Ok(CallbackReturn::Return)
                }),
            );
            result.set_field(
                ctx,
                "close",
                Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    preview.close();
                    stack.replace(ctx, true);
                    Ok(CallbackReturn::Return)
                }),
            );
            stack.replace(ctx, result);
            Ok(CallbackReturn::Return)
        }),
    );
}
