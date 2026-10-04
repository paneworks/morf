//! The hub's bookkeeping, without a program to run.

use super::*;

fn spec() -> Spec {
    Spec {
        command: vec!["true".into()],
        environment: BTreeMap::new(),
        working_directory: None,
        scrollback: DEFAULT_SCROLLBACK,
    }
}

#[test]
fn colours_read_from_names_and_values() {
    assert_eq!(
        rgba(&SceneValue::String("#ff0000".into())),
        Some([255, 0, 0, 255])
    );
    assert_eq!(rgba(&SceneValue::Number(1.0)), None);
    let colors = SceneValue::Map(BTreeMap::from([
        ("foreground".into(), SceneValue::String("#00ff00".into())),
        (
            "palette".into(),
            SceneValue::List(vec![SceneValue::String("#0000ff".into())]),
        ),
    ]));
    let palette = palette_of(&colors);
    assert_eq!(palette.foreground, [0, 255, 0, 255]);
    assert_eq!(palette.ansi[0], [0, 0, 255, 255]);
    assert_eq!(palette.ansi[1], Palette::default().ansi[1]);
}

#[test]
fn what_is_written_before_the_start_waits_and_a_removed_terminal_is_gone() {
    let mut scene = Scene::default();
    let node = scene.create(morf_scene::Element::Terminal);
    let mut hub: Hub<u32> = Hub::default();
    assert!(hub.write(node, b"x".to_vec()).is_err());
    hub.register(node, spec(), Callbacks::default());
    assert_eq!(hub.len(), 1);
    hub.write(node, b"echo hi\n".to_vec()).unwrap();
    assert_eq!(hub.entries[&node].pending, b"echo hi\n");
    assert_eq!(hub.pid(node), None);
    assert!(!hub.kill(node, 15));
    hub.remove(node);
    assert!(!hub.contains(node));
    assert!(hub.take_effects().properties.is_empty());
}
