-- Capture, recording and the colour picker, wired to the rest of the shell:
-- their surfaces, the control centre's tiles, the bar's capture button and
-- the verbs a key binding (or a test bench) calls.
--
--   morf ipc call capture [region|window|screen] [photo|video] [file|clipboard|editor|text]
--   morf ipc call capture.select <x> <y> <w> <h>   -- a drag, without a pointer
--   morf ipc call capture.take | capture.cancel | capture.last
--   morf ipc call record [toggle|start|stop|status]
--   morf ipc call picker                           -- the lens; again lets it go
--   morf ipc call picker.hover <x> <y> | picker.take <x> <y> | picker.last
--
-- The shortcuts in the original (capture a region, a window, the screen,
-- annotate, read text, record, pick) are these verbs with arguments.
--
-- A verb reaches every screen's shell. Only the screen being worked on acts
-- on `capture`, `record` and `picker` (services/live.lua), so a key is one
-- surface, one take and one lens, not one per screen; the others answer
-- "elsewhere". A take is seen by every screen, so `record stop` works
-- from any.

local theme = require("theme")
local controls = require("services.controls")
local island_state = require("bar.island_state")
local capture = require("services.capture")
local recorder = require("services.recorder")
local picker = require("services.picker")
local overlay = require("capture.overlay")
local lens = require("capture.picker")
local live = require("services.live")

-- A panel closing first must be gone from the photograph.
local function settle()
  return island_state.expanded() and theme.duration_island_gone() or 1
end

-- ------------------------------------------------------------------ tiles --

controls.define_tile("screenshot", {
  available = function() return morf.screencopy ~= nil end,
  active = function() return capture.active() end,
  action = function() capture.open("", "", "", theme.duration_island_gone()) end,
})
controls.define_tile("annotate", {
  available = function() return capture.can("editor") end,
  action = function() capture.open("region", "photo", "editor", theme.duration_island_gone()) end,
})
controls.define_tile("text", {
  available = function() return capture.can("text") end,
  action = function() capture.open("region", "photo", "text", theme.duration_island_gone()) end,
})
controls.define_tile("picker", {
  icon = function() return picker.icon end,
  detail = function() return picker.last() ~= "" and picker.last() or "A pixel" end,
  available = function() return picker.available() end,
  active = function() return picker.picking() end,
  action = function() picker.pick(theme.duration_island_gone()) end,
})
controls.define_tile("record", {
  icon = function() return recorder.recording() and "󰑊" or "󰕧" end,
  detail = function()
    if recorder.recording() then return "Recording  " .. recorder.display() end
    return recorder.available() and recorder.subject() or "No recorder installed"
  end,
  -- A dry run can pretend without an encoder.
  available = function() return recorder.available() or require("services.act").dry end,
  active = function() return recorder.recording() end,
  -- Stopping needs no pause; starting waits for the island to leave the picture.
  action = function()
    if recorder.recording() then recorder.stop() return end
    morf.timer(theme.duration_island_gone(), function() recorder.start("screen", nil) end, false)
  end,
})

-- The bar's capture button asks for a "capture" panel; the capture surface
-- is not a panel, so the request becomes an opening.
morf.effect("impasto.capture.button", function()
  if island_state.open_panel() ~= "capture" then return end
  morf.timer(1, function()
    island_state.close()
    capture.open("", "", "", theme.duration_island_gone())
  end, false)
end)

-- --------------------------------------------------------------------- IPC --

local SHAPES = { region = true, window = true, screen = true }
local KINDS = { photo = true, video = true }
local TO = { file = true, clipboard = true, editor = true, text = true }

morf.ipc.capture = function(...)
  local shape, kind, to = "", "", ""
  for _, word in ipairs { ... } do
    if SHAPES[word] then shape = word elseif KINDS[word] then kind = word elseif TO[word] then to = word end
  end
  if capture.active() then return "open" end
  if not live.here() then return "elsewhere" end
  local wait = settle()
  if island_state.expanded() then island_state.close() end
  return capture.open(shape, kind, to, wait) and "opening" or "busy"
end

morf.ipc["capture.select"] = function(x, y, w, h)
  if not capture.active() then return "not open" end
  local sel = overlay.selection
  sel.x, sel.y, sel.w, sel.h = tonumber(x) or 0, tonumber(y) or 0, tonumber(w) or 0, tonumber(h) or 0
  sel.px, sel.py = sel.x + sel.w / 2, sel.y + sel.h / 2
  return ("%d,%d %dx%d"):format(sel.x, sel.y, sel.w, sel.h)
end
morf.ipc["capture.take"] = function()
  if not capture.active() then return "not open" end
  overlay.take()
  return "taken"
end
morf.ipc["capture.cancel"] = function() capture.cancel() return "cancelled" end
morf.ipc["capture.last"] = function() return capture.signals.last:get() end
morf.ipc["capture.tools"] = function()
  return ("editor=%s text=%s directory=%s recorder=%s audio=%s"):format(
    tostring(capture.can("editor")), tostring(capture.can("text")), capture.directory(),
    recorder.tool ~= "" and recorder.tool or "none", tostring(recorder.can_audio()))
end

morf.ipc.record = function(verb)
  verb = verb or "toggle"
  if verb ~= "status" and not live.here() then return "elsewhere" end
  if verb == "start" then recorder.start("screen", nil)
  elseif verb == "stop" then recorder.stop()
  elseif verb == "toggle" then
    island_state.close()
    recorder.toggle()
  end
  return (recorder.recording() and ("recording " .. recorder.display() .. " " .. recorder.path()) or "idle")
end

morf.ipc.picker = function()
  -- The lens up here is let go by the same key, wherever the pointer is.
  if not picker.active() and not live.here() then return "elsewhere" end
  local wait = settle()
  if island_state.expanded() then island_state.close() end
  picker.pick(wait)
  return "picking"
end
morf.ipc["picker.hover"] = function(x, y)
  if not picker.active() then return "not open" end
  lens.hover_at(tonumber(x) or 0, tonumber(y) or 0)
  return "ok"
end
morf.ipc["picker.take"] = function(x, y)
  if not picker.active() then return "not open" end
  picker.take(tonumber(x) or 0, tonumber(y) or 0)
  return "taken"
end
morf.ipc["picker.last"] = function() return picker.last() end
