//! claude_usage: the block and the week summed from transcripts, large files
//! read in chunks, and the limits asked for only when called.

use super::*;

/// One transcript line the way Claude Code writes it: compact JSON, the
/// usage inside the message (with the per-iteration copy after it), then the
/// request id, the type and the timestamp.
fn assistant_line(request: &str, stamp: &str, input: u64, output: u64, created: u64) -> String {
    serde_json::json!({
        "parentUuid": "p",
        "message": {
            "model": "claude", "id": format!("msg_{request}"), "type": "message", "role": "assistant",
            "content": [{"type": "text", "text": "a \"usage\":{ that is only text, and ünïcödé"}],
            "usage": {
                "input_tokens": input, "cache_creation_input_tokens": created,
                "cache_read_input_tokens": 99999, "output_tokens": output,
                "output_tokens_details": {"thinking_tokens": 1},
                "iterations": [{"input_tokens": 777777, "output_tokens": 777777}]
            }
        },
        "requestId": request,
        "type": "assistant",
        "uuid": "u",
        "timestamp": stamp
    })
    .to_string()
        + "\n"
}

fn transcripts(root: &std::path::Path) {
    let user = serde_json::json!({"type": "user", "message": {"role": "user", "content": "hi"},
        "toolUseResult": {"usage": {"input_tokens": 555555, "output_tokens": 1}}, "timestamp": "2026-01-20T12:00:00Z"})
    .to_string()
        + "\n";
    let a = [
        user.clone(),
        assistant_line("r1", "2026-01-20T10:15:00.000Z", 100, 50, 10),
        assistant_line("r2", "2026-01-20T12:05:00.000Z", 400, 100, 500),
        // The same turn again, as a second content block: counted once.
        assistant_line("r2", "2026-01-20T12:05:01.000Z", 400, 100, 500),
        user,
    ]
    .concat();
    put(root, "/projects/-home-a/one.jsonl", &a);
    let b = [
        assistant_line("r3", "2026-01-17T09:00:00.000Z", 4000, 1000, 0),
        assistant_line("r4", "2026-01-01T00:30:00.000Z", 7, 0, 0),
    ]
    .concat();
    put(root, "/projects/-home-b/two/subagents/agent-x.jsonl", &b);
}

fn usage_script(root: &std::path::Path, extra: &str) -> String {
    format!(
        r##"
        local claude_usage = require("lib.integrations.claude_usage")
        local now = claude_usage.hour_of("2026-01-20T12:00:00Z") * 3600 + 1800
        local root = "{root}"
        local function make()
          return claude_usage.new {{ dir = root .. "/projects", state_path = root .. "/state.json",
            now = function() return now end, keep_hours = 24 * 30 {extra} }}
        end
        local usage = make()
        local stage = 0
        local function report(u)
          return u.block_tokens .. " " .. u.block_messages .. " " .. u.week_tokens .. " " .. u.week_messages
            .. " " .. u.peak_block_tokens .. " " .. u.peak_week_tokens .. " " .. u.files
        end
        ui.Text {{ text = function()
          local u = usage:get()
          if u.available and not u.scanning and stage == 0 then
            stage = 1
            assert(u.block_start == claude_usage.hour_of("2026-01-20T10:00:00Z") * 3600, u.block_start)
            assert(u.block_end == u.block_start + 5 * 3600)
            local first = report(u)
            morf.timer(1, function()
              note(first)
              -- A new turn is appended; the next pass reads only it.
              assert(morf.fs.append(root .. "/projects/-home-a/one.jsonl",
                {line}))
              usage:refresh()
            end, false)
          elseif stage == 1 and u.block_tokens ~= 1160 then
            stage = 2
            local second = report(u)
            morf.timer(1, function()
              note(second)
              -- A reader that starts from the kept state counts nothing twice.
              local again = make()
              local done_again = false
              ui.Text {{ text = function()
                local v = again:get()
                if v.available and not v.scanning and not done_again then
                  done_again = true
                  morf.timer(1, function() note(report(v)) note("done") end, false)
                end
                return ""
              end }}
            end, false)
          end
          return ""
        end }}
        "##,
        root = root.display(),
        extra = extra,
        line =
            serde_json::to_string(&assistant_line("r5", "2026-01-20T12:20:00Z", 1, 0, 0)).unwrap(),
    )
}

#[test]
fn claude_usage_sums_the_block_and_the_week_incrementally() {
    let root = scratch("claude-usage");
    transcripts(&root);
    let text = run(&usage_script(&root, ""), 10);
    // block 160 + 1000, week + 5000; the peak block is the 5000 on the 17th.
    assert!(
        text.starts_with(
            "1160 2 6160 3 5000 6160 2;1161 3 6161 4 5000 6161 2;1161 3 6161 4 5000 6161 2;done;"
        ),
        "{text}"
    );
    assert!(root.join("state.json").exists());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn claude_usage_reads_large_files_in_chunks() {
    // Files over `max_read` go through `read_range`, here a stand-in for dd
    // that slices the file, in chunks smaller than a line.
    let root = scratch("claude-usage-chunks");
    transcripts(&root);
    let extra = r#", max_read = 64, chunk = 700,
        read_range = function(path, offset, length, on_done)
          local text = morf.fs.read(path)
          morf.timer(1, function() on_done(text:sub(offset + 1, offset + length)) end, false)
        end"#;
    let text = run(&usage_script(&root, extra), 20);
    assert!(
        text.starts_with(
            "1160 2 6160 3 5000 6160 2;1161 3 6161 4 5000 6161 2;1161 3 6161 4 5000 6161 2;done;"
        ),
        "{text}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn claude_usage_asks_for_the_limits_only_when_called() {
    let root = scratch("claude-limits");
    put(
        &root,
        "/.credentials.json",
        r#"{"claudeAiOauth":{"accessToken":"tok"}}"#,
    );
    let (base, hits) = serve(vec![("/v1/messages", 200, "{}".to_owned())]);
    // The server answers without the headers: the limits are unavailable, and
    // the request carried the token and the smallest model.
    let text = run(
        &format!(
            r##"
            local claude_usage = require("lib.integrations.claude_usage")
            claude_usage.limits({{ credentials = "{root}/.credentials.json", url = "{base}/v1/messages" }},
              function(result)
                note(tostring(result.available) .. " " .. tostring(result.error ~= nil))
                claude_usage.limits({{ credentials = "{root}/missing.json" }}, function(missing)
                  note(tostring(missing.available))
                  note("done")
                end)
              end)
            "##,
            root = root.display()
        ),
        10,
    );
    assert!(text.contains("false true;false;done;"), "{text}");
    let hits = hits.lock().unwrap().clone();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].contains("\"max_tokens\":1"), "{hits:?}");
    std::fs::remove_dir_all(&root).unwrap();
}
