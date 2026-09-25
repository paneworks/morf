//! HCT in `morf.color`: hue, chroma and tone, the space Material's colour
//! schemes are made in, and tonal palettes along its tone axis. The colour
//! science is `morf_scene::hct` (a port of Material Color Utilities).
//!
//! ```lua
//! local c = morf.color.hct(265, 48, 40)       -- a colour; chroma sRGB lacks is given up
//! local h, chroma, tone = c:hct()
//! local primary = morf.color.tonal_palette(265, 48)   -- or tonal_palette(some_colour)
//! primary(40)  primary[90]  primary:tone(99)  primary.hue  primary.chroma
//! ```

use luna::{Callback, CallbackReturn, Context, Table, UserRef, Value as LuaValue};
use morf_scene::hct;
use pastel::Color;

use crate::api_color::{ColorToken, color_of, color_userdata};
use crate::scene_bindings::*;

/// The colour of a hue, chroma and tone, in sRGB.
pub(crate) fn from_hct(hue: f64, chroma: f64, tone: f64, alpha: f64) -> Color {
    let [r, g, b] = hct::solve(hue, chroma.max(0.0), tone.clamp(0.0, 100.0));
    Color::from_rgba_float(r, g, b, alpha.clamp(0.0, 1.0))
}

/// A colour's hue, chroma and tone.
pub(crate) fn to_hct(color: &Color) -> [f64; 3] {
    let rgba = color.to_rgba_float();
    hct::hct_from_srgb([rgba.r, rgba.g, rgba.b])
}

fn number<'gc>(value: LuaValue<'gc>, what: &str) -> Result<f64, HostError> {
    match value {
        LuaValue::Integer(value) => Ok(value as f64),
        LuaValue::Number(value) if value.is_finite() => Ok(value),
        _ => Err(HostError(format!("{what} must be a finite number"))),
    }
}

pub(crate) fn install_hct_methods<'gc>(ctx: Context<'gc>, methods: Table<'gc>) {
    let method = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<ColorToken> = stack.consume(ctx)?;
        let [hue, chroma, tone] = to_hct(&token.color);
        stack.replace(ctx, (hue, chroma, tone));
        Ok(CallbackReturn::Return)
    });
    methods.set_field(ctx, "hct", method);
}

pub(crate) fn install_hct_constructors<'gc>(ctx: Context<'gc>, color: Table<'gc>) {
    let constructor = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (hue, chroma, tone, alpha): (LuaValue, LuaValue, LuaValue, Option<f64>) =
            stack.consume(ctx)?;
        let made = from_hct(
            number(hue, "hue")?,
            number(chroma, "chroma")?,
            number(tone, "tone")?,
            alpha.unwrap_or(1.0),
        );
        stack.replace(ctx, color_userdata(ctx, made));
        Ok(CallbackReturn::Return)
    });
    color.set_field(ctx, "hct", constructor);

    // A palette is a table: `hue` and `chroma` fields, `tone(t)` as a
    // method, and callable or indexable by tone.
    let tone = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (palette, tone): (Table, LuaValue) = stack.consume(ctx)?;
        let hue = number(palette.get_value(ctx, "hue"), "palette hue")?;
        let chroma = number(palette.get_value(ctx, "chroma"), "palette chroma")?;
        let tone = number(tone, "tone")?;
        stack.replace(ctx, color_userdata(ctx, from_hct(hue, chroma, tone, 1.0)));
        Ok(CallbackReturn::Return)
    });
    let index = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (palette, key): (Table, LuaValue) = stack.consume(ctx)?;
        let value = match key {
            LuaValue::Integer(_) | LuaValue::Number(_) => {
                let hue = number(palette.get_value(ctx, "hue"), "palette hue")?;
                let chroma = number(palette.get_value(ctx, "chroma"), "palette chroma")?;
                color_userdata(ctx, from_hct(hue, chroma, number(key, "tone")?, 1.0))
            }
            _ => LuaValue::Nil,
        };
        stack.replace(ctx, value);
        Ok(CallbackReturn::Return)
    });
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__call", tone);
    metatable.set_field(ctx, "__tone", tone);
    metatable.set_field(ctx, "__index", index);
    metatable.set_field(ctx, "__name", "tonal_palette");
    let metatable = ctx.stash(metatable);

    let palette = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (first, second): (LuaValue, LuaValue) = stack.consume(ctx)?;
        let (hue, chroma) = match (first, second) {
            // From a key colour: its hue and chroma.
            (first, LuaValue::Nil)
                if !matches!(first, LuaValue::Integer(_) | LuaValue::Number(_)) =>
            {
                let key = color_of(ctx, first).map_err(HostError)?;
                let [hue, chroma, _] = to_hct(&key);
                (hue, chroma)
            }
            (hue, chroma) => (number(hue, "hue")?, number(chroma, "chroma")?),
        };
        let table = Table::new(&ctx);
        table.set_field(ctx, "hue", hue);
        table.set_field(ctx, "chroma", chroma.max(0.0));
        let metatable = ctx.fetch(&metatable);
        table.set_field(ctx, "tone", metatable.get_value(ctx, "__tone"));
        table.set_metatable(ctx, Some(metatable));
        stack.replace(ctx, table);
        Ok(CallbackReturn::Return)
    });
    color.set_field(ctx, "tonal_palette", palette);
}
