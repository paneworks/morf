-- A file other screens (or people) write, followed.
--
-- Every screen's shell is a runtime of its own, so a file one of them
-- writes -- the settings, the launch history -- has to be read back by the
-- others. `morf.fs.watch` pushes the change the moment the kernel reports
-- it (the engine's one inotify thread, asleep until then), coalesced per
-- turn of the loop, so a save is one call however many writes it took.
-- Where a watch cannot be had, the file's time and size are looked at on
-- a timer instead.

local fs = morf.fs

local M = {}

-- A watch closes when its handle is collected. Callers often drop what
-- `M.file` returns, so the handles are held here until cancelled.
local live = {}

--- Calls `on_change()` after the file at `path` is written, moved into
--- place or removed. `interval` is how often the file is looked at (500
--- ms) when it cannot be watched. Returns a handle with `cancel()`.
function M.file(path, on_change, interval)
  local ok, watch = pcall(fs.watch, path, function() on_change() end)
  if ok and watch then
    live[watch] = true
    return {
      cancel = function()
        live[watch] = nil
        watch:close()
      end,
    }
  end
  local last = fs.stat(path)
  local timer = morf.timer(interval or 500, function()
    local now = fs.stat(path)
    local changed = (now and now.modified) ~= (last and last.modified)
      or (now and now.size) ~= (last and last.size)
    last = now
    if changed then on_change() end
  end, true)
  return { cancel = function() timer:cancel() end }
end

return M
