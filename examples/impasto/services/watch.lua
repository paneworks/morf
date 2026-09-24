-- A file other screens (or people) write, followed.
--
-- Every screen's shell is a runtime of its own, so a file one of them
-- writes -- the settings, the launch history -- has to be read back by the
-- others. `morf.file(path):watch()` is inotify on the file's directory; its
-- queue is emptied from a timer (a channel read when nothing happened),
-- since the engine hands it over pulled rather than pushed. Where inotify
-- cannot be had, the file's time and size stand in.

local fs = morf.fs

local M = {}

--- Calls `on_change()` after the file at `path` is written, moved into
--- place or removed. `interval` is how often the queue is looked at (500
--- ms). Returns a handle with `cancel()`.
function M.file(path, on_change, interval)
  local watcher
  local ok, made = pcall(function() return morf.file(path):watch() end)
  if ok then watcher = made end
  local last = fs.stat(path)
  local timer = morf.timer(interval or 500, function()
    local changed = false
    if watcher then
      while true do
        local asked, event = pcall(watcher.next, watcher, 0)
        if not asked or not event then break end
        changed = true
      end
    else
      local now = fs.stat(path)
      changed = (now and now.modified) ~= (last and last.modified)
        or (now and now.size) ~= (last and last.size)
      last = now
    end
    if changed then on_change() end
  end, true)
  return {
    cancel = function()
      timer:cancel()
      watcher = nil
    end,
  }
end

return M
