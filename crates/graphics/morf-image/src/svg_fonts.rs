//! Shared font discovery for text-bearing SVGs.
use resvg::usvg;

pub(crate) fn database() -> std::sync::Arc<usvg::fontdb::Database> {
    static FONTS: std::sync::OnceLock<std::sync::Arc<usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut fonts = usvg::fontdb::Database::new();
            fonts.load_system_fonts();
            if fonts.faces().next().is_none() {
                // A development wrapper may point fontconfig at an empty store.
                fonts.load_fonts_dir("/usr/share/fonts");
                if let Some(home) = std::env::var_os("HOME") {
                    let home = std::path::PathBuf::from(home);
                    fonts.load_fonts_dir(home.join(".fonts"));
                    fonts.load_fonts_dir(home.join(".local/share/fonts"));
                }
            }
            let fallback = ["Roboto", "DejaVu Sans", "Liberation Sans", "Arial"]
                .into_iter()
                .find(|family| {
                    fonts
                        .query(&usvg::fontdb::Query {
                            families: &[usvg::fontdb::Family::Name(family)],
                            ..Default::default()
                        })
                        .is_some()
                })
                .map(str::to_owned)
                .or_else(|| {
                    fonts
                        .faces()
                        .next()
                        .and_then(|face| face.families.first().map(|f| f.0.clone()))
                });
            if let Some(fallback) = fallback {
                if fonts
                    .query(&usvg::fontdb::Query {
                        families: &[usvg::fontdb::Family::SansSerif],
                        ..Default::default()
                    })
                    .is_none()
                {
                    fonts.set_sans_serif_family(fallback.clone());
                }
                if fonts
                    .query(&usvg::fontdb::Query {
                        families: &[usvg::fontdb::Family::Serif],
                        ..Default::default()
                    })
                    .is_none()
                {
                    fonts.set_serif_family(fallback);
                }
            }
            std::sync::Arc::new(fonts)
        })
        .clone()
}
