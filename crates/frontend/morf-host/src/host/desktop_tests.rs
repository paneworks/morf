use morf_app::backend::headless::HeadlessBackend;
use morf_desktop::{
    DesktopCapabilities, DesktopRequest, HeadlessDesktop, OutputPowerMode, ToplevelAction,
    ToplevelInfo, WorkspaceInfo,
};
use morf_lua::Runtime;
use morf_value::IpcValue;

use crate::host::desktop::apply_service_requests;

fn asking() -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "ask.lua",
            br#"
            morf.workspace.activate("2")
            morf.toplevel.close("a")
            morf.output_power.set("off")
            morf.ipc.active = function()
                for _, w in ipairs(morf.workspaces) do
                    if w.active then return w.key end
                end
                return ""
            end
            morf.ipc.windows = function() return #morf.windows end
        "#,
        )
        .unwrap();
    runtime
}

fn text(runtime: &mut Runtime, verb: &str) -> IpcValue {
    runtime.call_ipc(verb, &[]).unwrap().remove(0)
}

fn workspace(key: &str, active: bool) -> WorkspaceInfo {
    WorkspaceInfo {
        key: key.to_owned(),
        output: "A".to_owned(),
        active,
        activatable: true,
        ..WorkspaceInfo::default()
    }
}

#[test]
fn a_desktop_without_the_protocols_hears_nothing_and_the_queue_drains() {
    let mut runtime = asking();
    let mut backend = HeadlessBackend::new(Vec::new());
    let mut desktop = HeadlessDesktop::new();
    apply_service_requests(&mut runtime, &mut backend, &mut desktop);
    assert!(desktop.requests().is_empty());
    assert!(runtime.take_workspace_requests().is_empty());
    assert!(runtime.take_toplevel_requests().is_empty());
    assert!(runtime.take_output_power_requests().is_empty());
    assert_eq!(
        text(&mut runtime, "active"),
        IpcValue::String(String::new())
    );
}

#[test]
fn a_desktop_with_the_protocols_is_handed_each_request() {
    let mut runtime = asking();
    let mut backend = HeadlessBackend::new(Vec::new());
    let mut desktop = HeadlessDesktop::new()
        .with_capabilities(DesktopCapabilities::ALL)
        .with_outputs(["A"])
        .with_workspaces(vec![workspace("1", true), workspace("2", false)])
        .with_toplevels(vec![ToplevelInfo {
            identifier: "a".to_owned(),
            controllable: true,
            ..ToplevelInfo::default()
        }]);
    apply_service_requests(&mut runtime, &mut backend, &mut desktop);
    assert_eq!(text(&mut runtime, "active"), IpcValue::String("1".into()));
    assert_eq!(text(&mut runtime, "windows"), IpcValue::Integer(1));
    assert_eq!(
        desktop.take_requests(),
        vec![
            DesktopRequest::OutputPower(OutputPowerMode::Off),
            DesktopRequest::ActivateWorkspace("2".into()),
            DesktopRequest::ControlToplevel {
                identifier: "a".into(),
                action: ToplevelAction::Close,
            },
        ]
    );
    apply_service_requests(&mut runtime, &mut backend, &mut desktop);
    assert_eq!(text(&mut runtime, "active"), IpcValue::String("2".into()));
    assert_eq!(text(&mut runtime, "windows"), IpcValue::Integer(0));
}
