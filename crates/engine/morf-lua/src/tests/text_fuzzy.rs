//! `morf.text`: fuzzy matching from Lua.

use crate::*;

fn run(source: &str) {
    let mut runtime = Runtime::default();
    if let Err(error) = runtime.execute("fuzzy.lua", source.as_bytes()) {
        panic!("{error}");
    }
}

#[test]
fn fuzzy_ranks_strings_and_reports_positions() {
    run(r##"
        local morf = require("morf")
        local hits = morf.text.fuzzy("gc", { "logic", "git commit", "nothing", "gc" })
        assert(#hits == 3, "three match, got " .. #hits)
        assert(hits[1].item == "gc" and hits[1].index == 4)
        assert(hits[2].item == "git commit" and hits[2].index == 2)
        assert(hits[3].item == "logic")
        assert(hits[1].score > hits[2].score and hits[2].score > hits[3].score)
        -- 1-based byte offsets, as string.sub counts
        local p = hits[2].positions
        assert(#p == 2 and p[1] == 1 and p[2] == 5, table.concat(p, ","))
        assert(("git commit"):sub(p[2], p[2]) == "c")

        local limited = morf.text.fuzzy("gc", { "logic", "git commit", "gc" }, { limit = 1 })
        assert(#limited == 1 and limited[1].item == "gc")

        -- smart case
        assert(#morf.text.fuzzy("FF", { "firefox", "FireFox" }) == 1)
        assert(#morf.text.fuzzy("ff", { "firefox", "FireFox" }) == 2)

        -- an empty query keeps everything in order
        local all = morf.text.fuzzy("", { "b", "a" })
        assert(#all == 2 and all[1].item == "b" and all[2].item == "a")
    "##);
}

#[test]
fn fuzzy_matches_tables_by_weighted_keys() {
    run(r##"
        local morf = require("morf")
        local apps = {
            { name = "Files", exec = "nautilus" },
            { name = "Terminal", exec = "foot" },
            { name = "Nautilus Browser", exec = "browser" },
            { exec = "nothing" },
        }
        local hits = morf.text.fuzzy("naut", apps, { key = { "name", { "exec", 0.5 } } })
        assert(#hits == 2, #hits)
        assert(hits[1].item == apps[3] and hits[1].key == "name")
        assert(hits[2].item == apps[1] and hits[2].key == "exec")
        local by_exec = morf.text.fuzzy("foot", apps, { key = "exec" })
        assert(#by_exec == 1 and by_exec[1].index == 2 and by_exec[1].key == "exec")
        assert(not pcall(morf.text.fuzzy, "x", apps))
        assert(not pcall(morf.text.fuzzy, "x", apps, { key = { { "name", -1 } } }))
        assert(not pcall(morf.text.fuzzy, "x", { 1, 2 }))
        assert(not pcall(morf.text.fuzzy, "x", {}, { limit = "many" }))
    "##);
}

#[test]
fn fuzzy_score_and_highlight() {
    run(r##"
        local morf = require("morf")
        local score, positions = morf.text.fuzzy_score("ffx", "Firefox")
        assert(score > 0)
        assert(#positions == 3 and positions[1] == 1 and positions[2] == 5 and positions[3] == 7)
        assert(morf.text.fuzzy_score("xyz", "Firefox") == nil)

        -- positions land on the first byte of each matched character
        local text = "Ünïcödé"
        local _, at = morf.text.fuzzy_score("üc", text)
        assert(at[1] == 1 and text:sub(at[2], at[2]) == "c", table.concat(at, ","))

        local spans = morf.text.highlight("Firefox", positions, { bold = true, color = "#f00" })
        assert(#spans == 5, #spans)
        assert(spans[1].text == "F" and spans[1].bold and spans[1].color == "#f00")
        assert(spans[2] == "ire")
        assert(spans[3].text == "f")
        assert(spans[4] == "o")
        assert(spans[5].text == "x")
        local wide = morf.text.highlight(text, at)
        assert(wide[1].text == "Ü" and wide[1].bold)
        assert(wide[2] == "nï" and wide[3].text == "c" and wide[4] == "ödé")
    "##);
}
