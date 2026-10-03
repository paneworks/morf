//! The XDG base directories: where data -- icons, desktop entries,
//! sessions -- is looked for.

use std::path::PathBuf;

/// The XDG base directories every data lookup searches, in order.
///
/// `$XDG_DATA_HOME` (or `~/.local/share`), then each `$XDG_DATA_DIRS`
/// entry, then the spec's default `/usr/local/share:/usr/share` for
/// whichever of the two the list left out. The spec only falls back to the
/// default when the variable is unset, but a Nix development shell sets it
/// to the store paths alone, and a shell run from one then finds no
/// application, no icon and no session on a machine that has hundreds; so
/// the system directories are always searched, last, where they cannot
/// shadow anything the environment named first.
pub fn data_dirs() -> Vec<PathBuf> {
    data_dirs_from(
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
        std::env::var_os("XDG_DATA_DIRS"),
    )
}

/// [`data_dirs`] over explicit values, for testing.
pub fn data_dirs_from(
    data_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    data_dirs: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = home.map(PathBuf::from);
    let data_home = data_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".local/share")));
    roots.extend(data_home);
    let data_dirs = data_dirs
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(std::env::split_paths(&data_dirs).filter(|path| !path.as_os_str().is_empty()));
    roots.extend(["/usr/local/share", "/usr/share"].map(PathBuf::from));
    // Flatpak adds its exports to the list from a login profile script, so a
    // shell started some other way (a Nix shell, a service) never sees the
    // applications and icons of anything installed with it.
    roots.extend(
        home.as_ref()
            .map(|home| home.join(".local/share/flatpak/exports/share")),
    );
    roots.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    let mut unique = Vec::with_capacity(roots.len());
    for root in roots {
        // `/usr/share/` and `/usr/share` are one directory.
        let root = root.components().collect::<PathBuf>();
        if !unique.contains(&root) {
            unique.push(root);
        }
    }
    unique
}
