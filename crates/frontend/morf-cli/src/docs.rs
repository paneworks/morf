//! `morf docs`: this executable's API reference, rendered from the
//! definitions `morf types` emits, and the guides bundled with it. A guide
//! is printed only from a bundle stamped with this executable's version.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// What `morf docs` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Docs {
    /// The API reference (`morf docs`, `morf docs --api`).
    Api,
    /// The UI guide, `docs/UI.md` (`--ui`).
    Ui,
    /// The I/O guide, `docs/IO.md` (`--io`).
    Io,
}

impl Docs {
    fn file(self) -> &'static str {
        match self {
            Docs::Api => "",
            Docs::Ui => "UI.md",
            Docs::Io => "IO.md",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Docs::Api => "API reference",
            Docs::Ui => "UI guide",
            Docs::Io => "I/O guide",
        }
    }
}

/// The line `morf --version` prints, which every docs header repeats.
pub(crate) fn version_line() -> String {
    format!("morf {}", env!("CARGO_PKG_VERSION"))
}

/// A folder that may hold the guides, and where its version is written.
#[derive(Debug, Clone)]
pub(crate) struct Bundle {
    pub(crate) dir: PathBuf,
    pub(crate) stamp: Stamp,
}

#[derive(Debug, Clone)]
pub(crate) enum Stamp {
    /// A packaged bundle: the version in `VERSION` beside the guides.
    File(PathBuf),
    /// A source checkout: the workspace version in its `Cargo.toml`.
    Checkout(PathBuf),
}

impl Bundle {
    pub(crate) fn packaged(dir: PathBuf) -> Self {
        Self {
            stamp: Stamp::File(dir.join("VERSION")),
            dir,
        }
    }
}

/// Where the guides are looked for, in order: the package this executable
/// is in (`<prefix>/bin/morf` beside `<prefix>/share/morf/library/docs`,
/// or an unpacked release's `library/docs`), the XDG data folders, and last
/// the source checkout a development build sits in.
pub(crate) fn bundles() -> Vec<Bundle> {
    let bin = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let mut dirs = Vec::new();
    if let Some(bin) = &bin {
        if let Some(prefix) = bin.parent() {
            dirs.push(prefix.join("share/morf/library/docs"));
        }
        dirs.push(bin.join("library/docs"));
    }
    if let Some(data) = env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    {
        dirs.push(data.join("morf/library/docs"));
    }
    let system = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(env::split_paths(&system).map(|dir| dir.join("morf/library/docs")));
    let mut bundles: Vec<Bundle> = dirs.into_iter().map(Bundle::packaged).collect();
    // `target/<profile>/morf` and `target/<triple>/<profile>/morf`.
    if let Some(root) = bin
        .iter()
        .flat_map(|bin| bin.ancestors().skip(1).take(3))
        .find(|root| root.join("Cargo.toml").is_file() && root.join("docs").is_dir())
    {
        bundles.push(Bundle {
            dir: root.join("docs"),
            stamp: Stamp::Checkout(root.join("Cargo.toml")),
        });
    }
    bundles
}

/// `version = "..."` under `[workspace.package]` in a `Cargo.toml`.
pub(crate) fn workspace_version(manifest: &str) -> Option<String> {
    let mut section = "";
    for line in manifest.lines().map(str::trim) {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = name.trim();
        } else if section == "workspace.package"
            && let Some(value) = line.strip_prefix("version")
            && let Some(value) = value.trim_start().strip_prefix('=')
            && let Some(value) = value.trim().strip_prefix('"')
        {
            return value.split('"').next().map(str::to_owned);
        }
    }
    None
}

fn stamped_version(stamp: &Stamp) -> Result<String, String> {
    match stamp {
        Stamp::File(path) => fs::read_to_string(path)
            .map(|text| text.trim().to_owned())
            .map_err(|_| format!("no version stamp at {}", path.display())),
        Stamp::Checkout(manifest) => fs::read_to_string(manifest)
            .ok()
            .and_then(|text| workspace_version(&text))
            .ok_or_else(|| format!("no workspace version in {}", manifest.display())),
    }
}

/// The guide from the first bundle stamped for `version`, with its path;
/// otherwise every place looked and why each was passed over.
pub(crate) fn guide(
    docs: Docs,
    bundles: &[Bundle],
    version: &str,
) -> Result<(PathBuf, String), String> {
    let mut passed = Vec::new();
    for bundle in bundles {
        let path = bundle.dir.join(docs.file());
        let Ok(text) = fs::read_to_string(&path) else {
            passed.push(format!("{}: not there", path.display()));
            continue;
        };
        match stamped_version(&bundle.stamp) {
            Ok(found) if found == version => return Ok((path, text)),
            Ok(found) => passed.push(format!(
                "{}: stamped for morf {found}, but this is morf {version}",
                path.display()
            )),
            Err(why) => passed.push(format!("{}: {why}", path.display())),
        }
    }
    Err(format!(
        "no {} bundled for morf {version}; looked at:\n  {}",
        docs.file(),
        passed.join("\n  ")
    ))
}

/// The API reference for this build, from `generate_types`.
pub(crate) fn api_reference() -> String {
    let mut out = format!("# {} {}\n", version_line(), Docs::Api.title());
    for (relative, contents) in morf_lua::generate_types() {
        out.push_str(&format!("\n## {relative}\n"));
        for line in contents.lines() {
            if let Some(name) = line.strip_prefix("---@class ") {
                out.push_str(&format!("\n### {name}\n"));
            } else if let Some(field) = line.strip_prefix("---@field ") {
                out.push_str(&format!("- `{field}`\n"));
            } else if let Some(function) = line
                .strip_prefix("function ")
                .and_then(|rest| rest.strip_suffix(" end"))
            {
                out.push_str(&format!("- `{function}`\n"));
            }
        }
    }
    out
}

/// What `morf docs` prints for `docs`, guides from `bundles` for `version`.
pub(crate) fn render_from(docs: Docs, bundles: &[Bundle], version: &str) -> Result<String, String> {
    if docs == Docs::Api {
        return Ok(api_reference());
    }
    let (path, text) = guide(docs, bundles, version)?;
    Ok(format!(
        "# morf {version} {}\n<!-- {} -->\n\n{text}",
        docs.title(),
        path.display()
    ))
}

/// What `morf docs` prints for `docs`.
pub(crate) fn render(docs: Docs) -> Result<String, String> {
    render_from(docs, &bundles(), env!("CARGO_PKG_VERSION"))
}
