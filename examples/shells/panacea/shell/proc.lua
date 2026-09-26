-- Child processes, as their output arrives.
--
-- `morf.run` and `morf.spawn` start a child whenever they are called and
-- call back from the main loop when it prints or exits, so there is no pool
-- to size and nothing to drain on a timer. A command that finishes calls
-- back with its stdout and whether it succeeded. Long-lived commands that
-- print a line at a time -- `pactl subscribe`, `cava` -- go through
-- `proc.stream`, which starts them again a while after they exit.
--
-- The engine keeps `LD_LIBRARY_PATH` from every child: morf may be running
-- under a nixGL-style wrapper that points it at store paths, and a system
-- binary that inherits those fails to load its own libraries.

local morf = require("morf")

local proc = {}

--- Runs `command` (a list) and calls `on_done(stdout, success)` when it
--- exits.
function proc.exec(command, on_done)
  -- A program that is not there answers through the callback too; only a
  -- refusal (too many children at once) raises.
  local ok = pcall(morf.run, command, function(result)
    if on_done then on_done(result.stdout, result.ok) end
  end)
  if not ok and on_done then on_done("", false) end
end

--- Runs a shell line through `sh -c`.
function proc.sh(line, on_done)
  proc.exec({ "sh", "-c", line }, on_done)
end

--- Keeps `command` running and calls `on_line(line)` for each line it
--- prints. Restarted a while after it exits, since the thing it watches
--- may have gone away and come back.
function proc.stream(command, on_line, options)
  options = options or {}
  local stream = { command = command, retry_ms = options.retry_ms or 5000 }
  local function start()
    stream.child = morf.spawn {
      command = command,
      on_stdout = on_line,
      on_exit = function()
        stream.child = nil
        morf.timer(stream.retry_ms, start, false)
      end,
    }
    if not stream.child then morf.timer(stream.retry_ms, start, false) end
  end
  start()
  return stream
end

--- Nothing to do any more: output is delivered as it arrives. Kept so a
--- configuration that still calls it keeps working.
function proc.tick() end

--- Splits a child's output on lines that are exactly `--`, the marker the
--- shell scripts here echo between commands. Every section is kept, an
--- empty one included: a pattern that needed a newline before the marker
--- merged an empty section with the next, and the microphone was named
--- after its own volume.
function proc.sections(output)
  local parts = { "" }
  for line in (tostring(output or "") .. "\n"):gmatch("([^\n]*)\n") do
    if line == "--" then
      parts[#parts + 1] = ""
    else
      parts[#parts] = parts[#parts] .. line .. "\n"
    end
  end
  return parts
end

--- Whitespace off both ends.
function proc.trim(text)
  return (tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

return proc
