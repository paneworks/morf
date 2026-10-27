//! `morf.<namespace>` tables, shared by every installer that adds to one.

use luna::{Context, Table, Value as LuaValue};

/// `morf.<name>`, made on the first ask.
pub(crate) fn table<'gc>(ctx: Context<'gc>, morf: Table<'gc>, name: &'static str) -> Table<'gc> {
    if let LuaValue::Table(existing) = morf.get_value(ctx, name) {
        return existing;
    }
    let made = Table::new(&ctx);
    morf.set_field(ctx, name, made);
    made
}
