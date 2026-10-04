//! Views with no VM: a scrolling list builds only what is in sight and
//! rebinds pooled delegates as it scrolls, a delegate without an updater is
//! rebuilt when its row changes, and a `Repeater` keeps its children in the
//! model's order.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use morf_scene::{Element, ListModel, NodeHandle, Scene, Value, VirtualList};

use super::*;
use crate::{HandlerId, HandlerRegistry};

struct Nowhere;

impl HandlerRegistry for Nowhere {
    fn release(&self, _: HandlerId) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn handler(id: u64) -> Handler {
    Handler::new(HandlerId(id), Rc::new(Nowhere))
}

#[derive(Default)]
struct Host {
    scene: Scene,
    built: usize,
    updated: Vec<(Value, usize)>,
    removed: usize,
    updaters: bool,
}

impl ViewHost for Host {
    fn build(&mut self, _: &Handler, item: &Value, index: usize) -> Result<Delegate, String> {
        self.built += 1;
        Ok(Delegate {
            node: self.scene.create(Element::Item),
            updater: self.updaters.then(|| handler(2)),
            item: item.clone(),
            index,
        })
    }
    fn update(&mut self, _: &Handler, item: &Value, index: usize) -> Result<(), String> {
        self.updated.push((item.clone(), index));
        Ok(())
    }
    fn create(&mut self) -> NodeHandle {
        self.scene.create(Element::Item)
    }
    fn remove(&mut self, node: NodeHandle) {
        self.removed += 1;
        let _ = self.scene.reparent(node, None);
    }
    fn begin_exit(&mut self, _: NodeHandle) -> bool {
        false
    }
    fn cancel_exit(&mut self, _: NodeHandle) -> bool {
        false
    }
    fn with_scene<R>(&mut self, f: impl FnOnce(&mut Scene) -> R) -> R {
        f(&mut self.scene)
    }
}

fn rows(count: usize) -> Rc<RefCell<ListModel>> {
    Rc::new(RefCell::new(ListModel::new(
        (0..count).map(|n| Value::Number(n as f64)),
    )))
}

fn view(model: Rc<RefCell<ListModel>>, list: VirtualList, positioned: bool) -> VirtualView {
    VirtualView {
        model,
        view: list,
        delegate: handler(1),
        active: HashMap::new(),
        reusable: HashMap::new(),
        reuse_order: Default::default(),
        reuse_limit: 8,
        pool_root: None,
        exiting: Vec::new(),
        column_extent: 0.0,
        positioned,
        size_field: None,
        kind_field: None,
    }
}

#[test]
fn a_scrolling_list_rebinds_what_scrolls_out_of_sight() {
    let mut host = Host {
        updaters: true,
        ..Host::default()
    };
    let parent = host.scene.create(Element::Item);
    let mut list = view(rows(100), VirtualList::new(10.0, 30.0, 0).unwrap(), true);
    list.reconcile(&mut host, parent, 0.0).unwrap();
    let shown = list.active.len();
    assert!(shown > 0 && shown < 10 && host.built == shown);
    list.reconcile(&mut host, parent, 500.0).unwrap();
    assert_eq!(host.built, shown, "rows coming into sight reuse the pool");
    assert_eq!(host.updated.len(), shown);
    assert!(list.active.values().all(|d| d.index >= 50));
    let first = list.active.values().find(|d| d.index == 50).unwrap();
    assert_eq!(host.scene.number(first.node, "y").ok(), Some(0.0));
    assert_eq!(list.content_extent(), 1000.0);
}

#[test]
fn a_row_whose_delegate_cannot_patch_itself_is_rebuilt() {
    let mut host = Host::default();
    let parent = host.scene.create(Element::Item);
    let model = rows(3);
    let mut list = view(Rc::clone(&model), VirtualList::new_unbounded(), false);
    list.reconcile(&mut host, parent, 0.0).unwrap();
    assert_eq!(host.built, 3);
    model.borrow_mut().set(1, Value::String("changed".into()));
    list.reconcile(&mut host, parent, 0.0).unwrap();
    assert_eq!((host.built, host.removed), (4, 1));
    assert!(host.updated.is_empty());
}

#[test]
fn a_repeater_follows_the_models_order() {
    let mut host = Host {
        updaters: true,
        ..Host::default()
    };
    let parent = host.scene.create(Element::Item);
    let model = rows(3);
    let mut list = view(Rc::clone(&model), VirtualList::new_unbounded(), false);
    list.reconcile(&mut host, parent, 0.0).unwrap();
    model.borrow_mut().move_item(0, 2);
    list.reconcile(&mut host, parent, 0.0).unwrap();
    let order: Vec<Value> = host
        .scene
        .children(parent)
        .unwrap()
        .iter()
        .map(|node| {
            list.active
                .values()
                .find(|d| d.node == *node)
                .unwrap()
                .item
                .clone()
        })
        .collect();
    assert_eq!(order, [1.0, 2.0, 0.0].map(Value::Number));
    assert!(list.reconcile(&mut host, parent, -1.0).is_err());
}

#[test]
fn rows_say_their_size_and_kind() {
    let row = Value::Map(
        [
            ("height".to_owned(), Value::Number(24.0)),
            ("kind".to_owned(), Value::String("header".into())),
        ]
        .into(),
    );
    assert_eq!(row_number(&row, "height"), Some(24.0));
    assert_eq!(row_kind(&row, Some("kind")), Some("header"));
    assert_eq!(row_kind(&row, None), None);
    let model = ListModel::new([row, Value::Number(1.0)]);
    assert_eq!(row_extents(&model, "height", 10.0), [24.0, 10.0]);
}
