//! Manifests: one per module, read as data, values honoured.

use morf_lua::Grant;

use super::{Fixture, stays_absent};

#[test]
fn single_file_modules_in_one_folder_declare_apart() {
    let fixture = Fixture::new("manifests");
    fixture.write("site/kit/weather.lua", "return {}");
    fixture.write(
        "site/kit/weather.permissions.lua",
        r#"return { http = { "api.example.org", "geo.example.org" }, run = false }"#,
    );
    fixture.write("site/kit/shell.lua", "return {}");
    fixture.write(
        "site/kit/shell.permissions.lua",
        r#"return { run = { "sh" }, services = "power", fs = true, telepathy = true }"#,
    );
    fixture.write("site/kit/quiet.lua", "return {}");
    let mut runtime = fixture.runtime();
    let weather = runtime.declare_module("kit.weather").unwrap();
    assert_eq!(
        weather.grants.get("http"),
        Some(&Grant::Names(vec![
            "api.example.org".into(),
            "geo.example.org".into()
        ]))
    );
    assert!(
        !weather.grants.contains_key("run"),
        "run = false declares nothing"
    );
    let shell = runtime.declare_module("kit.shell").unwrap();
    assert_eq!(
        shell.grants.get("run"),
        Some(&Grant::Names(vec!["sh".into()]))
    );
    assert_eq!(
        shell.grants.get("services"),
        Some(&Grant::Names(vec!["power".into()]))
    );
    assert_eq!(shell.grants.get("fs"), Some(&Grant::Any));
    assert_eq!(shell.unknown_groups(), ["telepathy"]);
    let quiet = runtime.declare_module("kit.quiet").unwrap();
    assert!(!quiet.declared(), "a sibling's manifest is not its own");
}

#[test]
fn a_directory_module_declares_in_its_own_folder() {
    let fixture = Fixture::new("dir-manifest");
    fixture.write("site/tray/init.lua", "return {}");
    fixture.write("site/tray/permissions.lua", r#"return { dbus = true }"#);
    let mut runtime = fixture.runtime();
    let tray = runtime.declare_module("tray").unwrap();
    assert_eq!(
        tray.manifest,
        Some(fixture.site().join("tray/permissions.lua"))
    );
    assert_eq!(tray.grants.get("dbus"), Some(&Grant::Any));
}

#[test]
fn a_manifest_is_data_with_no_api_and_its_module_still_loads() {
    let fixture = Fixture::new("manifest-data");
    let marker = fixture.out("manifest-ran");
    let cases = [
        (
            "spawner",
            format!(
                r#"morf.spawn {{ command = {{ "touch", "{}" }} }} return {{}}"#,
                marker.display()
            ),
        ),
        (
            "requirer",
            r#"local m = require("morf") return {}"#.to_owned(),
        ),
        (
            "global",
            r#"return { run = os ~= nil or string ~= nil }"#.to_owned(),
        ),
        ("number", "return 42".to_owned()),
        ("forever", "while true do end".to_owned()),
    ];
    for (name, manifest) in &cases {
        fixture.write(&format!("site/{name}.lua"), "return { loaded = true }");
        fixture.write(&format!("site/{name}.permissions.lua"), manifest);
    }
    let mut runtime = fixture.runtime();
    let names = cases
        .iter()
        .map(|(name, _)| format!("'{name}'"))
        .collect::<Vec<_>>();
    fixture.run(
        &mut runtime,
        &format!(
            "for _, name in ipairs({{ {} }}) do assert(require(name).loaded) end",
            names.join(", ")
        ),
    );
    for (name, _) in &cases {
        let record = fixture.record(&runtime, name);
        if *name == "global" {
            assert!(record.problem.is_none(), "{record:?}");
            assert!(
                !record.grants.contains_key("run"),
                "no globals reach a manifest"
            );
        } else {
            assert!(record.problem.is_some(), "{name}: {record:?}");
        }
    }
    assert!(stays_absent(&marker), "a manifest never runs with the API");
}
