//! Which parts of a large field are worth running the shader over.
//!
//! A field's quad covers everything any of its layers can reach, and every
//! fragment of it walks every layer. For a frame round the screen — the
//! screen minus a rounded inner box — that is the whole screen walked to find
//! that almost all of it is the empty inside. So a large field is cut into
//! tiles, the composed distance is taken once per tile on the CPU, and a tile
//! the surface cannot reach is not drawn at all.
//!
//! The CPU composition here is the shader's, operator for operator, over the
//! same distance functions `morf_region` keeps for the input region. It is
//! only asked one thing — could anything paint in this tile — so it answers
//! conservatively: a composed field changes by at most `√2` per pixel moved
//! (a circular seam's worst case; every other operator is 1), and the band a
//! blend group switches operators in is added on top.

use morf_layout::Geometry;
use morf_region::{Operation, Shape, ShapeParams, combine_profiled, distance};

use super::{MAX_FIELD_LAYERS, ShadowReach, layer_frame};
use crate::commands::SdfLayer;

/// Tiles are this many physical pixels square.
pub const FIELD_TILE: f32 = 32.0;

/// Tiles are ruled out this many to a side at a time before one by one.
const BLOCK: usize = 4;

/// A field smaller than this many physical pixels is drawn whole: its tiles
/// would cost more instances than the pixels they save.
const TILED_AREA: f32 = 384.0 * 384.0;

/// Whether every layer has a twin on the CPU. A letter or a drawing is an
/// outline in a GPU buffer, and has none.
pub fn composable(layers: &[SdfLayer]) -> bool {
    layers.iter().all(|layer| {
        layer.shape != Shape::Polygon
            && layer.morph_to != Shape::Polygon
            && layer.glyph.is_none()
            && layer.svg_source.is_none()
    })
}

/// One layer with everything that does not depend on the point worked out.
struct Prepared<'a> {
    layer: &'a SdfLayer,
    centre: [f32; 2],
    /// The rotation's sine and cosine, as the shader turns the point.
    turn: Option<(f32, f32)>,
    frame: [f32; 4],
    scale: f32,
    half: [f32; 2],
    params: ShapeParams,
}

fn prepare(layers: &[SdfLayer]) -> Vec<Prepared<'_>> {
    layers
        .iter()
        .take(MAX_FIELD_LAYERS)
        .map(|layer| {
            let (frame, scale) = layer_frame(layer.matrix);
            Prepared {
                layer,
                centre: [
                    (layer.bounds.x + layer.bounds.width / 2.0) as f32,
                    (layer.bounds.y + layer.bounds.height / 2.0) as f32,
                ],
                turn: (layer.rotation != 0.0).then(|| (-layer.rotation).to_radians().sin_cos()),
                frame,
                scale,
                half: [
                    (layer.bounds.width / 2.0) as f32,
                    (layer.bounds.height / 2.0) as f32,
                ],
                params: ShapeParams {
                    radii: layer.radii,
                    points: layer.points,
                    inner_radius: layer.inner_radius,
                    thickness: layer.thickness,
                    angle: layer.angle,
                },
            }
        })
        .collect()
}

/// The composed distance at `point`, in the same logical space as the
/// layers' bounds: `field.wgsl`'s `compose`, on the CPU, with every layer
/// that has any opacity at all.
pub fn composed_distance(layers: &[SdfLayer], point: [f32; 2]) -> f32 {
    compose(&prepare(layers), point, Take::Present)
}

/// Which layers a CPU composition takes, when some are fading: a fading
/// layer is there in some of the combinations the shader mixes and not in
/// others.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Take {
    /// Every layer with any opacity.
    Present,
    /// The combination that covers the most: fading layers that add
    /// surface, and not those that take it away.
    Most,
    /// The one that covers the least: the other way about.
    Least,
}

/// Whether a composition taking `take` includes the layer at `index`.
fn takes(take: Take, index: usize, layer: &SdfLayer) -> bool {
    if layer.opacity <= 0.0 {
        return false;
    }
    if layer.opacity >= 1.0 || take == Take::Present {
        return true;
    }
    // The first layer is where the composition starts, which adds.
    let adds = index == 0
        || matches!(
            layer.operation,
            Operation::Union | Operation::SmoothUnion | Operation::Xor
        );
    adds == (take == Take::Most)
}

fn compose(layers: &[Prepared<'_>], point: [f32; 2], take: Take) -> f32 {
    let mut accumulated = 1e20_f32;
    let mut group = 0;
    for (index, prepared) in layers.iter().enumerate() {
        let layer = prepared.layer;
        // Left out as the shader leaves it out: joined to what is there as if
        // it were not, which every operator answers from `1e20` for nothing.
        if !takes(take, index, layer) {
            continue;
        }
        let offset = [point[0] - prepared.centre[0], point[1] - prepared.centre[1]];
        let turned = match prepared.turn {
            Some((sin, cos)) => [
                offset[0] * cos - offset[1] * sin,
                offset[0] * sin + offset[1] * cos,
            ],
            None => offset,
        };
        let frame = prepared.frame;
        let local = [
            frame[0] * turned[0] + frame[2] * turned[1],
            frame[1] * turned[0] + frame[3] * turned[1],
        ];
        let start = distance(layer.shape, &prepared.params, prepared.half, local) * prepared.scale;
        let value = if layer.morph > 0.0 && layer.morph_to != layer.shape {
            let end =
                distance(layer.morph_to, &prepared.params, prepared.half, local) * prepared.scale;
            start + (end - start) * layer.morph
        } else {
            start
        };
        if index == 0 {
            accumulated = value;
            group = layer.blend_group;
            continue;
        }
        let mut operation = layer.operation;
        if layer.blend_group != 0 && group != 0 && layer.blend_group != group {
            operation = operation.hard();
        }
        let before = accumulated;
        accumulated = combine_profiled(operation, layer.profile, before, value, layer.blend);
        let adds = matches!(
            operation,
            Operation::Union | Operation::SmoothUnion | Operation::Xor
        );
        if adds && value < before {
            group = layer.blend_group;
        }
    }
    accumulated
}

/// What a field reaches beyond its composed surface.
pub(crate) struct Spill {
    /// Outline, softness and antialiasing, in logical pixels.
    pub(crate) edge: f64,
    /// An outer shadow, if the field casts one.
    pub(crate) shadow: Option<ShadowReach>,
    /// Whether a tile deep inside the surface may be filled without the
    /// layers being walked: every layer the same colour, and no inner shadow
    /// darkening the inside.
    pub(crate) solid: bool,
}

/// One run of tiles to draw: its area in the node's own physical space,
/// and whether all of it lies deep inside a surface of one colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldTile {
    pub area: [f32; 4],
    pub solid: bool,
}

/// What a tile holds.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Held {
    Nothing,
    Edge,
    Inside,
}

/// The tiles of a field's quad the surface can reach, each as an area in
/// the node's own physical space, or nothing when the quad should be drawn
/// whole — it is small, a layer has no CPU twin, or no tile is empty.
///
/// Neighbouring tiles in a row are joined, so a solid band is one instance.
pub(crate) fn field_tiles(
    layers: &[SdfLayer],
    bounds: Geometry,
    area: [f32; 4],
    scale: f64,
    spill: Spill,
) -> Option<Vec<FieldTile>> {
    let (width, height) = (area[2] - area[0], area[3] - area[1]);
    if width * height < TILED_AREA || !composable(layers) {
        return None;
    }
    // A fading xor both adds and takes away, so no one combination covers
    // the most: the field is drawn whole while it fades.
    let fading = layers
        .iter()
        .any(|layer| layer.opacity > 0.0 && layer.opacity < 1.0);
    if layers.iter().any(|layer| {
        layer.opacity > 0.0 && layer.opacity < 1.0 && layer.operation == Operation::Xor
    }) {
        return None;
    }
    let scale = scale.max(1e-6) as f32;
    let seam = layers
        .iter()
        .map(|layer| layer.blend)
        .fold(0.0_f32, f32::max);
    // How far outside the surface a pixel can still be painted, and how far a
    // blend group's switch of operator can move the surface.
    let slack = spill.edge as f32 + seam + 1.0;
    let columns = (width / FIELD_TILE).ceil() as usize;
    let rows = (height / FIELD_TILE).ceil() as usize;
    let origin = [bounds.x as f32, bounds.y as f32];
    // Whether anything can paint within `half` (a half diagonal, logical)
    // of `centre`.
    let prepared = prepare(layers);
    // While a layer fades, a tile holds something if the combination that
    // covers the most reaches it, and is filled throughout only if the one
    // that covers the least does.
    let (most, least) = if fading {
        (Take::Most, Take::Least)
    } else {
        (Take::Present, Take::Present)
    };
    let holds = |centre: [f32; 2], half: f32| {
        let reach = std::f32::consts::SQRT_2 * half + slack;
        let here = compose(&prepared, centre, most);
        if here <= reach {
            // Inside by more than anything can move the edge: filled
            // throughout, and by one colour when the layers have one.
            let inside = spill.solid
                && here < -reach
                && (!fading || compose(&prepared, centre, least) < -reach);
            return if inside { Held::Inside } else { Held::Edge };
        }
        let shadowed = spill.shadow.as_ref().is_some_and(|shadow| {
            let moved = [
                centre[0] - shadow.offset_x as f32,
                centre[1] - shadow.offset_y as f32,
            ];
            compose(&prepared, moved, most) - shadow.spread as f32 <= reach + shadow.blur as f32
        });
        if shadowed { Held::Edge } else { Held::Nothing }
    };
    // A rectangle of the quad (physical, node-relative) as a centre and a
    // half diagonal in the layers' logical space.
    let span = |left: f32, top: f32, right: f32, bottom: f32| {
        (
            [
                origin[0] + (left + right) / 2.0 / scale,
                origin[1] + (top + bottom) / 2.0 / scale,
            ],
            (right - left).hypot(bottom - top) / 2.0 / scale,
        )
    };
    // Blocks of tiles first: the empty middle of a frame is ruled out a block
    // at a time, and only the blocks it touches are looked at tile by tile.
    let block_columns = columns.div_ceil(BLOCK);
    let block_rows = rows.div_ceil(BLOCK);
    let blocks: Vec<bool> = (0..block_rows * block_columns)
        .map(|index| {
            let (row, column) = (index / block_columns, index % block_columns);
            let edge = FIELD_TILE * BLOCK as f32;
            let left = area[0] + column as f32 * edge;
            let top = area[1] + row as f32 * edge;
            let (centre, half) = span(
                left,
                top,
                (left + edge).min(area[2]),
                (top + edge).min(area[3]),
            );
            holds(centre, half) != Held::Nothing
        })
        .collect();
    let mut tiles = Vec::new();
    let mut empty = 0;
    for row in 0..rows {
        let top = area[1] + row as f32 * FIELD_TILE;
        let bottom = (top + FIELD_TILE).min(area[3]);
        let mut run: Option<(f32, Held)> = None;
        for column in 0..=columns {
            let left = area[0] + column as f32 * FIELD_TILE;
            let held = if column < columns && blocks[(row / BLOCK) * block_columns + column / BLOCK]
            {
                let (centre, half) = span(left, top, (left + FIELD_TILE).min(area[2]), bottom);
                holds(centre, half)
            } else {
                Held::Nothing
            };
            if column < columns && held == Held::Nothing {
                empty += 1;
            }
            match run {
                Some((_, kind)) if kind == held => {}
                _ => {
                    if let Some((start, kind)) = run.take() {
                        tiles.push(FieldTile {
                            area: [start, top, left.min(area[2]), bottom],
                            solid: kind == Held::Inside,
                        });
                    }
                    if held != Held::Nothing {
                        run = Some((left, held));
                    }
                }
            }
        }
    }
    let solid = tiles.iter().any(|tile| tile.solid);
    (empty > 0 || solid).then_some(tiles)
}
