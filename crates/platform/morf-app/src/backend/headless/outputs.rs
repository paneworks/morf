//! The outputs a headless run has.

use crate::Output;

/// `count` outputs of `size` at integer `scale`, side by side. None at all is
/// a desktop with every output gone.
pub fn virtual_outputs(count: usize, size: (u32, u32), scale: i32) -> Vec<Output> {
    (0..count)
        .map(|index| Output {
            id: index as u32 + 1,
            name: Some(format!("HEADLESS-{}", index + 1)),
            make: "morf".to_owned(),
            model: "headless".to_owned(),
            description: Some("morf headless output".to_owned()),
            position: Some((size.0 as i32 * index as i32, 0)),
            size: Some((size.0 as i32, size.1 as i32)),
            physical_size: None,
            scale: scale.max(1),
            transform: "normal",
            subpixel: "unknown",
        })
        .collect()
}
