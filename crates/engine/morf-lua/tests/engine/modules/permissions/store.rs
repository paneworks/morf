//! Approvals: fingerprinted over the module tree and its manifest, looked up
//! before anything is hashed, and kept under the canonical shell root.

use std::path::PathBuf;

use morf_lua::{Approval, Approvals, ModuleDeclaration, ModuleKind, Origin, Runtime};

use super::{Fixture, appears, stays_absent, touch};

#[test]
fn an_edit_to_the_module_or_its_manifest_file_lapses_the_approval() {
    let fixture = Fixture::new("lapse");
    let entry = fixture.write("site/plug/widget.lua", "return { answer = 42 }");
    let manifest = fixture.write("site/plug/widget.permissions.lua", "return { run = true }");
    let mut runtime = fixture.runtime();
    let record = runtime.declare_module("plug.widget").unwrap();
    assert_eq!(record.tree().files, [entry.clone(), manifest.clone()]);
    let mut approvals = Approvals::load(&fixture.store()).unwrap();
    assert_eq!(approvals.status(&record), Approval::Unknown);
    approvals.approve(&record).unwrap();
    approvals.save(&fixture.store()).unwrap();
    let approvals = Approvals::load(&fixture.store()).unwrap();
    assert_eq!(
        approvals.status(&record),
        Approval::Approved,
        "survives a round trip"
    );

    std::fs::write(&entry, "return { answer = 43 }").unwrap();
    assert_eq!(
        approvals.status(&record),
        Approval::Lapsed,
        "the code changed"
    );
    std::fs::write(&entry, "return { answer = 42 }").unwrap();
    assert_eq!(approvals.status(&record), Approval::Approved);

    // The manifest's bytes are part of the tree: a comment is enough.
    std::fs::write(&manifest, "-- now with a comment\nreturn { run = true }").unwrap();
    assert_eq!(
        approvals.status(&record),
        Approval::Lapsed,
        "the manifest file changed"
    );
}

#[test]
fn a_directory_module_is_its_whole_folder() {
    let fixture = Fixture::new("dir-tree");
    fixture.write("site/pack/init.lua", "return {}");
    fixture.write("site/pack/part.lua", "return {}");
    fixture.write("site/neighbour.lua", "return {}");
    let mut runtime = fixture.runtime();
    let record = runtime.declare_module("pack").unwrap();
    let files: Vec<_> = record
        .tree()
        .files
        .iter()
        .map(|f| f.file_name().unwrap().to_owned())
        .collect();
    assert_eq!(files, ["init.lua", "part.lua"], "not the folder around it");
    let mut approvals = Approvals::default();
    approvals.approve(&record).unwrap();
    fixture.write("site/pack/added.lua", "return {}");
    assert_eq!(
        approvals.status(&record),
        Approval::Lapsed,
        "a new file lapses it"
    );
}

#[test]
fn a_store_without_an_entry_answers_without_reading_the_module() {
    let missing = ModuleDeclaration::new(
        "gone",
        PathBuf::from("/nonexistent/morf/gone.lua"),
        Origin::Imported,
        ModuleKind::Module,
    );
    let approvals = Approvals::default();
    assert_eq!(approvals.status(&missing), Approval::Unknown);
    assert!(approvals.is_empty());
}

#[test]
fn an_approved_module_loads_free_until_its_manifest_is_edited() {
    let fixture = Fixture::new("end-to-end");
    let mark = fixture.out("ran");
    fixture.write("site/asks.lua", &format!("return {{ {} }}", touch(&mark)));
    let manifest = fixture.write("site/asks.permissions.lua", "return { run = { 'touch' } }");
    let config = "local r = require('asks') RESULT = r[1] ~= nil";

    let mut first = fixture.runtime();
    fixture.run(&mut first, config);
    assert!(stays_absent(&mark), "unapproved: held");
    let record = fixture.record(&first, "asks");
    let mut approvals = Approvals::load(&fixture.store()).unwrap();
    approvals.approve(&record).unwrap();
    approvals.save(&fixture.store()).unwrap();

    let mut second = fixture.runtime();
    fixture.run(&mut second, config);
    assert!(appears(&mark), "approved: free");
    std::fs::remove_file(&mark).unwrap();

    std::fs::write(&manifest, "return { run = { 'touch', 'sh' } }").unwrap();
    let mut third = fixture.runtime();
    fixture.run(&mut third, config);
    assert!(
        stays_absent(&mark),
        "the manifest changed: asked about again"
    );
}

#[test]
fn approvals_live_under_the_canonical_shell_root() {
    let fixture = Fixture::new("store-path");
    let real = fixture.config();
    let link = fixture.root.join("linked-config");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let store = |root: PathBuf| {
        let mut runtime = Runtime::default();
        runtime.set_shell_root(root);
        runtime.approvals_path().unwrap()
    };
    let canonical = store(real.clone());
    assert_eq!(store(link), canonical, "through a symlinked folder");
    assert_eq!(
        store(real.join("../config")),
        canonical,
        "through a dotted path"
    );
    assert!(canonical.ends_with(morf_lua::APPROVALS_FILE));
}
