//! `morf.prefers`: what the person asked their desktop for.
//!
//! A state with five fields — `color_scheme`, `contrast`, `reduced_motion`,
//! `accent_color` and `scale` — read from the settings portal over D-Bus and
//! kept current from its change signal, so a binding that reads one follows
//! the desktop's setting. Without a portal the fields hold their defaults
//! and a configuration reads them the same way. Reading the portal is
//! `morf_system::prefers`; this is the state it writes.

use luna::{Context, Table};
use morf_system::prefers::Portal;
use std::cell::RefCell;
use std::rc::Rc;

use crate::{api_state::build, state::*, surface_types::*, types::*};

pub(crate) use morf_system::prefers::PREFERENCES;

pub(crate) fn install_prefers_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    screen: Option<&Screen>,
) {
    let portal = Portal::watch();
    let seed = Table::new(&ctx);
    seed.set_field(ctx, "color_scheme", "none");
    seed.set_field(ctx, "contrast", "none");
    seed.set_field(ctx, "reduced_motion", false);
    seed.set_field(ctx, "scale", screen.map_or(1, |screen| screen.scale) as i64);
    let metatable = state
        .borrow()
        .state_metatable
        .clone()
        .expect("states are installed before preferences");
    let userdata = build(ctx, &state, &metatable, "prefers", None, seed)
        .expect("the preference seed is plain scalars");
    let token = userdata
        .downcast_static::<StateToken>()
        .expect("build makes a state");
    let mut fields = token.fields.borrow_mut();
    // A table cannot seed a nil field, and no accent is nil, so that one
    // signal is made by hand.
    if !fields.scalars.contains_key("accent_color") {
        let mut state = state.borrow_mut();
        let id = state
            .reactive
            .graph
            .as_mut()
            .expect("the graph is not running at install")
            .signal("prefers.accent_color", IpcValue::Nil);
        state.reactive.values.insert(id, IpcValue::Nil);
        state.reactive.signals.push(id);
        fields.scalars.insert("accent_color".to_owned(), id);
    }
    let id = |name: &str| fields.scalars[name];
    let prefers = Prefers {
        color_scheme: id("color_scheme"),
        contrast: id("contrast"),
        reduced_motion: id("reduced_motion"),
        accent_color: id("accent_color"),
        scale: id("scale"),
        portal,
        overridden: std::collections::HashSet::new(),
    };
    let reduced = matches!(
        state.borrow().reactive.values.get(&prefers.reduced_motion),
        Some(IpcValue::Boolean(true))
    );
    let mut state = state.borrow_mut();
    state.prefers = Some(prefers);
    state
        .scene
        .set_motion_scale(if reduced { 0.0 } else { 1.0 });
    drop(fields);
    morf.set_field(ctx, "prefers", userdata);
}
