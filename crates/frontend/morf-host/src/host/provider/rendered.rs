//! Read-only geometry from a successfully submitted primary GPU frame.
//! Named bounds describe that surface's scene, not compositor presentation,
//! input acceptance, authentication, or the absence of other surfaces.
use morf_app::Backend;
use morf_layout::{Geometry, Layout, Transform2D, node_transform};
use morf_scene::{Element, NodeHandle, Scene, Value};
use serde_json::{Value as Json, json};

const LIMIT: usize = 256;
const FRESH_NS: u64 = 250_000_000;
// Leave room for the fixed envelope and maximum-width decimal identities.
const MAX_FRAME_BYTES: usize = 128 * 1024 - 512;

#[derive(Clone, Debug, PartialEq)]
struct Region {
    node: u64,
    id: String,
    bounds: Geometry,
    interactive: bool,
    semantic: Option<Json>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Frame {
    size: (u32, u32),
    scale_120: u32,
    regions: Vec<Region>,
}

#[derive(Default)]
pub(super) struct Rendered {
    busy: bool,
    requested: bool,
    revision: u64,
    generation: u64,
    sampled: u64,
    stamp: Option<u64>,
    frame: Option<Frame>,
}
impl Rendered {
    pub fn busy(&mut self) {
        self.busy = true;
    }
    pub fn observe(
        &mut self,
        host: &crate::host::turn::Host,
        runtime: &morf_lua::Runtime,
        generation: u64,
    ) {
        self.busy = false;
        let cache = &host.state.layout;
        let scene = runtime.scene();
        let stamp = scene.paint_stamp(host.state.primary_root).ok();
        let current = cache.painted.is_some()
            && cache.painted == stamp
            && cache.revision == scene.layout_revision_of(host.state.primary_root)
            && host.backend.layer_logical_size(morf_app::PRIMARY_LAYER) == Some(cache.size)
            && host.backend.layer_scale_120(morf_app::PRIMARY_LAYER) == Some(cache.scale_120);
        let Some(now) = super::controls::now().and_then(|now| u64::try_from(now.as_nanos()).ok())
        else {
            self.frame = None;
            return;
        };
        if !current {
            self.frame = None;
            self.stamp = None;
            self.generation = generation;
            return;
        }
        // A shell with no observer pays only the stamp check. A read wakes the
        // UI thread and requests one observation; it never timestamps itself.
        if !std::mem::take(&mut self.requested) {
            if self.generation != generation || self.stamp != stamp {
                self.frame = None;
                self.stamp = None;
            }
            return;
        }
        // Idle observations refresh the same submitted frame without walking
        // the scene or painting again. Every scene mutation changes its stamp.
        if self.generation == generation && self.stamp == stamp && self.frame.is_some() {
            self.sampled = now;
            return;
        }
        let frame = capture(
            &scene,
            &cache.layout,
            host.state.primary_root,
            cache.size,
            cache.scale_120,
        );
        if self.generation != generation || self.frame != frame {
            let Some(next) = self.revision.checked_add(1) else {
                self.frame = None;
                return;
            };
            self.revision = next;
        }
        self.generation = generation;
        self.sampled = now;
        self.stamp = stamp;
        self.frame = frame;
    }
    pub fn report(&mut self, epoch: &str, generation: u64, now: u64) -> Json {
        self.requested = true;
        let fresh = !self.busy
            && self.generation == generation
            && self.sampled <= now
            && now - self.sampled < FRESH_NS;
        let frame = self.frame.as_ref().filter(|_| fresh).and_then(|frame| {
            let regions: Vec<_> = frame
                .regions
                .iter()
                .map(|region| {
                    let mut value = json!({
                        "node":region.node.to_string(),"id":region.id,"interactive":region.interactive,
                        "bounds":{"x":region.bounds.x,"y":region.bounds.y,
                            "width":region.bounds.width,"height":region.bounds.height},
                    });
                    if let Some(semantic) = &region.semantic {
                        value["semantic"] = semantic.clone();
                    }
                    value
                })
                .collect();
            let frame = json!({"width":frame.size.0,"height":frame.size.1,
                "scale_120":frame.scale_120,"regions":regions});
            // Escaping names/layouts can exceed the wire budget even when
            // every individual field and the region count are valid.
            (serde_json::to_vec(&frame).ok()?.len() <= MAX_FRAME_BYTES).then_some(frame)
        });
        json!({"version":1,"epoch":epoch,"ui_generation":generation.to_string(),
            "revision":self.revision.to_string(),"sampled_ns":self.sampled.to_string(),
            "coverage":"primary_surface_named_bounds","frame":frame})
    }
}

fn capture(
    scene: &Scene,
    layout: &Layout,
    root: NodeHandle,
    size: (u32, u32),
    scale_120: u32,
) -> Option<Frame> {
    if size.0 == 0 || size.1 == 0 || scale_120 == 0 {
        return None;
    }
    let mut regions = Vec::new();
    let clip = Geometry {
        x: 0.0,
        y: 0.0,
        width: size.0.into(),
        height: size.1.into(),
    };
    walk(
        scene,
        layout,
        root,
        Transform2D::IDENTITY,
        clip,
        1.0,
        true,
        &mut regions,
    )
    .ok()?;
    regions.sort_by_key(|region| region.node);
    Some(Frame {
        size,
        scale_120,
        regions,
    })
}

#[allow(clippy::too_many_arguments)]
fn walk(
    scene: &Scene,
    layout: &Layout,
    node: NodeHandle,
    inherited: Transform2D,
    clip: Geometry,
    opacity: f64,
    enabled: bool,
    regions: &mut Vec<Region>,
) -> Result<(), String> {
    let get = |name| scene.bool_value(node, name).map_err(|e| e.to_string());
    if !get("visible")? || scene.is_exiting(node) {
        return Ok(());
    }
    let opacity = opacity * scene.number(node, "opacity").map_err(|e| e.to_string())?;
    if !opacity.is_finite() {
        return Err("nonfinite opacity".into());
    }
    if opacity <= 0.0 {
        return Ok(());
    }
    let enabled = enabled && get("enabled")?;
    let Some(geometry) = layout.geometry(node) else {
        return Ok(());
    };
    let transform =
        inherited.then(node_transform(scene, node, geometry).map_err(|e| e.to_string())?);
    let bounds = transform.bounds(geometry);
    if [bounds.x, bounds.y, bounds.width, bounds.height]
        .into_iter()
        .any(|n| !n.is_finite() || n.abs() > 1_000_000.0)
    {
        return Err("invalid rendered bounds".into());
    }
    let bounds = intersect(bounds, clip);
    let clip = if get("clip")? { bounds } else { clip };
    if clip.width <= 0.0 || clip.height <= 0.0 {
        return Ok(());
    }
    let semantic = semantic(
        scene
            .current(node, "input_context")
            .map_err(|e| e.to_string())?,
    )?;
    let id = scene.current(node, "id").map_err(|e| e.to_string())?;
    if semantic.is_some() && !matches!(id,Value::String(id) if !id.is_empty()) {
        return Err("input_context requires a named scene node".into());
    }
    if let Value::String(id) = id
        && !id.is_empty()
        && bounds.width > 0.0
        && bounds.height > 0.0
    {
        if id.len() > 128 || id.chars().any(char::is_control) || regions.len() >= LIMIT {
            return Err("invalid or excessive named bounds".into());
        }
        let element = scene.element(node).map_err(|e| e.to_string())?;
        regions.push(Region {
            node: node.to_bits(),
            id: id.clone(),
            bounds,
            interactive: enabled
                && matches!(
                    element,
                    Element::MouseArea | Element::DropArea | Element::TextInput | Element::Terminal
                ),
            semantic,
        });
    }
    for &child in scene.children(node).map_err(|e| e.to_string())? {
        if !scene.is_mask(child) {
            walk(
                scene, layout, child, transform, clip, opacity, enabled, regions,
            )?;
        }
    }
    Ok(())
}
fn semantic(value: &Value) -> Result<Option<Json>, String> {
    if matches!(value, Value::Nil) {
        return Ok(None);
    }
    let Value::Map(fields) = value else {
        return Err("input_context must be a role table".into());
    };
    if fields
        .keys()
        .any(|key| !matches!(key.as_str(), "role" | "layout"))
    {
        return Err("input_context contains unknown fields".into());
    }
    let Some(Value::String(role)) = fields.get("role") else {
        return Err("input_context lacks a role".into());
    };
    if !matches!(
        role.as_str(),
        "keyboard"
            | "secure_overlay"
            | "clock_background"
            | "content_bottom"
            | "physical_bottom"
            | "top_left"
            | "top_right"
            | "side_left"
            | "side_right"
            | "local_control"
    ) {
        return Err("unknown input_context role".into());
    }
    let layout = match fields.get("layout") {
        Some(Value::String(layout))
            if role == "keyboard"
                && !layout.is_empty()
                && layout.len() <= 128
                && !layout.chars().any(char::is_control) =>
        {
            Some(layout)
        }
        None if role != "keyboard" => None,
        _ => {
            return Err(
                "keyboard context requires a bounded layout; other roles cannot set it".into(),
            );
        }
    };
    let mut value = json!({"role":role});
    if let Some(layout) = layout {
        value["layout"] = json!(layout);
    }
    Ok(Some(value))
}
fn intersect(a: Geometry, b: Geometry) -> Geometry {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Geometry {
        x,
        y,
        width: ((a.x + a.width).min(b.x + b.width) - x).max(0.0),
        height: ((a.y + a.height).min(b.y + b.height) - y).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_layout::{Size, TextMeasurer, TextOptions};
    struct NoText;
    impl TextMeasurer for NoText {
        fn measure(&mut self, _: NodeHandle, _: &str, _: &str, _: f64, _: TextOptions) -> Size {
            Size::default()
        }
    }
    #[test]
    fn escaped_semantics_obey_the_total_reply_budget_without_truncation() {
        let regions = (1..=LIMIT)
            .map(|node| Region {
                node: node as u64,
                id: "\"".repeat(128),
                interactive: false,
                bounds: Geometry {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                semantic: Some(json!({"role":"keyboard","layout":"\"".repeat(128)})),
            })
            .collect();
        let mut state = Rendered {
            generation: 2,
            revision: 1,
            sampled: 1_000,
            frame: Some(Frame {
                size: (100, 100),
                scale_120: 120,
                regions,
            }),
            ..Default::default()
        };
        let report = state.report(&"a".repeat(32), 2, 1_001);
        assert!(report["frame"].is_null());
        assert!(serde_json::to_vec(&report).unwrap().len() < 128 * 1024);
        state.frame.as_mut().unwrap().regions.truncate(1);
        let report = state.report(&"a".repeat(32), 2, 1_001);
        assert_eq!(report["frame"]["regions"].as_array().unwrap().len(), 1);
        assert_eq!(report["frame"]["regions"][0]["id"], "\"".repeat(128));
    }
    #[test]
    fn semantic_annotations_are_typed_and_cannot_supply_geometry_or_authentication() {
        let value = |json: Json| -> Value {
            let mut fields = std::collections::BTreeMap::new();
            for (key, value) in json.as_object().unwrap() {
                fields.insert(
                    key.clone(),
                    match value {
                        Json::String(value) => Value::String(value.clone()),
                        _ => Value::Bool(true),
                    },
                );
            }
            Value::Map(fields)
        };
        assert!(semantic(&Value::Nil).unwrap().is_none());
        assert_eq!(
            semantic(&value(json!({"role":"keyboard","layout":"dev"}))).unwrap(),
            Some(json!({"role":"keyboard","layout":"dev"}))
        );
        for invalid in [
            json!({"role":"keyboard"}),
            json!({"role":"keyboard","layout":""}),
            json!({"role":"top_left","layout":"dev"}),
            json!({"role":"unlocked"}),
            json!({"role":"keyboard","layout":"full","x":0}),
            json!({"role":"keyboard","layout":"full","authenticated":true}),
        ] {
            assert!(semantic(&value(invalid)).is_err());
        }
    }
    #[test]
    fn named_bounds_follow_transforms_clips_visibility_and_keep_repeated_names_distinct() {
        let mut scene = Scene::new();
        let root = scene.create(Element::Item);
        scene.assign(root, "width", 100.0).unwrap();
        scene.assign(root, "height", 100.0).unwrap();
        let parent = scene.create(Element::Item);
        scene.reparent(parent, Some(root)).unwrap();
        for (key, value) in [
            ("x", 10.0),
            ("y", 20.0),
            ("width", 50.0),
            ("height", 50.0),
            ("translate_x", 5.0),
        ] {
            scene.assign(parent, key, value).unwrap();
        }
        scene.assign(parent, "clip", true).unwrap();
        let child = scene.create(Element::MouseArea);
        scene.reparent(child, Some(parent)).unwrap();
        scene.assign(child, "id", "key").unwrap();
        scene.assign(child, "x", 40.0).unwrap();
        scene.assign(child, "y", 5.0).unwrap();
        scene.assign(child, "width", 30.0).unwrap();
        scene.assign(child, "height", 20.0).unwrap();
        scene
            .assign(
                child,
                "input_context",
                Value::Map(
                    [
                        ("role".into(), Value::String("keyboard".into())),
                        ("layout".into(), Value::String("dev".into())),
                    ]
                    .into(),
                ),
            )
            .unwrap();
        let compute = |scene: &Scene| {
            Layout::compute(
                scene,
                root,
                Size {
                    width: 100.0,
                    height: 100.0,
                },
                &mut NoText,
            )
            .unwrap()
        };
        let frame = capture(&scene, &compute(&scene), root, (100, 100), 120).unwrap();
        assert_eq!(frame.regions.len(), 1);
        assert_eq!(
            frame.regions[0].bounds,
            Geometry {
                x: 55.0,
                y: 25.0,
                width: 10.0,
                height: 20.0
            }
        );
        assert!(frame.regions[0].interactive);
        assert_eq!(
            frame.regions[0].semantic,
            Some(json!({"role":"keyboard","layout":"dev"}))
        );
        scene.assign(parent, "enabled", false).unwrap();
        assert!(
            !capture(&scene, &compute(&scene), root, (100, 100), 120)
                .unwrap()
                .regions[0]
                .interactive
        );
        scene.assign(parent, "opacity", 0.0).unwrap();
        assert!(
            capture(&scene, &compute(&scene), root, (100, 100), 120)
                .unwrap()
                .regions
                .is_empty()
        );
        scene.assign(parent, "opacity", 1.0).unwrap();
        scene.assign(parent, "id", "key").unwrap();
        let repeated = capture(&scene, &compute(&scene), root, (100, 100), 120).unwrap();
        assert_eq!(repeated.regions.len(), 2);
        assert_eq!(repeated.regions[0].id, repeated.regions[1].id);
        assert_ne!(repeated.regions[0].node, repeated.regions[1].node);
        scene.assign(parent, "visible", false).unwrap();
        assert!(
            capture(&scene, &compute(&scene), root, (100, 100), 120)
                .unwrap()
                .regions
                .is_empty()
        );
    }
    #[test]
    fn report_never_refreshes_itself_or_crosses_a_runtime_generation() {
        let mut state = Rendered {
            revision: 7,
            generation: 2,
            sampled: 1_000,
            frame: Some(Frame {
                size: (100, 200),
                scale_120: 120,
                regions: Vec::new(),
            }),
            ..Default::default()
        };
        assert!(state.report("epoch", 2, 1_001)["frame"].is_object());
        for (generation, now) in [(3, 1_001), (2, 999), (2, 1_000 + FRESH_NS)] {
            assert!(state.report("epoch", generation, now)["frame"].is_null());
        }
        state.busy();
        assert!(state.report("epoch", 2, 1_001)["frame"].is_null());
    }
}
