-- The lock screen: the compositor's lock, one surface per output.
--
-- Port of LockScreen.qml. Built when init.lua is started as
--
--     morf examples/impasto/init.lua -- lock
--
-- which is what `lock.lock()` in the shell runs, after it has photographed
-- the desk. `morf.surface.session_lock = true` makes morf take this file as
-- an ext-session-lock client rather than a layer: every output gets a lock
-- surface drawing this one tree, the compositor hides everything else, and
-- keeps it hidden if this process dies -- a crash leaves the session locked
-- rather than open. The lock falls when the file clears the flag, which the
-- lock service does once PAM (or a face) has said yes; the process then
-- ends.
--
-- `-- lock window` builds the same screen as an ordinary overlay layer that
-- holds nothing, which is how it is looked at without locking anything.

local lock = require("services.lock")
local surface = require("lock.surface")

local M = {}

--- Builds the lock. `options.hold` false is the look-only window;
--- `options.preview` puts it in a state for a still picture.
function M.build(options)
  options = options or {}
  local hold = options.hold ~= false
  local screen = (morf.screens or {})[1] or {}
  local W = tonumber(screen.width) or 1920
  local H = tonumber(screen.height) or 1080

  morf.surface.namespace = "impasto-lock"
  morf.surface.width = W
  morf.surface.height = H
  morf.surface.anchors = { top = true, left = true, right = true, bottom = true }
  morf.surface.layer = "overlay"
  morf.surface.keyboard_focus = "exclusive"
  morf.surface.exclusive_zone = -1
  morf.surface.session_lock = hold

  lock.begin_lock_process(hold)
  if not hold then
    -- Nothing is held, so letting go just ends the window.
    lock.on_released = function() morf.quit() end
  end
  if options.preview then lock.preview(options.preview) end

  return surface {
    width = function() return W end,
    height = function() return H end,
    screens = #(morf.screens or {}) > 0 and #morf.screens or 1,
  }
end

return M
