//! Two contacts owned by their nearest accepting common ancestor. Once an
//! owner accepts, ordinary keys, clicks and single-finger pans are cancelled.
//! Lifting either finger ends once; all remaining contacts drain silently.
use morf_lua::{EventPoint, IpcValue, Runtime, UiEvent};
use morf_scene::NodeHandle;

use crate::surfaces::PointerInput;

pub struct TwoFingerPan {
    ids: [i32; 2],
    origins: [(f64, f64); 2],
    last: [(f64, f64); 2],
    owner: NodeHandle,
    ended: bool,
}

impl TwoFingerPan {
    fn args(&self, phase: &str) -> Vec<IpcValue> {
        let mut args = vec![IpcValue::String(phase.into())];
        for i in 0..2 {
            args.push(IpcValue::Number(self.last[i].0 - self.origins[i].0));
            args.push(IpcValue::Number(self.last[i].1 - self.origins[i].1));
        }
        for (x, y) in self.origins {
            args.push(IpcValue::Number(x));
            args.push(IpcValue::Number(y));
        }
        args
    }

    fn send(&self, runtime: &mut Runtime, phase: &str) -> bool {
        runtime.dispatch_gesture(self.owner, UiEvent::TwoFingerPanned, &self.args(phase))
    }
}

/// Called after inserting a contact, but before pressing its target.
pub fn down(runtime: &mut Runtime, input: &mut PointerInput) -> (bool, bool) {
    if let Some(group) = input.two_finger.as_mut() {
        let changed = !group.ended && group.send(runtime, "cancel");
        group.ended = true;
        return (true, changed);
    }
    if input.touches.len() != 2 {
        return (false, false);
    }
    let mut ids: Vec<_> = input.touches.keys().copied().collect();
    ids.sort_unstable();
    let a = input.touches[&ids[0]];
    let b = input.touches[&ids[1]];
    if a.0 != b.0 {
        return (false, false);
    }
    let mut group = TwoFingerPan {
        ids: [ids[0], ids[1]],
        origins: [input.touch_origins[&ids[0]], input.touch_origins[&ids[1]]],
        last: [(a.2, a.3), (b.2, b.3)],
        owner: a.1.node,
        ended: false,
    };
    let mut parents = Vec::new();
    let mut node = Some(b.1.node);
    while let Some(n) = node {
        parents.push(n);
        node = runtime.scene().parent(n).ok().flatten();
    }
    let mut node = Some(a.1.node);
    while let Some(n) = node {
        if parents.contains(&n)
            && runtime.offer_pan(n, UiEvent::TwoFingerPanned, &group.args("begin"))
        {
            group.owner = n;
            let mut changed = super::pan::up(runtime, input, None, None).1;
            changed |= crate::surface_gesture::finger_up(runtime, input, None);
            runtime.cancel_gesture();
            for (&id, &(_, hit, x, y, _)) in &input.touches {
                let point = EventPoint::new((x, y), (hit.local_x, hit.local_y)).with_button(0x110);
                changed |=
                    runtime.dispatch_touch_event(hit.node, UiEvent::TouchCanceled, id, point);
                changed |= runtime.dispatch_pointer(hit.node, UiEvent::Released, point, (0.0, 0.0));
            }
            input.two_finger = Some(group);
            return (true, changed);
        }
        node = runtime.scene().parent(n).ok().flatten();
    }
    (false, false)
}

pub fn motion(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    id: i32,
    x: f64,
    y: f64,
) -> (bool, bool) {
    let Some(group) = input.two_finger.as_mut() else {
        return (false, false);
    };
    if group.ended {
        return (true, false);
    }
    if let Some(i) = group.ids.iter().position(|&n| n == id) {
        group.last[i] = (x, y);
        return (true, group.send(runtime, "update"));
    }
    (true, false)
}

/// Contact is still in the map here; consume its final coordinates too.
pub fn up(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    id: i32,
    x: f64,
    y: f64,
) -> (bool, bool) {
    let Some(mut group) = input.two_finger.take() else {
        return (false, false);
    };
    let mut changed = false;
    if !group.ended {
        if let Some(i) = group.ids.iter().position(|&n| n == id) {
            group.last[i] = (x, y);
        }
        changed = group.send(runtime, "end");
        group.ended = true;
    }
    if input.touches.len() > 1 {
        input.two_finger = Some(group);
    }
    (true, changed)
}

pub fn cancel(runtime: &mut Runtime, input: &mut PointerInput) -> bool {
    input
        .two_finger
        .take()
        .is_some_and(|group| !group.ended && group.send(runtime, "cancel"))
}
