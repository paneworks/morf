//! `morf docs` and `--install-skill` (PLAN item 8): the documentation a
//! binary prints is its own version's, and the skill it installs is the
//! generated one, byte for byte.

use std::path::PathBuf;

use crate::commands::{api_reference, install_skill, ui_guide_from};

#[test]
fn the_api_reference_carries_this_versions_header() {
    let reference = api_reference();
    let header = format!("# morf {} API reference", env!("CARGO_PKG_VERSION"));
    assert!(reference.starts_with(&header), "{}", &reference[..80]);
    assert!(reference.contains("signal"), "the reference lists the API");
    assert!(
        reference.contains("ui.Rect"),
        "the reference lists ui constructors"
    );
}

#[test]
fn a_missing_ui_guide_names_where_it_looked() {
    let missing = PathBuf::from("/nonexistent/morf/library/docs/UI.md");
    let error = ui_guide_from(std::slice::from_ref(&missing)).unwrap_err();
    assert!(error.contains("docs/UI.md"), "{error}");
    assert!(error.contains("/nonexistent"), "{error}");
}

#[test]
fn install_skill_writes_the_generated_skill_and_is_idempotent() {
    let home = std::env::temp_dir().join(format!("morf-skill-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let first = install_skill(&home).unwrap();
    assert_eq!(first.len(), 2);
    assert!(
        first.iter().all(|line| line.contains("(written)")),
        "{first:?}"
    );
    for agent in [".claude", ".codex"] {
        let path = home.join(agent).join("skills/morf/SKILL.md");
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("# morf"), "{agent} skill has the header");
        assert!(
            written.contains("morf.font_families"),
            "{agent} skill has the inventory"
        );
    }
    let second = install_skill(&home).unwrap();
    assert!(
        second.iter().all(|line| line.contains("(already current)")),
        "{second:?}"
    );
    // Exactly the generated files, nothing else.
    let mut files = Vec::new();
    collect(&home, &mut files);
    assert_eq!(files.len(), 2, "{files:?}");
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn an_approval_prompt_says_what_a_module_asks_for() {
    let record = morf_lua::ModuleDeclaration {
        name: "plug.widget".to_owned(),
        path: PathBuf::from("/site/plug/widget.lua"),
        imported: true,
        declared: true,
        groups: vec!["dbus".to_owned(), "run".to_owned()],
    };
    let text = crate::commands::describe_module(&record);
    assert!(text.contains("may talk to the message bus"), "{text}");
    assert!(text.contains("may run programs"), "{text}");
    assert!(
        text.contains("runs anything it is told: this asks for everything."),
        "{text}"
    );
}

fn collect(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
