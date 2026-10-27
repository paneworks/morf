//! The load-time gate: whose top level is running, and whether a gated API
//! called now is refused.
//!
//! Every `require` and every plugin script pushes a scope for its top level,
//! held back (`Some`) when it is an unapproved imported module. While any
//! held scope is on the stack, every gated call is refused: whatever the
//! held module's top level causes synchronously -- functions it calls,
//! chunks it loads, coroutines it resumes, metamethods it triggers, modules
//! it requires first -- runs held.

use luna::{Callback, CallbackReturn, Context, Error, Execution, Stack, Value as LuaValue};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::LogLevel;
use crate::runtime::state::{ReactiveState, StateCell};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GateMode {
    /// Unapproved imported modules are held while they load.
    #[default]
    Enforce,
    /// Every imported module is held, approved or not (`morf check`).
    ImportedUnapproved,
    /// Every gated call is refused (`morf approve` reading a configuration).
    RefuseAll,
}

/// An unapproved imported module or plugin the gate holds back.
#[derive(Debug)]
pub(crate) struct Holder {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) own_dir: Option<PathBuf>,
}

impl Holder {
    fn refusal(&self, group: &str, api: &str) -> String {
        format!(
            "permission: `{}` is not approved to use {group} ({api}) while it loads \
             ({}); `morf approve` this configuration to allow it",
            self.name,
            self.path.display()
        )
    }
}

#[derive(Default)]
pub(crate) struct Gate {
    mode: GateMode,
    scopes: Vec<Option<Rc<Holder>>>,
}

impl Gate {
    pub(crate) fn mode(&self) -> GateMode {
        self.mode
    }

    pub(crate) fn set_mode(&mut self, mode: GateMode) {
        self.mode = mode;
    }

    /// A top level starts: held back when `holder` is set.
    pub(crate) fn enter(&mut self, holder: Option<Rc<Holder>>) {
        self.scopes.push(holder);
    }

    /// The top level `enter` started has finished.
    pub(crate) fn leave(&mut self) {
        self.scopes.pop();
    }

    /// The held module whose top level is innermost now, if any is running.
    fn holder(&self) -> Option<&Rc<Holder>> {
        self.scopes.iter().rev().find_map(Option::as_ref)
    }

    fn refusal(&self, group: &str, api: &str, target: Option<&Path>) -> Option<String> {
        if self.mode == GateMode::RefuseAll {
            return Some(format!(
                "permission: {api} ({group}) is refused while `morf approve` reads this \
                 configuration"
            ));
        }
        let holder = self.holder()?;
        if let (Some(target), Some(own)) = (target, &holder.own_dir)
            && super::canonical(target).starts_with(own)
        {
            return None;
        }
        Some(holder.refusal(group, api))
    }
}

/// Returns `nil, reason` from the enclosing native when the gate refuses
/// it: `gate!(ctx, &state, stack, "run", "morf.process.spawn")`.
macro_rules! gate {
    ($ctx:expr, $state:expr, $stack:expr, $group:expr, $api:expr) => {
        if $crate::vm::permissions::refused($ctx, $state, &mut $stack, $group, $api) {
            return Ok(luna::CallbackReturn::Return);
        }
    };
}
pub(crate) use gate;

/// An argument as a path, for a write's gate to judge before the arguments
/// are taken.
fn path_argument(stack: &Stack<'_, '_>, index: usize) -> PathBuf {
    match stack.get(index) {
        LuaValue::String(text) => PathBuf::from(text.display_lossy().to_string()),
        _ => PathBuf::new(),
    }
}

/// A native that asks the gate first: `group`'s API `api`.
pub(crate) fn gated<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    group: &'static str,
    api: &'static str,
    call: impl 'static
    + Fn(
        Context<'gc>,
        Execution<'gc, '_>,
        Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, Error<'gc>>,
) -> Callback<'gc> {
    let state = Rc::clone(state);
    Callback::from_fn(&ctx, move |ctx, exec, mut stack| {
        if refused(ctx, &state, &mut stack, group, api) {
            return Ok(CallbackReturn::Return);
        }
        call(ctx, exec, stack)
    })
}

/// A native that writes the paths in arguments `indexes`, asking the gate
/// first about each.
pub(crate) fn gated_write<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    api: &'static str,
    indexes: &'static [usize],
    call: impl 'static
    + Fn(
        Context<'gc>,
        Execution<'gc, '_>,
        Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, Error<'gc>>,
) -> Callback<'gc> {
    let state = Rc::clone(state);
    Callback::from_fn(&ctx, move |ctx, exec, mut stack| {
        for index in indexes {
            let target = path_argument(&stack, *index);
            if refused_write(ctx, &state, &mut stack, api, &target) {
                return Ok(CallbackReturn::Return);
            }
        }
        call(ctx, exec, stack)
    })
}

/// The gate's answer for a gated native about to run: when refused, the
/// reason is logged once and left on the stack as `nil, reason`.
pub(crate) fn refused<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    stack: &mut Stack<'gc, '_>,
    group: &str,
    api: &str,
) -> bool {
    refuse(ctx, state, stack, group, api, None)
}

/// As [`refused`], for a call that writes `target`: a held directory
/// module may still write inside its own folder.
pub(crate) fn refused_write<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    stack: &mut Stack<'gc, '_>,
    api: &str,
    target: &Path,
) -> bool {
    refuse(ctx, state, stack, "fs", api, Some(target))
}

fn refuse<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    stack: &mut Stack<'gc, '_>,
    group: &str,
    api: &str,
    target: Option<&Path>,
) -> bool {
    let reason = state.borrow().permissions().refusal(group, api, target);
    let Some(reason) = reason else {
        return false;
    };
    state.borrow_mut().log(LogLevel::Warn, reason.clone());
    stack.replace(ctx, (LuaValue::Nil, reason.as_str()));
    true
}
