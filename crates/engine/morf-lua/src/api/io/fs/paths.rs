//! `morf.fs` calls about paths rather than files: glob and match, expand,
//! normalize, join and take apart, the home and well-known folders, and disk
//! usage.

use super::*;

/// Installs the path calls on `fs`.
pub(super) fn install_paths<'gc>(ctx: Context<'gc>, fs: Table<'gc>) {
    fs.set_field(
        ctx,
        "glob",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let pattern: String = stack.consume(ctx)?;
            ops::check_pattern(&pattern).map_err(HostError)?;
            let (found, truncated) = ops::glob(&pattern);
            let out = Table::new(&ctx);
            for (index, path) in found.iter().enumerate() {
                out.set(ctx, index as i64 + 1, text(path))?;
            }
            stack.replace(ctx, (out, truncated));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "matches",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (pattern, name): (String, String) = stack.consume(ctx)?;
            stack.replace(ctx, ops::matches(&pattern, &name));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "expand",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let path: String = stack.consume(ctx)?;
            stack.replace(ctx, ops::expand(&path));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "normalize",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let path: String = stack.consume(ctx)?;
            stack.replace(ctx, text(&ops::normalize(Path::new(&path))));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "join",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let mut joined = PathBuf::new();
            for index in 0..stack.len() {
                match stack.get(index) {
                    LuaValue::String(part) => joined.push(OsStr::from_bytes(part.as_bytes())),
                    LuaValue::Integer(part) => joined.push(part.to_string()),
                    _ => return Err(HostError("fs.join takes strings".into()).into()),
                }
            }
            stack.replace(ctx, text(&joined));
            Ok(CallbackReturn::Return)
        }),
    );

    for (name, part) in [
        ("basename", ops::PathPart::Basename),
        ("dirname", ops::PathPart::Dirname),
        ("extension", ops::PathPart::Extension),
        ("stem", ops::PathPart::Stem),
    ] {
        fs.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let path: String = stack.consume(ctx)?;
                stack.replace(ctx, ops::path_part(Path::new(&path), part));
                Ok(CallbackReturn::Return)
            }),
        );
    }

    fs.set_field(
        ctx,
        "home",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            stack.replace(ctx, ops::home_dir().map(|home| text(&home)));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "dir",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let kind: String = stack.consume(ctx)?;
            stack.replace(ctx, ops::user_dir(&kind).map(|dir| text(&dir)));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "disk",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let path: LuaValue = stack.consume(ctx)?;
            let path = path_of(path, "fs.disk")?;
            match ops::disk_usage(&path) {
                Ok((total, free, available)) => {
                    let out = Table::new(&ctx);
                    out.set_field(ctx, "total", total as i64);
                    out.set_field(ctx, "free", free as i64);
                    out.set_field(ctx, "available", available as i64);
                    out.set_field(ctx, "used", total.saturating_sub(free) as i64);
                    stack.replace(ctx, out);
                }
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );
}
