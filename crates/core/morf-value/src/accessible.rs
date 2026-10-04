//! What a screen reader is told about a node, said without a scene or a
//! platform: a scene builds these (`morf_scene::Scene::accessible_tree`),
//! a window's backend hands them to the platform's accessibility bus.

/// The checked state of a check box, switch or toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    False,
    True,
    Mixed,
}

/// A value read out: a number on a range, text in a field.
#[derive(Clone, Debug, PartialEq)]
pub enum AccessibleValue {
    Number(f64),
    Text(String),
}

/// One node of the accessible tree, `Id` being whatever names a node where
/// the tree was built (a scene's node handle).
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibleNode<Id> {
    pub node: Id,
    pub role: String,
    pub name: String,
    pub description: String,
    pub value: Option<AccessibleValue>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub step: Option<f64>,
    pub checked: Option<Checked>,
    pub expanded: Option<bool>,
    pub selected: Option<bool>,
    pub disabled: bool,
    pub pressed: bool,
    pub read_only: bool,
    pub modal: bool,
    pub focusable: bool,
    pub focused: bool,
    pub orientation: String,
    pub placeholder: String,
    pub level: Option<usize>,
    /// The box on the surface: x, y, width, height.
    pub bounds: Option<(f64, f64, f64, f64)>,
    pub children: Vec<Id>,
}
