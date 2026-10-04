//! packages: pacman's version order, and asking pacman, the AUR and flatpak.

use super::*;

#[test]
fn packages_compares_versions_as_pacman_does() {
    let text = run(
        r##"
        local packages = require("lib.integrations.packages")
        local v = packages.vercmp
        assert(v("1.0", "1.0") == 0)
        assert(v("1.0", "1.1") == -1 and v("1.1", "1.0") == 1)
        assert(v("1.0-2", "1.0-1") == 1)
        assert(v("1:0.1", "2.0") == 1)
        assert(v("1.0a", "1.0") == -1 and v("1.0", "1.0a") == 1)
        assert(v("1.0", "1.0.1") == -1)
        assert(v("1.10", "1.9") == 1)
        assert(v("1.0.0.r15.gabc-1", "1.0.0.r9.gdef-1") == 1)
        assert(v("2.38.1-1", "2.38.1-1") == 0)
        assert(v("0.9.9-1", "1.0.0-1") == -1)
        note("done")
        "##,
        2,
    );
    assert!(text.contains("done;"), "{text}");
}

#[test]
fn packages_asks_pacman_the_aur_and_flatpak() {
    let aur = serde_json::json!({"resultcount": 2, "type": "multiinfo", "results": [
        {"Name": "yay", "Version": "12.4.2-1"},
        {"Name": "paru", "Version": "2.0.3-1"}
    ]})
    .to_string();
    let (base, hits) = serve(vec![("/rpc/v5/info", 200, aur)]);
    let text = run(
        &format!(
            r##"
            local packages = require("lib.integrations.packages")
            local ran = {{}}
            local answers = {{
              ["/usr/bin/pacman -Q"] = {{ ok = true, code = 0, stdout = "a 1-1\nb 2-1\nyay 12.3.0-1\nparu 2.0.3-1\n", stderr = "" }},
              ["/usr/bin/checkupdates"] = {{ ok = true, code = 0,
                stdout = "linux 6.9.1.arch1-1 -> 6.9.2.arch1-1\nmesa 1:24.1.0-1 -> 1:24.1.1-1\n", stderr = "" }},
              ["/usr/bin/pacman -Qm"] = {{ ok = true, code = 0, stdout = "yay 12.3.0-1\nparu 2.0.3-1\n", stderr = "" }},
              ["/usr/bin/flatpak list --columns=application"] = {{ ok = true, code = 0, stdout = "org.a.App\norg.b.App\norg.c.App\n", stderr = "" }},
              ["/usr/bin/flatpak remote-ls --updates --columns=application,version"] = {{ ok = true, code = 0, stdout = "org.b.App\t3.1\n", stderr = "" }},
            }}
            local checker = packages.new {{
              aur_url = "{base}/rpc/v5/info",
              which = function(name) return "/usr/bin/" .. name end,
              run = function(argv, on_done)
                local key = table.concat(argv, " ")
                ran[#ran + 1] = key
                local answer = answers[key] or {{ ok = false, code = 1, stdout = "", stderr = "unknown " .. key }}
                morf.timer(1, function() on_done(answer) end, false)
              end,
            }}
            local reported = false
            ui.Text {{ text = function()
              local state = checker:get()
              if not state.checking and not reported then
                reported = true
                assert(#state.errors == 0, state.errors[1])
                assert(table.concat(state.managers, ",") == "pacman,aur,flatpak")
                assert(state.pacman.installed == 4 and state.pacman.count == 2 and state.pacman.via == "checkupdates")
                assert(state.pacman.updates[2].name == "mesa" and state.pacman.updates[2].new == "1:24.1.1-1")
                assert(state.aur.foreign == 2 and state.aur.count == 1)
                assert(state.aur.updates[1].name == "yay" and state.aur.updates[1].new == "12.4.2-1")
                assert(state.flatpak.installed == 3 and state.flatpak.count == 1)
                assert(state.flatpak.updates[1].name == "org.b.App" and state.flatpak.updates[1].new == "3.1")
                morf.timer(1, function()
                  note(state.total)
                  note(table.concat(ran, "|"))
                  note("done")
                end, false)
              end
              return ""
            end }}
            "##
        ),
        10,
    );
    assert!(text.starts_with("4;"), "{text}");
    // Only queries, and nothing through a shell.
    for command in text.split(';').nth(1).unwrap().split('|') {
        assert!(
            command == "/usr/bin/pacman -Q"
                || command == "/usr/bin/checkupdates"
                || command == "/usr/bin/pacman -Qm"
                || command.starts_with("/usr/bin/flatpak list")
                || command.starts_with("/usr/bin/flatpak remote-ls"),
            "{command}"
        );
    }
    let hits = hits.lock().unwrap().clone();
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0].contains("arg[]=yay") && hits[0].contains("arg[]=paru"),
        "{hits:?}"
    );
}

#[test]
fn packages_without_checkupdates_falls_back_to_pacman_qu() {
    let text = run(
        r##"
        local packages = require("lib.integrations.packages")
        local checker = packages.new {
          aur = false,
          which = function(name) if name == "pacman" then return "/usr/bin/pacman" end end,
          run = function(argv, on_done)
            local key = table.concat(argv, " ")
            local answer
            if key == "/usr/bin/pacman -Q" then
              answer = { ok = true, code = 0, stdout = "a 1-1\n", stderr = "" }
            elseif key == "/usr/bin/pacman -Qu" then
              -- pacman -Qu says "nothing" by exiting 1 with no output.
              answer = { ok = false, code = 1, stdout = "", stderr = "" }
            end
            morf.timer(1, function() on_done(answer) end, false)
          end,
        }
        local reported = false
        ui.Text { text = function()
          local state = checker:get()
          if not state.checking and not reported then
            reported = true
            morf.timer(1, function()
              note(state.pacman.via .. " " .. state.pacman.count .. " " .. state.total .. " " .. #state.errors
                .. " " .. tostring(state.flatpak) .. " " .. tostring(state.aur))
              note("done")
            end, false)
          end
          return ""
        end }
        "##,
        10,
    );
    assert!(text.contains("pacman -Qu 0 0 0 nil nil;done;"), "{text}");
}
