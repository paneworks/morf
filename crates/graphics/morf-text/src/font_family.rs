//! Which installed face a requested family means: generic names, the
//! answers fontconfig gives for them, and the fallbacks when it gives none.

use cosmic_text::{Family, FontSystem};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResolvedFamily {
    Name(String),
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
}

impl ResolvedFamily {
    pub(crate) fn family(&self) -> Family<'_> {
        match self {
            Self::Name(name) => Family::Name(name),
            Self::Serif => Family::Serif,
            Self::SansSerif => Family::SansSerif,
            Self::Monospace => Family::Monospace,
            Self::Cursive => Family::Cursive,
            Self::Fantasy => Family::Fantasy,
        }
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Serif => "serif",
            Self::SansSerif => "sans-serif",
            Self::Monospace => "monospace",
            Self::Cursive => "cursive",
            Self::Fantasy => "fantasy",
        }
    }
}

pub(crate) fn resolve_family(fonts: &FontSystem, requested: &str) -> ResolvedFamily {
    for candidate in requested
        .split(',')
        .map(clean_family)
        .filter(|name| !name.is_empty())
    {
        let generic = match candidate.to_ascii_lowercase().as_str() {
            "serif" => Some(ResolvedFamily::Serif),
            "sans-serif" | "sans serif" | "sans" => Some(ResolvedFamily::SansSerif),
            "monospace" | "mono" => Some(ResolvedFamily::Monospace),
            "cursive" => Some(ResolvedFamily::Cursive),
            "fantasy" => Some(ResolvedFamily::Fantasy),
            _ => None,
        };
        if let Some(generic) = generic {
            return generic;
        }
        if let Some(installed) = installed_family(fonts, candidate) {
            return ResolvedFamily::Name(installed);
        }
    }
    if looks_monospace(requested) {
        ResolvedFamily::Monospace
    } else {
        ResolvedFamily::SansSerif
    }
}

fn clean_family(family: &str) -> &str {
    family
        .trim()
        .trim_matches(|character| character == '\'' || character == '"')
}

pub(crate) fn installed_family(fonts: &FontSystem, requested: &str) -> Option<String> {
    fonts.db().faces().find_map(|face| {
        face.families
            .iter()
            .find(|(family, _)| family.eq_ignore_ascii_case(requested))
            .map(|(family, _)| family.clone())
    })
}

fn looks_monospace(family: &str) -> bool {
    let family = family.to_ascii_lowercase();
    family.contains("mono")
        || family.contains("iosevka")
        || family.contains("terminal")
        || family.contains("typewriter")
        || family.contains("code")
}

/// The family fontconfig picks for a generic name (`sans-serif`), when it is
/// installed where this font system can see it.
///
/// fontconfig is where a desktop says which face "sans-serif" means (the
/// person's choice, their distribution's default); fontdb has no such idea
/// and falls back to a fixed list, so a shell drew in a different face from
/// every other application. `fc-match` is asked once, and not waited on for
/// long: a missing or hung fontconfig leaves the fixed list in charge.
fn fontconfig_family(fonts: &FontSystem, generic: &str) -> Option<String> {
    // Asked once per process: every font system (a test runner makes one per
    // test) would otherwise start fontconfig again, and one started with a
    // fresh cache directory rescans every installed font before it answers.
    let answers = fontconfig_answers();
    let index = match generic {
        "sans-serif" => 0,
        "serif" => 1,
        _ => 2,
    };
    installed_family(fonts, answers[index].as_deref()?.trim())
}

static FONTCONFIG_ANSWERS: std::sync::OnceLock<[Option<String>; 3]> = std::sync::OnceLock::new();

fn fontconfig_answers() -> &'static [Option<String>; 3] {
    FONTCONFIG_ANSWERS.get_or_init(|| ["sans-serif", "serif", "monospace"].map(ask_fontconfig))
}

/// Asks fontconfig for the generic families now, under the environment as it
/// is. A process about to point `XDG_CACHE_HOME` somewhere empty (a test
/// runner isolating a configuration) calls this first, so fontconfig answers
/// from the person's cache instead of rescanning every font into the new one.
pub fn warm_font_preferences() {
    let _ = fontconfig_answers();
    let _ = crate::subpixel::font_subpixel();
}

/// What `fc-match` names for one generic family, or nothing when it is
/// missing or slow.
fn ask_fontconfig(generic: &str) -> Option<String> {
    fc_match(&["-f", "%{family[0]}", generic])
}

/// `fc-match` with `args`, its answer trimmed, or nothing when it is
/// missing, slow or says nothing.
pub(crate) fn fc_match(args: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut child = Command::new("fc-match")
        .args(args)
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A few seconds at most, once: a large font collection behind an
    // unwarmed cache takes fc-match past a second, and giving up then drew
    // the shell in the wrong face and without subpixel text.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut name = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut()?, &mut name).ok()?;
    Some(name.trim().to_owned()).filter(|name| !name.is_empty())
}

pub(crate) fn configure_generic_families(fonts: &mut FontSystem) {
    let sans = fontconfig_family(fonts, "sans-serif").or_else(|| {
        preferred_family(
            fonts,
            &[
                "Noto Sans",
                "DejaVu Sans",
                "Liberation Sans",
                "Cantarell",
                "Nimbus Sans",
            ],
            |monospaced| !monospaced,
        )
    });
    let serif = fontconfig_family(fonts, "serif").or_else(|| {
        preferred_family(
            fonts,
            &[
                "Noto Serif",
                "DejaVu Serif",
                "Liberation Serif",
                "Nimbus Roman",
            ],
            |monospaced| !monospaced,
        )
    });
    let monospace = fontconfig_family(fonts, "monospace").or_else(|| {
        preferred_family(
            fonts,
            &[
                "Noto Sans Mono",
                "DejaVu Sans Mono",
                "Liberation Mono",
                "Nimbus Mono PS",
            ],
            |monospaced| monospaced,
        )
    });
    let db = fonts.db_mut();
    if let Some(family) = sans {
        db.set_sans_serif_family(family);
    }
    if let Some(family) = serif {
        db.set_serif_family(family);
    }
    if let Some(family) = monospace {
        db.set_monospace_family(family);
    }
}

fn preferred_family(
    fonts: &FontSystem,
    preferred: &[&str],
    fallback: impl Fn(bool) -> bool,
) -> Option<String> {
    preferred
        .iter()
        .find_map(|family| installed_family(fonts, family))
        .or_else(|| {
            fonts.db().faces().find_map(|face| {
                fallback(face.monospaced)
                    .then(|| face.families.first().map(|(family, _)| family.clone()))
                    .flatten()
            })
        })
}
