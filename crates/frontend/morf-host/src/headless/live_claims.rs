//! A claim's condition changed over IPC moves its property on a host that
//! ticks only on the frame callbacks its own paints ask for, as a
//! compositor's do: nothing here asks the backend for a frame by hand.

use std::sync::mpsc;
use std::time::Duration;

use morf_app::Backend;
use morf_lua::{IpcValue, Screen};

use crate::headless::{FRAME, Headless, LoadOptions};
use crate::host::turn::Links;
use crate::lock::WorkerCommand;
use crate::process::supervisor::LoadPolicy;

const CONFIG: &str = r##"
  local ui = require("morf.ui")
  morf.surface.namespace = "claims-live"
  morf.surface.width = 400
  morf.surface.height = 300
  local opened = morf.signal("opened", false)
  local panel = ui.Rect { width = 100, height = 100, color = "#336699" }
  local claims = ui.claims(panel, {
    motion = { duration = 200 },
    open = { when = opened, set = { x = 200 } },
    shut = { set = { x = 0 } },
    peek = { class = "asked", set = { x = 50 } },
  })
  ui.Item { width = 400, height = 300, panel }
  morf.ipc.open = function() opened:set(true) end
  morf.ipc.close = function() opened:set(false) end
  morf.ipc.peek = function() claims.peek.ask() end
  morf.ipc.x = function() return panel.x end
"##;

fn call(
    commands: &mpsc::Sender<WorkerCommand>,
    verb: &str,
) -> mpsc::Receiver<Result<Vec<IpcValue>, String>> {
    let (reply, answer) = mpsc::sync_channel(1);
    commands
        .send(WorkerCommand::Call {
            target: verb.to_owned(),
            args: Vec::new(),
            reply,
        })
        .unwrap();
    answer
}

#[test]
fn a_condition_or_an_ask_over_ipc_drives_the_frames_that_move_it() {
    let mut options = LoadOptions::new("claims-live.lua".into());
    options.source = Some(CONFIG.as_bytes().to_vec());
    options.size = (400, 300);
    let mut headless =
        Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error));
    headless.settle(Duration::from_secs(2));
    let (tx, _supervisor) = mpsc::channel();
    let (commands_tx, commands) = mpsc::channel();
    let screen = Screen::default();
    let links = Links {
        policy: LoadPolicy::default(),
        tx: &tx,
        commands: &commands,
        screen: &screen,
    };
    let host = headless
        .host
        .as_mut()
        .expect("the configuration opens a surface");
    let mut run = |verb: &str| {
        let answer = call(&commands_tx, verb);
        for _ in 0..40 {
            host.turn(&mut headless.runtime, Some(&links)).unwrap();
            if let Some(backend) = host.backend.as_headless_mut() {
                backend.advance(FRAME);
            }
        }
        answer.try_recv().expect("answered").expect("succeeded");
        headless.runtime.call_ipc("x", &[]).unwrap()
    };
    assert_eq!(run("open"), vec![IpcValue::Number(200.0)]);
    assert_eq!(run("close"), vec![IpcValue::Number(0.0)]);
    assert_eq!(run("peek"), vec![IpcValue::Number(50.0)]);
}
