//! Named outlines retain their existing default path strings, so migrating
//! their construction cannot alter the current morph correspondence.
use super::geometry::{self, Cubic, Options, SEGMENTS};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
fn presets() -> &'static HashMap<&'static str, Arc<str>> {
    static PRESETS: OnceLock<HashMap<&'static str, Arc<str>>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        include_str!("geometry_presets.txt")
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(name, path)| (name, Arc::from(path)))
            .collect()
    })
}
pub fn names() -> Vec<&'static str> {
    let mut names: Vec<_> = presets().keys().copied().collect();
    names.sort_unstable();
    names
}
fn rect(w: f64, h: f64, r: [f64; 4]) -> Result<Vec<Cubic>, String> {
    geometry::polygon(&[[-w, -h, r[0]], [w, -h, r[1]], [w, h, r[2]], [-w, h, r[3]]])
}
fn polygon(vertices: &[[f64; 2]], r: f64) -> Result<Vec<Cubic>, String> {
    geometry::polygon(&vertices.iter().map(|v| [v[0], v[1], r]).collect::<Vec<_>>())
}
pub fn outline(name: &str) -> Result<Vec<Cubic>, String> {
    let regular = |n, r| {
        geometry::regular(
            n,
            Options {
                rounding: r,
                ..Options::default()
            },
        )
    };
    let star = |n, i, r, ir| {
        geometry::star(
            n,
            i,
            Options {
                rounding: r,
                inner_rounding: Some(ir),
                ..Options::default()
            },
        )
    };
    match name {
        "circle" => regular(8, 10.0),
        "square" => rect(1.0, 1.0, [0.3; 4]),
        "slanted" => polygon(&[[-0.8, -1.0], [1.0, -1.0], [0.8, 1.0], [-1.0, 1.0]], 0.3),
        "arch" => rect(1.0, 1.0, [1.0, 1.0, 0.2, 0.2]),
        "semicircle" => rect(1.0, 0.5, [1.0, 1.0, 0.1, 0.1]),
        "oval" => {
            let a = (-std::f64::consts::PI / 4.0).sin_cos();
            polygon(
                &[[0.0, -1.0], [0.7, 0.0], [0.0, 1.0], [-0.7, 0.0]]
                    .map(|[x, y]| [x * a.1 - y * a.0, x * a.0 + y * a.1]),
                10.0,
            )
        }
        "pill" => rect(1.0, 0.55, [10.0; 4]),
        "triangle" => regular(3, 0.25),
        "arrow" => geometry::polygon(&[
            [0.0, -1.0, 0.2],
            [0.95, 0.8, 0.2],
            [0.0, 0.35, 0.3],
            [-0.95, 0.8, 0.2],
        ]),
        "fan" => rect(1.0, 1.0, [1.0, 0.2, 0.2, 0.2]),
        "diamond" => polygon(&[[0.0, -1.0], [0.8, 0.0], [0.0, 1.0], [-0.8, 0.0]], 0.2),
        "clam_shell" => polygon(
            &[
                [-0.55, -1.0],
                [0.55, -1.0],
                [1.0, 0.0],
                [0.55, 1.0],
                [-0.55, 1.0],
                [-1.0, 0.0],
            ],
            0.2,
        ),
        "pentagon" => regular(5, 0.2),
        "gem" => polygon(
            &[
                [-0.5, -0.95],
                [0.5, -0.95],
                [1.0, -0.2],
                [0.0, 1.0],
                [-1.0, -0.2],
            ],
            0.25,
        ),
        "sunny" => star(8, 0.8, 0.15, 0.15),
        "very_sunny" => star(8, 0.65, 0.15, 0.15),
        "cookie4" => star(4, 0.75, 0.6, 0.3),
        "cookie6" => star(6, 0.8, 0.45, 0.3),
        "cookie7" => star(7, 0.82, 0.4, 0.3),
        "cookie9" => star(9, 0.85, 0.3, 0.2),
        "cookie12" => star(12, 0.88, 0.2, 0.15),
        "clover4" => geometry::lobes(
            4,
            0.3,
            Options {
                rotation: 45.0,
                spread: Some(28.0),
                ..Options::default()
            },
        ),
        "clover8" => geometry::lobes(
            8,
            0.62,
            Options {
                spread: Some(12.0),
                ..Options::default()
            },
        ),
        "burst" => star(12, 0.72, 0.03, 0.03),
        "soft_burst" => star(10, 0.72, 0.15, 0.08),
        "boom" => star(15, 0.5, 0.03, 0.03),
        "soft_boom" => star(15, 0.55, 0.1, 0.04),
        "flower" => star(8, 0.6, 0.35, 0.1),
        "puffy" => star(10, 0.82, 0.5, 0.02),
        "heart" => geometry::polygon(&[
            [0.0, -0.5, 0.0],
            [0.5, -1.0, 0.45],
            [1.0, -0.35, 0.45],
            [0.0, 0.95, 0.12],
            [-1.0, -0.35, 0.45],
            [-0.5, -1.0, 0.45],
        ]),
        _ => Err(format!("no shape named {name}")),
    }
}
type Key = (String, u64, Option<usize>);
#[derive(Default)]
struct Paths {
    items: HashMap<Key, Arc<str>>,
    bytes: usize,
}
fn cache() -> &'static Mutex<Paths> {
    static PATHS: OnceLock<Mutex<Paths>> = OnceLock::new();
    PATHS.get_or_init(Default::default)
}
pub fn path(name: &str, size: f64, segments: Option<usize>) -> Result<Arc<str>, String> {
    if size == 100.0 && segments == Some(SEGMENTS) {
        return presets()
            .get(name)
            .cloned()
            .ok_or_else(|| format!("no shape named {name}"));
    }
    let key = (name.to_owned(), size.to_bits(), segments);
    if let Some(path) = cache().lock().unwrap().items.get(&key) {
        return Ok(path.clone());
    }
    let curves = geometry::curves(&outline(name)?, segments)?;
    let path: Arc<str> = geometry::path(&curves, size)?.into();
    let mut cache = cache().lock().unwrap();
    if path.len() <= 512 * 1024 {
        if cache.items.len() >= 128 || cache.bytes + path.len() > 512 * 1024 {
            cache.items.clear();
            cache.bytes = 0;
        }
        if !cache.items.contains_key(&key) {
            cache.bytes += path.len();
            cache.items.insert(key, path.clone());
        }
    }
    Ok(path)
}
