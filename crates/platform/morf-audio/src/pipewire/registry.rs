//! Following the registry: binding the globals worth watching, and
//! forgetting the ones that go.

use std::collections::HashMap;
use std::ffi::{CString, c_int, c_void};
use std::time::Duration;

use crate::Update;

use super::ffi::*;
use super::{
    Class, DEVICE_EVENTS, DeviceObject, Listener, METADATA_EVENTS, Metadata, NODE_EVENTS, Node,
    Session,
};

impl Session {
    pub(super) fn sync(&mut self) {
        // SAFETY: the core is live for the session.
        unsafe {
            if let Some((methods, object)) = methods::<CoreMethods>(self.core)
                && let Some(sync) = methods.sync
            {
                self.pending_sync = Some(sync(object, PW_ID_CORE, 0));
            }
        }
    }

    pub(super) fn done(&mut self, seq: c_int) {
        if self.pending_sync != Some(seq) {
            return;
        }
        self.pending_sync = None;
        if !self.ready {
            self.rounds += 1;
            if self.rounds < 2 {
                self.sync();
                return;
            }
            self.ready = true;
            self.events.send(Update::Available(true));
            self.report_defaults();
        }
        let ids: Vec<u32> = self.nodes.keys().copied().collect();
        for id in ids {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.settled = true;
            }
            self.report(id);
        }
    }

    /// A new global. Takes the session as a pointer, because binding hands
    /// the library pointers into it.
    ///
    /// # Safety
    /// `session` is the live session, and nothing else borrows it.
    pub(super) unsafe fn global(
        session: *mut Session,
        id: u32,
        kind: &str,
        props: HashMap<String, String>,
    ) {
        // SAFETY: promised by the caller.
        let this = unsafe { &mut *session };
        match kind {
            TYPE_NODE => {
                let Some(class) = props.get("media.class").and_then(|class| Class::of(class))
                else {
                    return;
                };
                let mut listener = Listener::new(session, id);
                // SAFETY: the registry is live; the listener is boxed.
                let proxy = unsafe {
                    this.bind(
                        id,
                        TYPE_NODE,
                        VERSION_NODE,
                        &mut listener,
                        |methods, object, hook, data| {
                            let methods = &*(methods as *const ParamMethods<NodeEvents>);
                            if let Some(add_listener) = methods.add_listener {
                                add_listener(object, hook, &NODE_EVENTS, data);
                            }
                            if let Some(subscribe) = methods.subscribe_params {
                                let mut ids = [PARAM_PROPS, PARAM_LATENCY];
                                subscribe(object, ids.as_mut_ptr(), 2);
                            }
                        },
                    )
                };
                let Some(proxy) = proxy else {
                    return;
                };
                this.nodes.insert(
                    id,
                    Node {
                        proxy,
                        listener,
                        class,
                        serial: props.get("object.serial").cloned(),
                        props,
                        own: false,
                        have_info: false,
                        settled: false,
                        volumes: Vec::new(),
                        muted: false,
                        reported: None,
                        latency: Duration::ZERO,
                    },
                );
                if this.ready && this.pending_sync.is_none() {
                    this.sync();
                }
            }
            TYPE_DEVICE => {
                if props.get("media.class").map(String::as_str) != Some("Audio/Device") {
                    return;
                }
                let mut listener = Listener::new(session, id);
                // SAFETY: as above.
                let proxy = unsafe {
                    this.bind(
                        id,
                        TYPE_DEVICE,
                        VERSION_DEVICE,
                        &mut listener,
                        |methods, object, hook, data| {
                            let methods = &*(methods as *const ParamMethods<DeviceEvents>);
                            if let Some(add_listener) = methods.add_listener {
                                add_listener(object, hook, &DEVICE_EVENTS, data);
                            }
                            if let Some(subscribe) = methods.subscribe_params {
                                let mut ids = [PARAM_ROUTE];
                                subscribe(object, ids.as_mut_ptr(), 1);
                            }
                        },
                    )
                };
                if let Some(proxy) = proxy {
                    this.devices.insert(
                        id,
                        DeviceObject {
                            proxy,
                            _listener: listener,
                            icon_name: props.get("device.icon-name").cloned(),
                            routes: Vec::new(),
                        },
                    );
                }
            }
            TYPE_LINK => {
                let node = |key: &str| props.get(key).and_then(|value| value.parse::<u32>().ok());
                if let (Some(output), Some(input)) =
                    (node("link.output.node"), node("link.input.node"))
                {
                    this.links.insert(id, (output, input));
                    this.report(output);
                    this.report(input);
                }
            }
            TYPE_METADATA => {
                if this.metadata.is_some()
                    || props.get("metadata.name").map(String::as_str) != Some("default")
                {
                    return;
                }
                let mut listener = Listener::new(session, id);
                // SAFETY: as above.
                let proxy = unsafe {
                    this.bind(
                        id,
                        TYPE_METADATA,
                        VERSION_METADATA,
                        &mut listener,
                        |methods, object, hook, data| {
                            let methods = &*(methods as *const MetadataMethods);
                            if let Some(add_listener) = methods.add_listener {
                                add_listener(object, hook, &METADATA_EVENTS, data);
                            }
                        },
                    )
                };
                if let Some(proxy) = proxy {
                    this.metadata = Some(Metadata {
                        id,
                        proxy,
                        _listener: listener,
                    });
                }
            }
            _ => {}
        }
    }

    /// Binds a global and lets `listen` attach to it.
    ///
    /// # Safety
    /// The registry must be live; `listen` receives the proxy's method table
    /// and must read it as the interface's own.
    unsafe fn bind(
        &mut self,
        id: u32,
        kind: &str,
        version: u32,
        listener: &mut Listener,
        listen: impl FnOnce(*const c_void, *mut c_void, *mut SpaHook, *mut c_void),
    ) -> Option<*mut c_void> {
        // SAFETY: promised by the caller.
        unsafe {
            let (methods, object) = methods::<RegistryMethods>(self.registry)?;
            let bind = methods.bind?;
            let kind = CString::new(kind).ok()?;
            let proxy = bind(object, id, kind.as_ptr(), version, 0);
            if proxy.is_null() {
                return None;
            }
            let interface = &*(proxy as *const SpaInterface);
            let data = (&raw mut *listener).cast::<c_void>();
            listen(
                interface.cb.funcs,
                interface.cb.data,
                &raw mut listener.hook,
                data,
            );
            Some(proxy)
        }
    }

    pub(super) fn global_remove(&mut self, id: u32) {
        if let Some(node) = self.nodes.remove(&id) {
            // SAFETY: the proxy is ours and live; destroying it unhooks the
            // listener, which is dropped after.
            unsafe { (self.pw.proxy_destroy)(node.proxy) };
            if node.reported.is_some() {
                self.events.send(if node.class.is_device() {
                    Update::DeviceRemoved(id)
                } else {
                    Update::StreamRemoved(id)
                });
            }
            drop(node.listener);
        } else if let Some(device) = self.devices.remove(&id) {
            // SAFETY: as above.
            unsafe { (self.pw.proxy_destroy)(device.proxy) };
        } else if let Some((output, input)) = self.links.remove(&id) {
            self.report(output);
            self.report(input);
        } else if self
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.id == id)
        {
            let metadata = self.metadata.take().expect("checked above");
            // SAFETY: as above.
            unsafe { (self.pw.proxy_destroy)(metadata.proxy) };
            self.default_sink = None;
            self.default_source = None;
            self.report_defaults();
        }
    }
}
