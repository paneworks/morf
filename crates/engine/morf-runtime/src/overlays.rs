//! Overlays: what opens over a surface -- a menu, a dialog, a tooltip, a
//! drawer -- and the stack of them on each one. The newest is on top;
//! Escape and a press outside close the top one first; each is placed
//! beside its anchor after layout; closing gives focus back to the node
//! that had it and calls the overlay's `on_close`.
//!
//! The nodes an overlay lives in -- the layer, its wrapper, scrim and
//! blocker -- are the scripting layer's to make. This keeps the stack and
//! decides what happens to it.

use std::collections::HashMap;

use morf_scene::overlay::{Bounds, Placement, place};
use morf_scene::{NodeHandle, Scene};
use morf_value::IpcValue;

use crate::focus::{FocusReason, in_subtree};
use crate::handler::{Handler, Handlers};

/// The scrim an overlay that dims draws when no colour was given.
pub const DIM: &str = "#00000052";

/// One open overlay.
pub struct Overlay {
    pub root: NodeHandle,
    pub wrapper: NodeHandle,
    pub content: NodeHandle,
    pub anchor: Option<NodeHandle>,
    pub placement: Placement,
    pub gap: f64,
    pub margin: f64,
    pub modal: bool,
    pub escape: bool,
    pub outside: bool,
    pub on_close: Option<Handler>,
    /// The node that had focus when it opened, and whether it showed it.
    pub restore: Option<(NodeHandle, bool)>,
    /// Whether closing gives focus back (`restore`, true).
    pub give_back: bool,
    pub placed: Option<(f64, f64)>,
    /// Left where it is (`morf.overlay.track`): only the behaviour is the
    /// layer's, and `wrapper` is the catcher behind it.
    pub tracked: bool,
    /// Nodes besides the anchor a press on which is not outside it: the
    /// other controls that open it.
    pub except: Vec<NodeHandle>,
}

/// Every surface's overlay layer and the overlays open on it. `P` is what
/// the scripting layer keeps of an open asked for while the same content's
/// close was pending -- its options, to open with once that close lands.
pub struct Overlays<P> {
    pub layers: HashMap<NodeHandle, NodeHandle>,
    /// The wrapper each content was put in, kept while it is closed.
    pub wrappers: HashMap<NodeHandle, NodeHandle>,
    pub stack: Vec<Overlay>,
    pub closing: Vec<(NodeHandle, &'static str)>,
    /// Opens asked for while the same content's close was still pending:
    /// run once that close has landed, in the same turn, so a close and a
    /// reopen (a popup re-anchored) both happen, in order.
    pub reopening: Vec<(NodeHandle, P)>,
}

impl<P> Default for Overlays<P> {
    fn default() -> Self {
        Self {
            layers: HashMap::new(),
            wrappers: HashMap::new(),
            stack: Vec::new(),
            closing: Vec::new(),
            reopening: Vec::new(),
        }
    }
}

impl<P> Overlays<P> {
    /// Whether `content` is on the stack, closing or not.
    pub fn contains(&self, content: NodeHandle) -> bool {
        self.stack.iter().any(|o| o.content == content)
    }

    /// Whether a close of `content` is waiting for the next turn.
    pub fn close_pending(&self, content: NodeHandle) -> bool {
        self.closing.iter().any(|(c, _)| *c == content)
    }

    /// Whether `content` is open and staying so.
    pub fn is_open(&self, content: NodeHandle) -> bool {
        self.contains(content) && !self.close_pending(content)
    }

    /// Asks for `content` to close at the next turn, for `reason`.
    pub fn request_close(&mut self, content: NodeHandle, reason: &'static str) {
        self.closing.push((content, reason));
    }

    /// An open of `content`, which is already on the stack: queued to run
    /// after its pending close, if one is pending, else nothing to do.
    pub fn open_again(&mut self, content: NodeHandle, options: impl FnOnce() -> P) {
        if self.close_pending(content) {
            self.reopening.push((content, options()));
        }
    }

    /// The roots of the surfaces something is open over.
    pub fn roots(&self) -> Vec<NodeHandle> {
        let mut roots: Vec<NodeHandle> = self.stack.iter().map(|o| o.root).collect();
        roots.dedup();
        roots
    }

    /// The nodes whose boxes decide whether a press on `root` is outside its
    /// overlays: their contents, anchors and exceptions.
    pub fn nodes(&self, root: NodeHandle) -> Vec<NodeHandle> {
        self.stack
            .iter()
            .filter(|o| o.root == root)
            .flat_map(|o| {
                std::iter::once(o.content)
                    .chain(o.anchor)
                    .chain(o.except.iter().copied())
            })
            .collect()
    }

    /// The top overlay open on the surface whose tree is `root`.
    pub fn top(&self, root: NodeHandle) -> Option<usize> {
        self.stack.iter().rposition(|o| o.root == root)
    }

    /// What Tab walks on a surface: inside a modal overlay while one is
    /// open, the whole tree otherwise.
    pub fn focus_root(&self, root: NodeHandle) -> NodeHandle {
        self.stack
            .iter()
            .rev()
            .find(|o| o.root == root && o.modal)
            .map_or(root, |o| o.content)
    }

    /// Escape on a surface: takes its top overlay off the stack if Escape
    /// closes it.
    pub fn escape(&mut self, root: NodeHandle) -> Option<Overlay> {
        let index = self.top(root).filter(|i| self.stack[*i].escape)?;
        Some(self.stack.remove(index))
    }

    /// A press on a surface, on `hit`, at a point within the boxes of
    /// `inside`: takes its top overlay off the stack when the press is
    /// outside it (and not on its anchor or an exception) and it closes so.
    pub fn press(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        hit: Option<NodeHandle>,
        inside: &[NodeHandle],
    ) -> Option<Overlay> {
        let index = self.top(root)?;
        let overlay = &self.stack[index];
        let within = |outer: NodeHandle| {
            inside.contains(&outer) || hit.is_some_and(|hit| in_subtree(scene, outer, hit))
        };
        let outside = overlay.outside
            && !within(overlay.content)
            && !overlay.anchor.is_some_and(within)
            && !overlay.except.iter().copied().any(within);
        outside.then(|| self.stack.remove(index))
    }

    /// What is to close this turn: what was asked, then every overlay whose
    /// content `exists` no longer, as `"gone"`.
    pub fn take_closing(
        &mut self,
        exists: impl Fn(NodeHandle) -> bool,
    ) -> Vec<(NodeHandle, &'static str)> {
        let mut closing = std::mem::take(&mut self.closing);
        closing.extend(
            self.stack
                .iter()
                .filter(|o| !exists(o.content))
                .map(|o| (o.content, "gone")),
        );
        closing
    }

    /// Takes `content`'s overlay off the stack.
    pub fn remove(&mut self, content: NodeHandle) -> Option<Overlay> {
        let index = self.stack.iter().position(|o| o.content == content)?;
        Some(self.stack.remove(index))
    }
}

impl Overlay {
    /// Places it, `size` big, on a surface `surface` big, beside `anchor`
    /// (its box in surface coordinates): where it now goes, when that moved.
    pub fn place(
        &mut self,
        surface: (f64, f64),
        size: (f64, f64),
        anchor: Option<Bounds>,
    ) -> Option<(f64, f64)> {
        let bounds = Bounds {
            x: 0.0,
            y: 0.0,
            width: surface.0,
            height: surface.1,
        };
        let ((x, y), _) = place(anchor, size, bounds, self.placement, self.gap, self.margin);
        let at = (x.round(), y.round());
        (self.placed != Some(at)).then(|| {
            self.placed = Some(at);
            at
        })
    }

    /// Where focus goes once it has closed, if it goes back at all: when
    /// focus is in it or gone (`owner` is the surface's focused node), to
    /// the node that had it -- if `can_hold` it still -- with a ring if it
    /// showed one.
    pub fn give_focus_back(
        &self,
        scene: &Scene,
        owner: Option<NodeHandle>,
        can_hold: impl Fn(NodeHandle) -> bool,
    ) -> Option<(Option<NodeHandle>, FocusReason)> {
        let refocus = owner
            .is_none_or(|owner| !scene.contains(owner) || in_subtree(scene, self.content, owner));
        if !refocus || !self.give_back {
            return None;
        }
        let (node, visual) = self.restore.unzip();
        let reason = if visual == Some(true) {
            FocusReason::Keyboard
        } else {
            FocusReason::Program
        };
        Some((node.filter(|node| can_hold(*node)), reason))
    }

    /// Tells its `on_close` why it closed.
    pub fn notify_closed(&self, handlers: &mut dyn Handlers, reason: &str) -> Result<(), String> {
        match &self.on_close {
            Some(on_close) => handlers
                .call(on_close, &[IpcValue::String(reason.to_owned())])
                .map(drop),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
