//! The layout tree: splits and stacks of panels, and the drop zones that
//! change it.

use std::collections::BTreeMap;
use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use super::list;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Zone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl Zone {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    pub(super) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "center" => Self::Center,
            "left" => Self::Left,
            "right" => Self::Right,
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Node {
    Split {
        id: String,
        vertical: bool,
        ratios: Vec<f64>,
        children: Vec<Node>,
    },
    Stack {
        id: String,
        panels: Vec<String>,
        current: usize,
    },
}

impl Node {
    pub(super) fn id(&self) -> &str {
        match self {
            Node::Split { id, .. } | Node::Stack { id, .. } => id,
        }
    }

    pub(super) fn stacks<'a>(&'a self, out: &mut Vec<&'a Node>) {
        match self {
            Node::Stack { .. } => out.push(self),
            Node::Split { children, .. } => children.iter().for_each(|c| c.stacks(out)),
        }
    }

    pub(super) fn find_mut(&mut self, wanted: &str) -> Option<&mut Node> {
        if self.id() == wanted {
            return Some(self);
        }
        match self {
            Node::Split { children, .. } => children.iter_mut().find_map(|c| c.find_mut(wanted)),
            Node::Stack { .. } => None,
        }
    }

    /// The stack holding `panel`.
    pub(super) fn stack_of(&self, panel: &str) -> Option<&str> {
        match self {
            Node::Stack { id, panels, .. } => {
                panels.iter().any(|p| p == panel).then_some(id.as_str())
            }
            Node::Split { children, .. } => children.iter().find_map(|c| c.stack_of(panel)),
        }
    }

    pub(super) fn remove_panel(&mut self, panel: &str) -> bool {
        match self {
            Node::Stack {
                panels, current, ..
            } => match panels.iter().position(|p| p == panel) {
                Some(i) => {
                    panels.remove(i);
                    if *current >= panels.len() || (i < *current) {
                        *current = current.saturating_sub(1);
                    }
                    true
                }
                None => false,
            },
            Node::Split { children, .. } => children.iter_mut().any(|c| c.remove_panel(panel)),
        }
    }

    /// Drops empty stacks and splits of one; `None` when nothing is left.
    pub(super) fn tidy(self) -> Option<Node> {
        match self {
            Node::Stack { ref panels, .. } if panels.is_empty() => None,
            Node::Stack { .. } => Some(self),
            Node::Split {
                id,
                vertical,
                ratios,
                children,
            } => {
                let mut kept = Vec::new();
                let mut kept_ratios = Vec::new();
                for (child, ratio) in children
                    .into_iter()
                    .zip(ratios.into_iter().chain(std::iter::repeat(0.0)))
                {
                    if let Some(child) = child.tidy() {
                        kept.push(child);
                        kept_ratios.push(ratio);
                    }
                }
                match kept.len() {
                    0 => None,
                    1 => kept.pop(),
                    n => {
                        let sum: f64 = kept_ratios.iter().sum();
                        let ratios = if sum > 0.0 {
                            kept_ratios.iter().map(|r| r / sum).collect()
                        } else {
                            vec![1.0 / n as f64; n]
                        };
                        Some(Node::Split {
                            id,
                            vertical,
                            ratios,
                            children: kept,
                        })
                    }
                }
            }
        }
    }

    pub(super) fn to_ipc(&self) -> IpcValue {
        let mut map = BTreeMap::new();
        match self {
            Node::Split {
                id,
                vertical,
                ratios,
                children,
            } => {
                map.insert("id".into(), id.as_str().into());
                map.insert("kind".into(), "split".into());
                map.insert(
                    "orientation".into(),
                    if *vertical { "vertical" } else { "horizontal" }.into(),
                );
                map.insert(
                    "ratios".into(),
                    list(ratios.iter().map(|r| (*r).into()).collect()),
                );
                map.insert(
                    "children".into(),
                    list(children.iter().map(Node::to_ipc).collect()),
                );
            }
            Node::Stack {
                id,
                panels,
                current,
            } => {
                map.insert("id".into(), id.as_str().into());
                map.insert("kind".into(), "stack".into());
                map.insert(
                    "panels".into(),
                    list(panels.iter().map(|p| p.as_str().into()).collect()),
                );
                map.insert(
                    "current".into(),
                    panels
                        .get(*current)
                        .map(String::as_str)
                        .unwrap_or("")
                        .into(),
                );
            }
        }
        IpcValue::Table(Arc::new(IpcTable::Map(map)))
    }
}

/// Puts `new` beside the stack `target` on `zone`'s side: into the split
/// around it when that runs the same way, else a split of the two.
pub(super) fn insert_beside(
    node: &mut Node,
    target: &str,
    new: Node,
    zone: Zone,
    fresh_id: &mut dyn FnMut() -> String,
) -> Result<(), Node> {
    let vertical = matches!(zone, Zone::Top | Zone::Bottom);
    let before = matches!(zone, Zone::Left | Zone::Top);
    if let Node::Split {
        vertical: v,
        ratios,
        children,
        ..
    } = node
    {
        if let Some(i) = children.iter().position(|c| c.id() == target) {
            if *v == vertical {
                let half = ratios[i] / 2.0;
                ratios[i] = half;
                let at = if before { i } else { i + 1 };
                children.insert(at, new);
                ratios.insert(at, half);
                return Ok(());
            }
            let old = std::mem::replace(
                &mut children[i],
                Node::Stack {
                    id: String::new(),
                    panels: Vec::new(),
                    current: 0,
                },
            );
            let pair = if before {
                vec![new, old]
            } else {
                vec![old, new]
            };
            children[i] = Node::Split {
                id: fresh_id(),
                vertical,
                ratios: vec![0.5, 0.5],
                children: pair,
            };
            return Ok(());
        }
        let mut new = new;
        for child in children.iter_mut() {
            match insert_beside(child, target, new, zone, fresh_id) {
                Ok(()) => return Ok(()),
                Err(back) => new = back,
            }
        }
        return Err(new);
    }
    if node.id() == target {
        let old = std::mem::replace(
            node,
            Node::Stack {
                id: String::new(),
                panels: Vec::new(),
                current: 0,
            },
        );
        let pair = if before {
            vec![new, old]
        } else {
            vec![old, new]
        };
        *node = Node::Split {
            id: fresh_id(),
            vertical,
            ratios: vec![0.5, 0.5],
            children: pair,
        };
        return Ok(());
    }
    Err(new)
}
