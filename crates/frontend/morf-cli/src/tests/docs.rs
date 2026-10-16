//! `morf docs` (PLAN item 8): the documentation a binary prints is its
//! own version's.

use std::path::PathBuf;

use crate::commands::{api_reference, ui_guide_from};

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
