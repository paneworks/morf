//! The layout lint: items that laid out to nothing and have children,
//! found after layout and logged from the poll, once per node.

use super::*;

impl Runtime {
    /// Complains about items that laid out to nothing and have children.
    ///
    /// The commonest way a configuration draws nothing and says nothing: a
    /// container whose size never got set, or got set to zero, with a whole
    /// subtree inside it that is never seen. The layout is the only place this
    /// is knowable -- a width of zero in the scene may be "auto", and only the
    /// resolved geometry says it resolved to nothing. Said once per node.
    pub fn lint_layout(&self, layout: &morf_layout::Layout, root: NodeHandle) {
        let scene = self.scene();
        let mut pending = vec![root];
        let mut queue = self.lint_queue.borrow_mut();
        while let Some(node) = pending.pop() {
            // What is hidden on purpose is not lost: an invisible item and
            // everything in it are passed over.
            if !scene.bool_value(node, "visible").unwrap_or(true) {
                continue;
            }
            let Ok(children) = scene.children(node) else {
                continue;
            };
            pending.extend(children.iter().copied());
            let Some(geometry) = layout.geometry(node) else {
                continue;
            };
            if geometry.width > 0.0 && geometry.height > 0.0 {
                continue;
            }
            // Only children that would show something count: a column of
            // rows that are all hidden, or that are empty themselves, lays
            // out to nothing because there is nothing in it to see.
            let seen = children
                .iter()
                .filter(|&&child| would_show(&scene, layout, child))
                .count();
            if seen > 0 && !growing_from_nothing(&scene, node) {
                let element = lint_path(&scene, node);
                queue.push((node, element, seen));
            }
        }
    }

    /// Logs what the lint found, once per node.
    ///
    /// Called from the poll, where nothing else holds the state, which is the
    /// reason the lint queues rather than logs.
    pub(crate) fn flush_lint(&mut self) {
        let found = std::mem::take(&mut *self.lint_queue.borrow_mut());
        if found.is_empty() {
            return;
        }
        let mut state = self.reactive.borrow_mut();
        for (node, element, count) in found {
            if !state.lint_warned.insert(node) {
                continue;
            }
            state.log(
                LogLevel::Warn,
                format!(
                    "lint: {element} laid out to nothing and has {count} child{} that will never be seen",
                    if count == 1 { "" } else { "ren" }
                ),
            );
        }
    }
}

/// Whether a node's size is on its way somewhere: its own size, or an
/// ancestor's, is animating.
///
/// A container that morphs open from nothing lays out to nothing on its first
/// frames, with its whole subtree inside, and that is the animation working
/// rather than a configuration forgetting a size. The lint is for a size that
/// stays at nothing, so it looks past one that is moving.
/// Whether a node would show something given room: it is visible, and it is
/// a leaf, or laid out to a size, or holds something that would show. A
/// container whose children are all hidden (or empty the same way) holds
/// nothing to see at any size.
fn would_show(scene: &morf_scene::Scene, layout: &morf_layout::Layout, node: NodeHandle) -> bool {
    if !scene.bool_value(node, "visible").unwrap_or(true) {
        return false;
    }
    if layout
        .geometry(node)
        .is_some_and(|geometry| geometry.width > 0.0 && geometry.height > 0.0)
    {
        return true;
    }
    match scene.children(node) {
        Ok(children) if !children.is_empty() => children
            .iter()
            .any(|&child| would_show(scene, layout, child)),
        _ => true,
    }
}

fn growing_from_nothing(scene: &morf_scene::Scene, node: NodeHandle) -> bool {
    const SIZES: &[&str] = &[
        "width",
        "height",
        "implicit_width",
        "implicit_height",
        "scale",
        "scale_x",
        "scale_y",
    ];
    let mut current = Some(node);
    while let Some(candidate) = current {
        if SIZES
            .iter()
            .any(|property| scene.is_animating(candidate, property).unwrap_or(false))
        {
            return true;
        }
        current = scene.parent(candidate).ok().flatten();
    }
    false
}

/// A node as the lint names it: its element under up to three ancestors,
/// outermost first (`Rect > Column > Item`), so the one meant can be found.
pub(crate) use morf_runtime::engine::node_path as lint_path;
