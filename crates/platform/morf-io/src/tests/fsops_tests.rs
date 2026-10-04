//! Tests for the filesystem operations.

use crate::fs as ops;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-fsops-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn patterns_match_like_a_shell() {
    assert!(ops::matches("*.jpeg", "tiger.jpeg"));
    assert!(!ops::matches("*.jpeg", "tiger.jpg"));
    assert!(ops::matches("*.{jpg,jpeg,png}", "a.png"));
    assert!(ops::matches("t?ger", "tiger"));
    assert!(ops::matches("[a-c]*", "bengal"));
    assert!(!ops::matches("[!a-c]*", "bengal"));
    assert!(ops::matches("*", ""));
    assert!(ops::matches("a*b*c", "aXXbYYc"));
    assert!(!ops::matches("a*b*c", "aXXbYY"));
    assert!(ops::matches("[", "["));
}

#[test]
fn list_write_copy_rename_and_remove() {
    let dir = scratch("ops");
    ops::write(
        &dir.join("sub/a.txt"),
        b"one",
        ops::WriteOptions {
            parents: true,
            ..Default::default()
        },
    )
    .unwrap();
    ops::write(
        &dir.join("sub/a.txt"),
        b" two",
        ops::WriteOptions {
            append: true,
            ..Default::default()
        },
    )
    .unwrap();
    ops::write(
        &dir.join("b.json"),
        b"{}",
        ops::WriteOptions {
            atomic: true,
            ..Default::default()
        },
    )
    .unwrap();
    ops::write(&dir.join(".hidden"), b"", Default::default()).unwrap();
    assert_eq!(ops::read(&dir.join("sub/a.txt"), 64).unwrap(), b"one two");
    assert!(ops::read(&dir.join("sub/a.txt"), 3).is_err());

    let (flat, truncated) = ops::list(&dir, Default::default()).unwrap();
    assert!(!truncated);
    let names = flat.iter().map(|e| e.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, ["b.json", "sub"]);
    let (deep, _) = ops::list(
        &dir,
        ops::ListOptions {
            hidden: true,
            depth: 4,
            follow: false,
        },
    )
    .unwrap();
    assert_eq!(deep.len(), 4);
    assert_eq!(
        ops::stat(&dir.join("sub")).unwrap().kind,
        ops::EntryKind::Dir
    );
    assert_eq!(ops::stat(&dir.join("b.json")).unwrap().size, 2);

    assert_eq!(
        ops::copy(&dir.join("sub"), &dir.join("copy"), true).unwrap(),
        7
    );
    ops::rename(&dir.join("copy/a.txt"), &dir.join("copy/c.txt")).unwrap();
    assert!(ops::stat(&dir.join("copy/c.txt")).is_ok());
    assert!(ops::remove(&dir.join("copy"), false).is_err());
    ops::remove(&dir.join("copy"), true).unwrap();
    assert!(ops::stat(&dir.join("copy")).is_err());
    assert!(ops::remove(std::path::Path::new("/"), true).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn globs_walk_segments_and_double_star() {
    let dir = scratch("glob");
    for path in ["w/a.jpg", "w/b.png", "w/deep/c.jpg", "w/.x.jpg", "n/d.txt"] {
        ops::write(
            &dir.join(path),
            b"",
            ops::WriteOptions {
                parents: true,
                ..Default::default()
            },
        )
        .unwrap();
    }
    let base = dir.to_string_lossy();
    let (flat, _) = ops::glob(&format!("{base}/w/*.{{jpg,png}}"));
    assert_eq!(flat.len(), 2);
    let (deep, _) = ops::glob(&format!("{base}/**/*.jpg"));
    assert_eq!(deep.len(), 2, "{deep:?}");
    let (dots, _) = ops::glob(&format!("{base}/w/.*.jpg"));
    assert_eq!(dots.len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn expand_and_normalize_paths() {
    let home = std::env::var("HOME").unwrap();
    assert_eq!(ops::expand("~/x"), format!("{home}/x"));
    assert_eq!(ops::expand("${HOME}/y"), format!("{home}/y"));
    assert_eq!(ops::expand("$HOME"), home);
    assert_eq!(
        ops::normalize(std::path::Path::new("/a/b/../c/./d")),
        PathBuf::from("/a/c/d")
    );
    assert!(ops::user_dir("config").is_some());
    assert!(ops::user_dir("nonsense").is_none());
}
