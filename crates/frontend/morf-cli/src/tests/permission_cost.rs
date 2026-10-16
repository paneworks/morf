//! What the permission bookkeeping costs one caelestia load, measured:
//! `cargo test --release -p morf-cli -- --ignored --nocapture permission_cost`.
//!
//! Before: every module outside the configuration's folder was hashed over
//! its whole folder on each `require`. Now: every module is classified, and
//! only an imported one with a store entry is hashed, over its own tree.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use morf_host::headless::{Headless, LoadOptions};
use morf_lua::{Approvals, ModuleDeclaration, PermissionMode, module_fingerprint};

fn caelestia() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/shells/caelestia/shell/init.lua")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn best_of<T>(runs: usize, mut work: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = None;
    for _ in 0..runs {
        let started = Instant::now();
        last = Some(work());
        best = best.min(started.elapsed());
    }
    (best, last.expect("at least one run"))
}

#[test]
#[ignore = "a measurement, not a check"]
fn permission_cost() {
    #[cfg(feature = "kit")]
    morf_lua::register_kit();
    let config = morf_lua::canonical_path(&caelestia());
    let mut options = LoadOptions::new(config.clone());
    options.permissions = PermissionMode::HoldImported;
    options.approvals = Some(std::env::temp_dir().join("morf-permission-cost.json"));
    let mut headless = Headless::load(&options)
        .map_err(|failure| failure.error)
        .unwrap();
    let records = headless.runtime.module_declarations();
    let outside: Vec<_> = records
        .iter()
        .filter(|r| r.origin != morf_lua::Origin::ConfigLocal)
        .collect();

    // The old fingerprint: the whole folder of the module's entry.
    let (before, (files, bytes)) = best_of(5, || {
        let (mut files, mut bytes) = (0usize, 0u64);
        for record in &outside {
            let folder = ModuleDeclaration {
                path: record.path.with_file_name("init.lua"),
                ..(*record).clone()
            };
            let tree = folder.tree();
            files += tree.files.len();
            bytes += tree
                .files
                .iter()
                .filter_map(|f| f.metadata().ok())
                .map(|m| m.len())
                .sum::<u64>();
            let _ = module_fingerprint(&folder);
        }
        (files, bytes)
    });

    // Now: classification of every module, a store lookup for each
    // imported one, its manifest read as data.
    let approvals = Approvals::default();
    let names: Vec<_> = records
        .iter()
        .filter(|r| r.kind == morf_lua::ModuleKind::Module)
        .map(|r| r.name.clone())
        .collect();
    let (after, imported) = best_of(5, || {
        let mut imported = 0;
        for name in &names {
            if let Some(record) = headless.runtime.declare_module(name)
                && record.imported()
            {
                imported += 1;
                let _ = approvals.status(&record);
            }
        }
        imported
    });
    // The engine's library installed elsewhere (a Nix store path on
    // `MORF_RUNTIME_PATH`): every module outside it is compared byte for
    // byte with the installed copy.
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../library");
    let copy = std::env::temp_dir().join("morf-permission-cost-library");
    let _ = std::fs::remove_dir_all(&copy);
    copy_tree(&library.join("lib"), &copy.join("lib"));
    let (compared, _) = best_of(5, || {
        headless.runtime.set_engine_library(Some(copy.clone()));
        for name in &names {
            let _ = headless.runtime.declare_module(name);
        }
    });
    let _ = std::fs::remove_dir_all(&copy);
    println!(
        "caelestia: {} modules, {} outside its folder, {imported} imported",
        records.len(),
        outside.len()
    );
    println!("library elsewhere: {compared:?}, each module compared with the installed copy");
    println!("before: {before:?} hashing {files} files, {bytes} bytes");
    println!(
        "after:  {after:?} classifying {} modules, nothing hashed",
        names.len()
    );
}
