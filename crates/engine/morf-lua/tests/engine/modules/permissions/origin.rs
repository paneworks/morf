//! Origins: config-local, first-party and imported, decided on canonical
//! paths with the most specific root winning.

use std::path::Path;

use morf_lua::{Origin, Runtime};

use super::Fixture;

fn origin(runtime: &mut Runtime, name: &str) -> Origin {
    runtime
        .declare_module(name)
        .unwrap_or_else(|| panic!("{name} does not resolve"))
        .origin
}

#[test]
fn config_library_and_site_modules_have_their_own_origins() {
    let fixture = Fixture::new("origins");
    fixture.write("config/local.lua", "return {}");
    fixture.write("library/lib/util/thing.lua", "return {}");
    fixture.write("site/plug.lua", "return {}");
    let mut runtime = fixture.runtime();
    assert_eq!(origin(&mut runtime, "local"), Origin::ConfigLocal);
    assert_eq!(origin(&mut runtime, "lib.util.thing"), Origin::FirstParty);
    assert_eq!(origin(&mut runtime, "plug"), Origin::Imported);
}

#[test]
fn a_configuration_above_the_data_folders_does_not_own_them() {
    // A configuration at `~/shell.lua`, its site folder under `~/.local`.
    let fixture = Fixture::new("ancestor");
    let home = fixture.root.join("home");
    let site = home.join(".local/share/morf/site");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(site.join("plug.lua"), "return {}").unwrap();
    std::fs::write(home.join("mine.lua"), "return {}").unwrap();
    let mut runtime = Runtime::default();
    runtime.set_engine_library(None);
    runtime.set_module_roots(vec![home.clone(), site]);
    runtime.set_config_file(&home.join("shell.lua"));
    assert_eq!(origin(&mut runtime, "plug"), Origin::Imported);
    assert_eq!(origin(&mut runtime, "mine"), Origin::ConfigLocal);
}

#[test]
fn a_symlink_kept_in_the_configuration_is_its_own() {
    // A shell's parts sharing `../themes` through a link in each part.
    let fixture = Fixture::new("symlink");
    let shared = fixture.write("shared/themes/init.lua", "return {}");
    std::os::unix::fs::symlink(
        fixture.root.join("shared/themes"),
        fixture.config().join("themes"),
    )
    .unwrap();
    let mut runtime = fixture.runtime();
    let record = runtime.declare_module("themes").unwrap();
    assert_eq!(record.origin, Origin::ConfigLocal);
    assert_eq!(record.path, shared, "recorded at its canonical path");
}

#[test]
fn a_module_found_on_an_imported_root_stays_imported_wherever_it_links() {
    let fixture = Fixture::new("symlink-in");
    let mine = fixture.write("config/mine.lua", "return {}");
    std::os::unix::fs::symlink(&mine, fixture.site().join("borrowed.lua")).unwrap();
    let mut runtime = fixture.runtime();
    assert_eq!(origin(&mut runtime, "borrowed"), Origin::Imported);
}

#[test]
fn an_unchanged_copy_of_a_library_file_is_first_party_and_an_edit_is_not() {
    let fixture = Fixture::new("copies");
    let shipped = "return { shipped = true }";
    fixture.write("library/lib/util/same.lua", shipped);
    fixture.write("library/lib/util/edited.lua", shipped);
    let runtime_root = fixture.root.join("runtime-path");
    for (name, text) in [("same", shipped), ("edited", "return { shipped = false }")] {
        let path = runtime_root.join(format!("lib/util/{name}.lua"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut runtime = fixture.runtime();
    runtime.set_module_roots(vec![fixture.config(), runtime_root]);
    assert_eq!(origin(&mut runtime, "lib.util.same"), Origin::FirstParty);
    assert_eq!(origin(&mut runtime, "lib.util.edited"), Origin::Imported);
}

#[test]
fn integrations_are_imported_even_from_the_engine_library() {
    let fixture = Fixture::new("integrations");
    fixture.write("library/lib/integrations/weather.lua", "return {}");
    let mut runtime = fixture.runtime();
    let record = runtime.declare_module("lib.integrations.weather").unwrap();
    assert_eq!(record.origin, Origin::Imported);
    assert!(record.is_integration());
}

#[test]
fn a_bare_configuration_path_still_has_a_folder() {
    let bare = morf_lua::canonical_path(Path::new("shell.lua"));
    assert!(bare.is_absolute(), "{}", bare.display());
    let folder = bare.parent().unwrap();
    assert_eq!(folder, std::fs::canonicalize(".").unwrap());
}
