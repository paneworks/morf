//! `morf docs`: the API reference is this build's, and a
//! guide prints only from a bundle stamped with this executable's version.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::config::{Command, parse_command};
use crate::docs::{Bundle, Docs, Stamp, bundles, render_from, version_line, workspace_version};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A bundle folder holding both guides, stamped with `stamp` when given.
fn fixture(name: &str, stamp: Option<&str>, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-docs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("UI.md"), format!("# UI\n{body}\n")).unwrap();
    std::fs::write(dir.join("IO.md"), format!("# IO\n{body}\n")).unwrap();
    if let Some(stamp) = stamp {
        std::fs::write(dir.join("VERSION"), format!("{stamp}\n")).unwrap();
    }
    dir
}

fn docs_of(words: &[&str]) -> Result<Docs, String> {
    let args: Vec<OsString> = words.iter().map(OsString::from).collect();
    match parse_command(&args)? {
        Command::Docs(docs) => Ok(docs),
        other => Err(format!("not docs: {other:?}")),
    }
}

#[test]
fn every_docs_flag_parses_and_an_unknown_one_is_refused() {
    assert_eq!(docs_of(&["docs"]), Ok(Docs::Api));
    assert_eq!(docs_of(&["docs", "--api"]), Ok(Docs::Api));
    assert_eq!(docs_of(&["docs", "--ui"]), Ok(Docs::Ui));
    assert_eq!(docs_of(&["docs", "--io"]), Ok(Docs::Io));
    let error = docs_of(&["docs", "--guide"]).unwrap_err();
    assert!(
        error.contains("not `--guide`") && error.contains("--io"),
        "{error}"
    );
    let error = docs_of(&["docs", "--api", "--ui"]).unwrap_err();
    assert!(error.contains("not `--api --ui`"), "{error}");
}

#[test]
fn the_api_reference_header_is_what_morf_version_prints() {
    assert_eq!(version_line(), format!("morf {VERSION}"));
    let reference = render_from(Docs::Api, &[], "0.0.0-elsewhere").unwrap();
    let header = reference.lines().next().unwrap();
    assert_eq!(header, format!("# {} API reference", version_line()));
    assert!(reference.contains("signal"), "the reference lists the API");
    assert!(reference.contains("ui.Rect"), "and the ui constructors");
}

#[test]
fn a_guide_stamped_for_another_version_is_refused() {
    let stale = fixture("stale", Some("0.0.1-stale"), "stale words");
    let error = render_from(Docs::Ui, &[Bundle::packaged(stale.clone())], VERSION).unwrap_err();
    assert!(
        error.contains(&stale.join("UI.md").display().to_string()),
        "{error}"
    );
    assert!(error.contains("0.0.1-stale"), "{error}");
    assert!(
        error.contains(&format!("this is morf {VERSION}")),
        "{error}"
    );
    let error = render_from(Docs::Io, &[Bundle::packaged(stale)], VERSION).unwrap_err();
    assert!(
        error.contains("IO.md") && error.contains("0.0.1-stale"),
        "{error}"
    );
}

#[test]
fn a_guide_stamped_for_this_version_prints_under_the_version_header() {
    let matching = fixture("matching", Some(VERSION), "current words");
    let text = render_from(Docs::Ui, &[Bundle::packaged(matching.clone())], VERSION).unwrap();
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(format!("# {} UI guide", version_line()).as_str())
    );
    assert!(
        lines
            .next()
            .unwrap()
            .contains(&matching.join("UI.md").display().to_string())
    );
    assert!(text.contains("current words"), "{text}");
}

#[test]
fn a_stale_bundle_never_wins_over_a_matching_one_after_it() {
    let stale = fixture("first-stale", Some("0.0.1-stale"), "stale words");
    let matching = fixture("then-matching", Some(VERSION), "current words");
    let bundles = [Bundle::packaged(stale), Bundle::packaged(matching)];
    let text = render_from(Docs::Io, &bundles, VERSION).unwrap();
    assert!(
        text.contains("current words") && !text.contains("stale words"),
        "{text}"
    );
}

#[test]
fn an_unstamped_or_missing_guide_names_every_place_looked() {
    let unstamped = fixture("unstamped", None, "words");
    let missing = PathBuf::from("/nonexistent/morf/library/docs");
    let bundles = [
        Bundle::packaged(unstamped.clone()),
        Bundle::packaged(missing),
    ];
    let error = render_from(Docs::Ui, &bundles, VERSION).unwrap_err();
    assert!(error.contains(&format!(
        "no version stamp at {}",
        unstamped.join("VERSION").display()
    )));
    assert!(
        error.contains("/nonexistent/morf/library/docs/UI.md: not there"),
        "{error}"
    );
}

#[test]
fn a_checkout_is_stamped_by_its_workspace_version() {
    let root = fixture("checkout", None, "checkout words").join("tree");
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/UI.md"), "checkout words\n").unwrap();
    let bundle = |version: &str| {
        let manifest =
            format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{version}\"\n");
        std::fs::write(root.join("Cargo.toml"), manifest).unwrap();
        Bundle {
            dir: root.join("docs"),
            stamp: Stamp::Checkout(root.join("Cargo.toml")),
        }
    };
    assert!(render_from(Docs::Ui, &[bundle(VERSION)], VERSION).is_ok());
    let error = render_from(Docs::Ui, &[bundle("9.9.9")], VERSION).unwrap_err();
    assert!(error.contains("stamped for morf 9.9.9"), "{error}");
}

#[test]
fn the_workspace_version_is_read_from_its_own_section() {
    let manifest = "[package]\nversion = \"1.0.0\"\n[workspace.package]\nedition = \"2024\"\nversion = \"0.2.9\"\n";
    assert_eq!(workspace_version(manifest).as_deref(), Some("0.2.9"));
    assert_eq!(workspace_version("[package]\nversion = \"1.0.0\"\n"), None);
}

#[test]
fn the_package_beside_the_executable_is_looked_at_first() {
    let bin = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let found = bundles();
    assert_eq!(
        found[0].dir,
        bin.parent().unwrap().join("share/morf/library/docs")
    );
    assert_eq!(found[1].dir, bin.join("library/docs"));
    assert!(matches!(&found[0].stamp, Stamp::File(path) if path.ends_with("docs/VERSION")));
}

#[test]
fn a_closed_reader_is_told_apart_from_other_write_failures() {
    struct Closed;
    impl std::io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let error = crate::stdout::emit(&mut Closed, format_args!("x"), true).unwrap_err();
    assert!(crate::stdout::reader_gone(&error));
    assert!(!crate::stdout::reader_gone(
        &std::io::ErrorKind::Other.into()
    ));
    let mut written = Vec::new();
    crate::stdout::emit(&mut written, format_args!("a{}", 1), true).unwrap();
    assert_eq!(written, b"a1\n");
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
