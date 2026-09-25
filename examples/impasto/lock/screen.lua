-- The lock screen: the compositor's lock, one surface per output.
--
-- Port of LockScreen.qml. Built when init.lua is started as
--
--     morf examples/impasto/init.lua -- lock
--
-- which is what `lock.lock()` in the shell runs, after it has photographed
-- the desk. `morf.surface.session_lock = true` makes morf take this file as
-- an ext-session-lock client rather than a layer: every output gets a lock
-- surface with a tree of its own (`morf.lock_surface`), over that output's
-- own desk and at its own size; the compositor hides everything else, and
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

  -- The lid, heard here too: opening it is somebody sitting down.
  pcall(function()
    require("services.lid").start { on_change = function(closed) if not closed then lock.rouse() end end }
  end)

  morf.surface.namespace = "impasto-lock"
  morf.surface.blend = require("theme").blend
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

  if hold then
    -- One tree per output, each over its own desk. Called again for an
    -- output plugged in while locked.
    morf.lock_surface(function(output)
      return surface {
        width = function() return tonumber(output.width) or W end,
        height = function() return tonumber(output.height) or H end,
        output = output.name,
      }
    end)
    return nil
  end
  return surface {
    width = function() return W end,
    height = function() return H end,
    output = screen.name,
  }
end

return M
