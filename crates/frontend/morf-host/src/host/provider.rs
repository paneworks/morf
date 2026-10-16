//! Native shell connection evidence. This reports connection/configuration
//! lifetime, never Lua-declared geometry, authentication or action authority.
use crate::process::native::{Endpoint, SocketPath, birth, peer, server};
mod controls;
pub(crate) mod handoff;
mod rendered;
use std::{
    io::{self, Write},
    os::{fd::BorrowedFd, unix::net::UnixStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub struct Provider {
    _socket: SocketPath,
    configured: Arc<AtomicBool>,
    wake: UnixStream,
    thread: Option<JoinHandle<()>>,
    controls: Arc<Mutex<controls::Controls>>,
    runtime_generation: Option<u64>,
    gate: Arc<handoff::Gate>,
    rendered: Arc<Mutex<rendered::Rendered>>,
}
impl Provider {
    /// `output` must come from the actual LayerClient's own_output snapshot.
    pub fn bind(connection: BorrowedFd<'_>, output: &str) -> io::Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is unset"))?;
        Self::bind_at(Path::new(&runtime), connection, output)
    }
    fn bind_at(runtime: &Path, connection: BorrowedFd<'_>, output: &str) -> io::Result<Self> {
        let gate = handoff::Gate::open_at(runtime, output)?;
        Self::bind_using_gate(runtime, connection, output, gate)
    }
    pub(crate) fn bind_with_gate(
        connection: BorrowedFd<'_>,
        output: &str,
        gate: Arc<handoff::Gate>,
    ) -> io::Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is unset"))?;
        Self::bind_using_gate(Path::new(&runtime), connection, output, gate)
    }
    fn bind_using_gate(
        runtime: &Path,
        connection: BorrowedFd<'_>,
        output: &str,
        gate: Arc<handoff::Gate>,
    ) -> io::Result<Self> {
        if output.is_empty() || output.len() > 256 || output.chars().any(char::is_control) {
            return Err(io::Error::other("invalid native output name"));
        }
        let compositor = peer(connection)?;
        let start = birth(compositor.pid)?;
        let Endpoint {
            socket,
            listener,
            epoch,
            owner,
        } = Endpoint::bind(runtime, "shell")?;
        if compositor.uid != owner {
            return Err(io::Error::other("native shell compositor owner differs"));
        }
        let configured = Arc::new(AtomicBool::new(false));
        let ready = configured.clone();
        let output = output.to_owned();
        let controls = Arc::new(Mutex::new(controls::Controls::with_handoff(
            epoch.clone(),
            output.clone(),
            gate.clone(),
        )));
        let controller = controls.clone();
        let ui_waker = morf_io::waker();
        let persist_gate = gate.clone();
        let restore_gate = gate.clone();
        let native_work = controls.clone();
        let rendered = Arc::new(Mutex::new(rendered::Rendered::default()));
        let rendered_reader = rendered.clone();
        let work_waker = ui_waker.clone();
        let (wake, wait) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wait.set_nonblocking(true)?;
        let thread = std::thread::Builder::new()
            .name("morf-shell-provider".into())
            .spawn(move || {
                server::serve_with_work(
                    listener,
                    wait,
                    owner,
                    4096,
                    move |identity, bytes| {
                        let request: serde_json::Value = serde_json::from_slice(bytes).ok()?;
                        let response = if request["op"] == "control-capabilities"
                            && request["version"] == 1
                            && request.as_object()?.len() == 2
                        {
                            let mut capabilities = controller.lock().ok()?.capabilities();
                            capabilities["rendered_context"] = serde_json::json!(true);
                            capabilities["semantic_regions"] = serde_json::json!(true);
                            capabilities["layer_space"] = serde_json::json!(true);
                            capabilities["keyboard_registry"] = serde_json::json!(true);
                            capabilities
                        } else if request["op"] == "rendered-context"
                            && request["version"] == 1
                            && request.as_object()?.len() == 2
                        {
                            ui_waker.wake();
                            let capabilities = controller.lock().ok()?.capabilities();
                            let generation =
                                capabilities["ui_generation"].as_str()?.parse().ok()?;
                            let now = u64::try_from(controls::now()?.as_nanos()).ok()?;
                            rendered_reader.lock().ok()?.report(&epoch, generation, now)
                        } else if request["op"] == "handoff-state"
                            && request["version"] == 1
                            && request.as_object()?.len() == 2
                        {
                            ui_waker.wake();
                            controller.lock().ok()?.handoff_status(controls::now()?)
                        } else if request["op"] != "state" {
                            if !ready.load(Ordering::Acquire) || identity.uid != 0 {
                                return None;
                            }
                            let mut response = controller.lock().ok()?.request(
                                identity,
                                &request,
                                controls::now()?,
                                &controls::nonce()?,
                            )?;
                            if request["op"] == "handoff-quiesce"
                                && response["status"] == "accepted"
                                && persist_gate.persist_inhibition().is_err()
                            {
                                response = controller.lock().ok()?.handoff_inhibition_failed();
                            }
                            ui_waker.wake();
                            response
                        } else {
                            if request.as_object()?.len() != 2
                                || request["version"] != 1
                                || request["op"] != "state"
                            {
                                return None;
                            }
                            let configured = ready.load(Ordering::Acquire);
                            serde_json::json!({
                                "version":1,"role":"shell","epoch":epoch,
                                "revision":if configured {2} else {1},
                                "phase":if configured {"configured"} else {"pending"},
                                "compositor_pid":compositor.pid,"compositor_start":start,
                                "compositor_uid":compositor.uid,"output":output,
                            })
                        };
                        let mut response = serde_json::to_vec(&response).ok()?;
                        response.push(b'\n');
                        Some(response)
                    },
                    move || {
                        let ticket = native_work
                            .lock()
                            .ok()
                            .and_then(|mut state| state.handoff_take_restore());
                        if let Some(token) = ticket {
                            let success = restore_gate.remove_inhibition().is_ok();
                            if let Ok(mut state) = native_work.lock() {
                                state.handoff_prepared_restore(&token, success);
                            }
                            work_waker.wake();
                        }
                    },
                );
            })?;
        Ok(Self {
            _socket: socket,
            configured,
            wake,
            thread: Some(thread),
            controls,
            runtime_generation: None,
            gate,
            rendered,
        })
    }
    /// Native host startup completed its first configured surface/frame.
    /// This is not compositor presentation acknowledgement or region evidence.
    pub fn configured(&self) {
        self.configured.store(true, Ordering::Release);
    }

    pub(crate) fn begin_turn(&self) {
        if let Ok(mut rendered) = self.rendered.lock() {
            rendered.busy();
        }
    }
    pub(crate) fn observe_rendered(
        &self,
        host: &crate::host::turn::Host,
        runtime: &morf_lua::Runtime,
    ) {
        let generation = self.controls.lock().ok().and_then(|controls| {
            controls.capabilities()["ui_generation"]
                .as_str()?
                .parse()
                .ok()
        });
        if let Some(generation) = generation
            && let Ok(mut rendered) = self.rendered.lock()
        {
            rendered.observe(host, runtime, generation);
        }
    }

    /// Runs at most one bounded request on the UI thread. A dispatched receipt
    /// records controller acceptance, not animation or compositor completion.
    pub fn dispatch(&mut self, runtime: &mut morf_lua::Runtime, generation: u64) -> bool {
        self.dispatch_at_boundary(
            runtime,
            generation,
            handoff::Boundary {
                contacts: 0,
                neutral: false,
                revision: 0,
            },
        )
    }
    pub(crate) fn dispatch_at_boundary(
        &mut self,
        runtime: &mut morf_lua::Runtime,
        generation: u64,
        boundary: handoff::Boundary,
    ) -> bool {
        if self.runtime_generation != Some(generation) {
            if self.runtime_generation.is_some() {
                self.controls.lock().unwrap().reset_ui();
            }
            let controls = self.controls.clone();
            if runtime
                .set_controller_authorizer(Arc::new(move |token| {
                    controls::now().is_some_and(|at| {
                        controls
                            .lock()
                            .is_ok_and(|state| state.valid_for_effect(token, at))
                    })
                }))
                .is_err()
            {
                self.controls.lock().unwrap().close();
                return false;
            }
            self.runtime_generation = Some(generation);
            let retirement = self.controls.clone();
            runtime.set_native_policy_retirement(Arc::new(move || {
                if let Ok(mut state) = retirement.lock() {
                    state.retire_for_reload();
                }
            }));
            let gate = self.gate.clone();
            let controls = self.controls.clone();
            if runtime
                .set_recognizer_gate(Arc::new(move || gate.enabled()))
                .is_err()
                || runtime
                    .set_handoff_authorizer(Arc::new(move |token, enabled| {
                        controls
                            .lock()
                            .is_ok_and(|state| state.handoff_authorizes(token, enabled))
                    }))
                    .is_err()
            {
                self.controls.lock().unwrap().close();
                return false;
            }
        }
        let Some(at) = controls::now() else {
            return false;
        };
        let handoff_work = {
            let mut state = self.controls.lock().unwrap();
            state.handoff_observe(boundary, at);
            state.handoff_work(at)
        };
        let mut handoff_changed = false;
        if let Some((token, enabled)) = handoff_work {
            let ack = runtime
                .call_ipc(
                    "prek-native-handoff",
                    &[
                        morf_lua::IpcValue::String(token.clone()),
                        morf_lua::IpcValue::Boolean(enabled),
                    ],
                )
                .ok()
                .and_then(|values| values.first().map(|value| value.to_json()));
            if let Some(at) = controls::now() {
                let mut ready = ack
                    .as_ref()
                    .is_some_and(|value| value["ready"] == true && value["enabled"] == enabled);
                let driver = ack
                    .as_ref()
                    .and_then(|value| value["driver"].as_str())
                    .filter(|v| matches!(*v, "native" | "lisgd"))
                    .unwrap_or("unavailable");
                ready &= enabled || driver == "native";
                handoff_changed = self
                    .controls
                    .lock()
                    .unwrap()
                    .handoff_update(&token, boundary, ready, driver, at);
            }
        }
        if self.controls.lock().unwrap().handoff_restore_ready() {
            let _ = self.wake.write_all(&[2]);
        }
        let refreshed = runtime
            .call_ipc("prek-native-refresh", &[])
            .ok()
            .is_some_and(|values| values.first().is_some_and(|value| value.to_json() == true));
        // Refresh can execute Lua; sample expiry after it, immediately before
        // handing the next job to its controller.
        let Some(at) = controls::now() else {
            return refreshed || handoff_changed;
        };
        let job = self.controls.lock().unwrap().take(at);
        let Some(job) = job else {
            return refreshed || handoff_changed;
        };
        let accepted = runtime
            .call_ipc(
                "prek-native-control",
                &[
                    morf_lua::IpcValue::String(job.payload.clone()),
                    morf_lua::IpcValue::String(job.token.clone()),
                ],
            )
            .ok()
            .is_some_and(|values| {
                values
                    .first()
                    .is_some_and(|value| value.to_json()["status"] == "accepted")
            });
        self.controls.lock().unwrap().complete(&job, accepted);
        // Keep draining this bounded queue without depending on another socket wake.
        true
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        if let Ok(mut state) = self.controls.lock() {
            state.close();
        }
        let _ = self.wake.write_all(&[1]);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{BufRead, BufReader},
        os::{
            fd::AsFd,
            unix::fs::{DirBuilderExt, MetadataExt},
        },
        time::Duration,
    };

    #[test]
    fn shell_endpoint_uses_actual_socket_credentials_and_only_native_configuration() {
        let path = std::path::PathBuf::from(format!("/tmp/msp-{}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let (connection, _compositor) = UnixStream::pair().unwrap();
        let provider = Provider::bind_at(&path, connection.as_fd(), "HEADLESS-1").unwrap();
        let socket = provider._socket.path.clone();
        assert_eq!(fs::metadata(&socket).unwrap().mode() & 0o777, 0o600);
        let mut client = BufReader::new(UnixStream::connect(&socket).unwrap());
        client
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let query = |client: &mut BufReader<UnixStream>| {
            client
                .get_mut()
                .write_all(b"{\"version\":1,\"op\":\"state\"}\n")
                .unwrap();
            let mut response = String::new();
            client.read_line(&mut response).unwrap();
            serde_json::from_str::<serde_json::Value>(&response).unwrap()
        };
        let pending = query(&mut client);
        assert_eq!(pending["phase"], "pending");
        assert_eq!(pending["role"], "shell");
        assert_eq!(pending["compositor_pid"], std::process::id());
        assert_eq!(
            pending["compositor_start"],
            birth(std::process::id()).unwrap()
        );
        assert_eq!(pending["output"], "HEADLESS-1");
        provider.configured();
        let ready = query(&mut client);
        assert_eq!(ready["phase"], "configured");
        assert_eq!(ready["revision"], 2);
        assert_eq!(ready["epoch"], pending["epoch"]);
        client
            .get_mut()
            .write_all(b"{\"version\":1,\"op\":\"control-capabilities\"}\n")
            .unwrap();
        let mut response = String::new();
        client.read_line(&mut response).unwrap();
        let capabilities: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(capabilities["epoch"], ready["epoch"]);
        assert_eq!(capabilities["ui_generation"], "1");
        assert_eq!(capabilities["root_only"], true);
        assert_eq!(capabilities["recognizer_ownership"], false);
        assert_eq!(capabilities["rendered_context"], true);
        client
            .get_mut()
            .write_all(b"{\"version\":1,\"op\":\"rendered-context\"}\n")
            .unwrap();
        let mut rendered = String::new();
        client.read_line(&mut rendered).unwrap();
        let rendered: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(rendered["epoch"], ready["epoch"]);
        assert_eq!(rendered["ui_generation"], "1");
        assert!(
            rendered["frame"].is_null(),
            "a connection/configure alone is not a GPU frame"
        );
        if unsafe { libc::geteuid() } != 0 {
            // The kernel supplies our actual unprivileged identity. A JSON
            // credential cannot grant authority or change the native state.
            let mut attacker = UnixStream::connect(&socket).unwrap();
            attacker
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            writeln!(
                attacker,
                "{}",
                serde_json::json!({"version":1,"op":"lease","uid":0,"epoch":ready["epoch"]})
            )
            .unwrap();
            use std::io::Read;
            assert_eq!(attacker.read(&mut [0u8; 1]).unwrap(), 0);
            assert_eq!(query(&mut client), ready);
        }
        client
            .get_mut()
            .write_all(b"{\"version\":1,\"op\":\"state\",\"phase\":\"configured\"}\n")
            .unwrap();
        let mut response = String::new();
        assert_eq!(client.read_line(&mut response).unwrap(), 0);
        drop(provider);
        assert!(!socket.exists());
        fs::remove_dir_all(path).unwrap();
    }
}
