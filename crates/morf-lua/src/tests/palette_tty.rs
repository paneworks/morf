//! `morf.terminal`: a terminal of the shell's own hears the colours a colour
//! tool writes to every terminal, and a file of the same sequences parses.

use std::io::Write;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn a_colour_tool_writing_to_every_terminal_reaches_the_shell() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "listen.lua",
            br#"
                heard = ""
                tty = morf.terminal.listen(function(palette, changed)
                    heard = palette.colors[2]:hex() .. " " .. palette.background:hex()
                        .. " " .. tostring(palette.colors[1]) .. " " .. #changed
                end)
                morf.ipc.path = function() return tty.path end
                morf.ipc.heard = function() return heard end
            "#,
        )
        .unwrap();
    let path = match runtime.call_ipc("path", &[]).unwrap().as_slice() {
        [IpcValue::String(path)] => path.clone(),
        other => panic!("{other:?}"),
    };
    assert!(path.starts_with("/dev/pts/"), "{path}");
    // What `colors_to_tty` does: the sequences, written to the device.
    let mut device = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    device
        .write_all(b"\x1b]4;1;#c7cf9c\x1b\\\x1b]11;#10110c\x1b\\")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        runtime.poll_services();
        if let [IpcValue::String(heard)] = runtime.call_ipc("heard", &[]).unwrap().as_slice()
            && !heard.is_empty()
        {
            assert_eq!(heard, "#c7cf9c #10110c nil 2");
            break;
        }
        assert!(Instant::now() < deadline, "nothing was heard");
        std::thread::sleep(Duration::from_millis(5));
    }
    runtime.execute("stop.lua", b"tty:stop()").unwrap();
}

#[test]
fn a_file_of_sequences_parses_to_a_palette() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "parse.lua",
            b"local p = morf.terminal.parse('\\27]4;0;#101010\\7\\27]10;#f0f0f0\\7text')
              assert(p.colors[1]:hex() == '#101010', tostring(p.colors[1]))
              assert(p.foreground:hex() == '#f0f0f0')
              assert(p.background == nil)",
        )
        .unwrap();
}
