use xkbcommon::xkb;

/// The directories xkbcommon searches for its data (`rules/`, `symbols/`,
/// ...), in its own order: where this machine keeps them, whatever the
/// distribution.
pub fn xkb_include_paths() -> Vec<String> {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    context
        .include_paths()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}
