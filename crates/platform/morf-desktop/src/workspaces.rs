//! Workspaces, without knowing whose they are.
//!
//! Every compositor has these and, until this protocol, every compositor had
//! its own way of saying so: Hyprland over its own socket, sway over i3's,
//! each with a different vocabulary. A shell that wanted a workspace indicator
//! had to grow a client per compositor, and morf's own examples reached for
//! `/dispatch workspace N` because there was nothing neutral to reach for.
//!
//! `ext-workspace-v1` is the neutral answer, and binding it is how workspaces
//! arrive here without a line of per-compositor code.
//!
//! The shape is two levels: groups, which are roughly outputs, and workspaces,
//! which belong to groups. Both arrive piecemeal and neither is worth reading
//! until the manager says `done` — the same contract the window list beside
//! this one has, and for the same reason.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::globals::GlobalList;
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};

use crate::{Desktop, DesktopState};

/// `state` is a bitfield, and these are its bits.
const STATE_ACTIVE: u32 = 1;
const STATE_URGENT: u32 = 2;
const STATE_HIDDEN: u32 = 4;

/// `capabilities` is another. Activate is the bit a bar acts on; remove and
/// assign are what a workspace *manager* acts on, and a configuration is
/// entitled to know which of the three the compositor will honour.
const CAPABILITY_ACTIVATE: u32 = 1;
const CAPABILITY_REMOVE: u32 = 4;
const CAPABILITY_ASSIGN: u32 = 8;

/// One workspace, as `ext-workspace-v1` describes it.
///
/// Compositor-neutral by construction: nothing here is Hyprland's or sway's
/// vocabulary, because the protocol is what both of them speak.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceInfo {
    /// The field to act on, and the one `activate` takes.
    ///
    /// Unique, and lives exactly as long as the workspace does. Not the name --
    /// names are for people, are not unique, and change.
    pub key: String,
    /// The compositor's own cross-session id, when it offers one.
    ///
    /// Optional in the protocol and empty on compositors that send none, so it
    /// is no use as a key. What it is good for is remembering a preference
    /// against a workspace between sessions, which is exactly what the protocol
    /// says it is for.
    pub id: String,
    /// What to show a person, which is often a number.
    pub name: String,
    /// Where it sits in the compositor's arrangement, however many dimensions
    /// that has. What they mean is the compositor's business; what a shell does
    /// with them is sort by them.
    pub coordinates: Vec<u32>,
    /// The output whose group it belongs to, so a per-screen bar can show its
    /// own workspaces rather than all of them.
    pub output: String,
    pub active: bool,
    /// The workspace is asking for attention.
    pub urgent: bool,
    /// The compositor would rather it were not listed.
    pub hidden: bool,
    /// Whether `activate` will do anything. A compositor may list a workspace
    /// it will not switch to, and a bar that offers the click anyway is a bar
    /// with a dead button on it.
    pub activatable: bool,
    /// Whether `remove` will, and whether `assign` will.
    pub removable: bool,
    pub assignable: bool,
}

/// Workspaces and their groups, as the compositor last described them.
#[derive(Default)]
pub(crate) struct WorkspaceState {
    manager: Option<ExtWorkspaceManagerV1>,
    /// Every workspace the compositor reports, keyed by its protocol object.
    list: HashMap<ObjectId, WorkspaceInfo>,
    handles: HashMap<ObjectId, ExtWorkspaceHandleV1>,
    /// Which group each workspace belongs to, and which output each group is
    /// on: the two halves of "which screen is this workspace on".
    groups: HashMap<ObjectId, ObjectId>,
    group_handles: HashMap<ObjectId, ExtWorkspaceGroupHandleV1>,
    group_outputs: HashMap<ObjectId, String>,
    changed: bool,
}

impl WorkspaceState {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<DesktopState>) -> Self {
        Self {
            manager: globals.bind(qh, 1..=1, ()).ok(),
            ..Self::default()
        }
    }
}

impl Desktop {
    /// Every workspace the compositor reports, in a stable order.
    ///
    /// Sorted by coordinates and then id, because the protocol delivers them in
    /// whatever order it happens to and a bar whose workspaces reshuffle
    /// between frames is unusable.
    pub fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let mut workspaces = self.state.workspaces.list.values().cloned().collect::<Vec<_>>();
        workspaces.sort_by(|a, b| a.coordinates.cmp(&b.coordinates).then(a.key.cmp(&b.key)));
        workspaces
    }

    /// Whether the workspace list changed since this was last asked.
    pub fn take_workspaces_changed(&mut self) -> bool {
        std::mem::take(&mut self.state.workspaces.changed)
    }

    /// Switches to a workspace by its key, reporting whether it could.
    ///
    /// `false` covers three different disappointments a configuration would
    /// otherwise have to guess between: no such workspace, a compositor that
    /// will not switch to it, or no workspace protocol at all.
    pub fn activate_workspace(&mut self, key: &str) -> bool {
        let Some(manager) = &self.state.workspaces.manager else {
            return false;
        };
        let Some(key) = self
            .state
            .workspaces
            .list
            .iter()
            .find(|(_, info)| info.key == key && info.activatable)
            .map(|(key, _)| key.clone())
        else {
            return false;
        };
        let Some(handle) = self.state.workspaces.handles.get(&key) else {
            return false;
        };
        handle.activate();
        // Nothing happens until the manager is told to apply it. The protocol
        // batches, so a configuration that activated one workspace and
        // deactivated another gets both or neither.
        manager.commit();
        true
    }

    /// Removes a workspace, reporting whether the compositor will.
    pub fn remove_workspace(&mut self, key: &str) -> bool {
        let Some(manager) = &self.state.workspaces.manager else {
            return false;
        };
        let Some(handle) = self.workspace_handle(key, |info| info.removable) else {
            return false;
        };
        handle.remove();
        manager.commit();
        true
    }

    /// Moves a workspace to the group on `output`, reporting whether it could.
    pub fn assign_workspace(&mut self, key: &str, output: &str) -> bool {
        let Some(manager) = &self.state.workspaces.manager else {
            return false;
        };
        let Some(group) = self
            .state
            .workspaces
            .group_outputs
            .iter()
            .find(|(_, name)| name.as_str() == output)
            .and_then(|(id, _)| self.state.workspaces.group_handles.get(id))
            .cloned()
        else {
            return false;
        };
        let Some(handle) = self.workspace_handle(key, |info| info.assignable) else {
            return false;
        };
        handle.assign(&group);
        manager.commit();
        true
    }

    /// The handle behind a workspace key, when the compositor allows the act.
    fn workspace_handle(
        &self,
        key: &str,
        allowed: impl Fn(&WorkspaceInfo) -> bool,
    ) -> Option<&ExtWorkspaceHandleV1> {
        let id = self
            .state
            .workspaces
            .list
            .iter()
            .find(|(_, info)| info.key == key && allowed(info))
            .map(|(id, _)| id)?;
        self.state.workspaces.handles.get(id)
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for DesktopState {
    /// Groups and workspaces appearing, and the point at which they are true.
    ///
    /// Nothing is published before `done`. A workspace announces its id, name,
    /// coordinates and state on four separate events, and a configuration that
    /// read it in between would see a workspace with no name.
    fn event(
        state: &mut Self,
        _manager: &ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        match event {
            ext_workspace_manager_v1::Event::Workspace { workspace } => {
                let key = workspace.id();
                // The key a configuration acts on, decided here rather than
                // waiting for the compositor to offer one. `id` is optional in
                // the protocol -- Hyprland sends none at all -- so keying
                // activation on it would mean workspaces that cannot be
                // switched to on the compositor most likely to be running.
                // This is unique and lives exactly as long as the workspace.
                let info = WorkspaceInfo {
                    key: format!("{}", key.protocol_id()),
                    ..WorkspaceInfo::default()
                };
                state.workspaces.list.insert(key.clone(), info);
                state.workspaces.handles.insert(key, workspace);
            }
            ext_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => {
                state
                    .workspaces
                    .group_handles
                    .insert(workspace_group.id(), workspace_group);
            }
            ext_workspace_manager_v1::Event::Done => state.workspaces.changed = true,
            ext_workspace_manager_v1::Event::Finished => {
                // The compositor has stopped talking about workspaces. The
                // handles are dead, so the list has to go with them rather than
                // stand as a snapshot that will never be corrected.
                state.workspaces.list.clear();
                state.workspaces.handles.clear();
                state.workspaces.changed = true;
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(DesktopState, ExtWorkspaceManagerV1, [
        ext_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE => (ExtWorkspaceGroupHandleV1, ()),
        ext_workspace_manager_v1::EVT_WORKSPACE_OPCODE => (ExtWorkspaceHandleV1, ()),
    ]);
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for DesktopState {
    /// Which output a group is on, and which workspaces belong to it.
    ///
    /// A group is how the protocol says "these workspaces live on that screen",
    /// which is the only reason a shell cares about groups at all: it is what
    /// lets a per-output bar show its own workspaces rather than all of them.
    fn event(
        state: &mut Self,
        group: &ExtWorkspaceGroupHandleV1,
        event: ext_workspace_group_handle_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        match event {
            ext_workspace_group_handle_v1::Event::OutputEnter { output } => {
                // Told once for each binding of the output this client has
                // (the window client binds it too): only this queue's own
                // binding has a name here.
                let Some(name) = state.outputs.info(&output).and_then(|info| info.name) else {
                    return;
                };
                state.workspaces.group_outputs.insert(group.id(), name);
                relabel_group(state, group);
            }
            ext_workspace_group_handle_v1::Event::OutputLeave { .. } => {
                state.workspaces.group_outputs.remove(&group.id());
                relabel_group(state, group);
            }
            ext_workspace_group_handle_v1::Event::WorkspaceEnter { workspace } => {
                state
                    .workspaces
                    .groups
                    .insert(workspace.id(), group.id().clone());
                relabel_group(state, group);
            }
            ext_workspace_group_handle_v1::Event::WorkspaceLeave { workspace } => {
                state.workspaces.groups.remove(&workspace.id());
                if let Some(entry) = state.workspaces.list.get_mut(&workspace.id()) {
                    entry.output.clear();
                }
            }
            ext_workspace_group_handle_v1::Event::Removed => {
                state.workspaces.group_outputs.remove(&group.id());
                state.workspaces.group_handles.remove(&group.id());
                group.destroy();
            }
            _ => {}
        }
    }
}

/// Re-stamps every workspace in a group with the group's output.
///
/// The two facts arrive in either order and on different events — a workspace
/// can join a group before the group has an output, or after — so rather than
/// guess which came first, whichever arrives last recomputes from both.
fn relabel_group(state: &mut DesktopState, group: &ExtWorkspaceGroupHandleV1) {
    let output = state
        .workspaces
        .group_outputs
        .get(&group.id())
        .cloned()
        .unwrap_or_default();
    let members = state
        .workspaces
        .groups
        .iter()
        .filter(|(_, owner)| *owner == &group.id())
        .map(|(workspace, _)| workspace.clone())
        .collect::<Vec<_>>();
    for workspace in members {
        if let Some(entry) = state.workspaces.list.get_mut(&workspace) {
            entry.output = output.clone();
        }
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for DesktopState {
    /// One workspace describing itself.
    fn event(
        state: &mut Self,
        workspace: &ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        let key = workspace.id();
        match event {
            ext_workspace_handle_v1::Event::Id { id } => {
                // Optional, and not for showing: the protocol says these are
                // sent only for workspaces the compositor expects to survive a
                // session, so a configuration can remember a preference against
                // one. Never the key -- see `key` above.
                state.workspaces.list.entry(key).or_default().id = id;
            }
            ext_workspace_handle_v1::Event::Name { name } => {
                state.workspaces.list.entry(key).or_default().name = name;
            }
            ext_workspace_handle_v1::Event::Coordinates { coordinates } => {
                // Four bytes per coordinate, native-endian, however many
                // dimensions this compositor counts in. Kept as numbers because
                // what they mean is the compositor's business; what a shell
                // does with them is sort by them.
                state.workspaces.list.entry(key).or_default().coordinates = coordinates
                    .chunks_exact(4)
                    .map(|chunk| u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
            }
            ext_workspace_handle_v1::Event::State { state: bits } => {
                // A bitfield arriving as a WEnum: `into_result` fails for bit
                // combinations the generated enum has no name for, which is
                // most of them, so the raw value is what to read.
                let bits: u32 = bits.into();
                let entry = state.workspaces.list.entry(key).or_default();
                entry.active = bits & STATE_ACTIVE != 0;
                entry.urgent = bits & STATE_URGENT != 0;
                entry.hidden = bits & STATE_HIDDEN != 0;
            }
            ext_workspace_handle_v1::Event::Capabilities { capabilities } => {
                let capabilities: u32 = capabilities.into();
                let entry = state.workspaces.list.entry(key).or_default();
                entry.activatable = capabilities & CAPABILITY_ACTIVATE != 0;
                entry.removable = capabilities & CAPABILITY_REMOVE != 0;
                entry.assignable = capabilities & CAPABILITY_ASSIGN != 0;
            }
            ext_workspace_handle_v1::Event::Removed => {
                state.workspaces.list.remove(&key);
                state.workspaces.handles.remove(&key);
                state.workspaces.groups.remove(&key);
                state.workspaces.changed = true;
                workspace.destroy();
            }
            _ => {}
        }
    }
}
