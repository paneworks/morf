//! The outputs a configuration is told about, and what is read off one:
//! its density and its orientation.

/// Output metadata exposed to one per-screen Lua configuration instance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Screen {
    pub id: u32,
    pub name: String,
    pub make: String,
    pub model: String,
    pub description: Option<String>,
    pub position: Option<(i32, i32)>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub physical_size: Option<(i32, i32)>,
    pub scale: i32,
    pub transform: String,
}

pub fn density(screen: &Screen) -> Option<f64> {
    let (width, height) = (screen.width?, screen.height?);
    let (physical_width, physical_height) = screen.physical_size?;
    if width <= 0 || height <= 0 || physical_width <= 0 || physical_height <= 0 {
        return None;
    }
    let scale = f64::from(screen.scale.max(1));
    let horizontal = f64::from(width) * scale * 25.4 / f64::from(physical_width);
    let vertical = f64::from(height) * scale * 25.4 / f64::from(physical_height);
    Some((horizontal + vertical) / 2.0)
}

pub fn primary_orientation(screen: &Screen) -> &'static str {
    let dimensions = screen
        .physical_size
        .or_else(|| screen.width.zip(screen.height));
    match dimensions {
        Some((width, height)) if width < height => "portrait",
        _ => "landscape",
    }
}

pub fn orientation(screen: &Screen) -> &'static str {
    let primary = primary_orientation(screen);
    match screen.transform.as_str() {
        "180" | "flipped_180" if primary == "portrait" => "inverted_portrait",
        "180" | "flipped_180" => "inverted_landscape",
        "90" | "flipped_90" if primary == "portrait" => "landscape",
        "90" | "flipped_90" => "portrait",
        "270" | "flipped_270" if primary == "portrait" => "inverted_landscape",
        "270" | "flipped_270" => "inverted_portrait",
        _ => primary,
    }
}
