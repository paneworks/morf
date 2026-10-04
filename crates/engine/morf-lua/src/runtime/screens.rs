use luna::{Table, Value as LuaValue};

use crate::{api_host::*, types::*};

impl Runtime {
    /// Replaces `morf.screens` with the compositor's current output list.
    ///
    /// The order is part of the contract Lua configurations rely on:
    ///
    /// 1. index 1 is the output this runtime was created for — matched by name
    ///    in `screens`, so its geometry follows the compositor, and left as it
    ///    was if the compositor no longer lists it (the supervisor tears such a
    ///    runtime down anyway);
    /// 2. every other output follows in the order it was passed in, which is
    ///    the order the compositor advertised it.
    ///
    /// A runtime built without an output of its own keeps the list exactly as
    /// the compositor advertised it.
    ///
    /// The table is updated in place, so a configuration that captured
    /// `morf.screens` keeps seeing the live list. An empty list is a no-op: a
    /// runtime with no compositor behind it (`morf check`, the lock screen)
    /// keeps whatever `Runtime::for_screen` installed.
    pub fn set_screens(&mut self, screens: &[Screen]) {
        if screens.is_empty() {
            return;
        }
        self.update_screens(screens, true);
    }

    /// The live desktop for a runtime owning several outputs, such as a
    /// session lock. It has no single output to retain or put first: removal
    /// (including an empty desktop) must actually remove the old entries.
    pub fn replace_screens(&mut self, screens: &[Screen]) {
        self.update_screens(screens, false);
    }

    fn update_screens(&mut self, screens: &[Screen], own_first: bool) {
        let mut own_scale = None;
        self.lua.enter(|ctx| {
            let Ok(morf) = ctx.get_global::<Table>("morf") else {
                return;
            };
            let LuaValue::Table(table) = morf.get_value(ctx, "screens") else {
                return;
            };
            let own = match (own_first, table.get_value(ctx, 1)) {
                (true, LuaValue::Table(entry)) => Some(entry),
                _ => None,
            };
            let own_name = own.and_then(|entry| match entry.get_value(ctx, "name") {
                LuaValue::String(name) => name.to_str().ok().map(str::to_owned),
                _ => None,
            });
            // By position rather than by name, so outputs the compositor left
            // unnamed cannot collapse into one another.
            let own_index = own_name
                .as_deref()
                .and_then(|name| screens.iter().position(|screen| screen.name == name));
            let mut ordered = Vec::with_capacity(screens.len() + 1);
            match own_index {
                Some(index) => {
                    own_scale = Some(screens[index].scale);
                    ordered.push(screen_entry(ctx, &screens[index]));
                }
                None if own_name.is_some() => ordered.extend(own),
                None => {}
            }
            ordered.extend(
                screens
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| Some(*index) != own_index)
                    .map(|(_, screen)| screen_entry(ctx, screen)),
            );
            for (offset, entry) in ordered.iter().enumerate() {
                table
                    .set(ctx, offset as i64 + 1, *entry)
                    .expect("screen table accepts integer keys");
            }
            // Outputs come and go: whatever the previous list left past the new
            // end has to disappear rather than linger as a stale entry.
            let mut index = ordered.len() as i64 + 1;
            while !matches!(table.get_value(ctx, index), LuaValue::Nil) {
                table
                    .set(ctx, index, LuaValue::Nil)
                    .expect("screen table accepts integer keys");
                index += 1;
            }
        });
        if let Some(scale) = own_scale {
            self.set_preferred_scale(scale);
        }
        self.bump_screens_revision(screens);
    }

    /// Moves `morf.screens_revision()` when the output list changed, and
    /// runs what follows it.
    fn bump_screens_revision(&mut self, screens: &[Screen]) {
        {
            let mut state = self.reactive.borrow_mut();
            let state = &mut *state;
            if !state
                .engine
                .session
                .screens_changed(&mut state.engine.reactive, screens)
                || state.engine.session.screens_revision.is_none()
            {
                return;
            }
        }
        if let Err(message) = self
            .lua
            .enter(|ctx| crate::reactive_bindings::flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("screens: {message}"));
        }
    }
}
