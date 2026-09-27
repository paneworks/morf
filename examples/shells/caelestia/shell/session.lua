-- The session menu: a drawer on the right edge behind the bar's power
-- button -- log out, shut down, a picture of the person, hibernate,
-- reboot -- over the desk dimmed to half. Up and Down (or Tab) move the
-- focus, Return or a click runs one, Escape or a click on the desk shuts it.
--
-- Each action is a command from the settings (`session.commands`), run
-- with `morf.run` when chosen, as the reference runs its own; nothing runs
-- on opening. CAELESTIA_DRY_RUN=1 logs the command instead of running it
-- (the tests set it).
--
-- Measured off the reference at 1920x1080: 102 wide (buttons 80 square,
-- 16 from the drawer's left, 16 apart), 495 tall, centred on the right
-- edge; the desk under it dimmed by half.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local drawer = require("drawer")

local C = theme.color
local M = {}

local BUTTON, GAP, PAD = 80, 16, 16
local WIDTH = PAD + BUTTON + 6
local ITEMS = {
  { id = "logout", icon = "logout" },
  { id = "shutdown", icon = "power_settings_new" },
  { id = "picture" },
  { id = "hibernate", icon = "downloading" },
  { id = "reboot", icon = "cached" },
}
local HEIGHT = 2 * PAD + #ITEMS * BUTTON + (#ITEMS - 1) * GAP - 1

M.focus = morf.signal("caelestia.session.focus", 1)

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end

--- The command an action runs: a list of words, `$USER` read from the
--- environment.
function M.command(id)
  local words = config.get("session.commands." .. id)
  if type(words) ~= "table" then return nil end
  local user = (morf.env and morf.env("USER")) or ""
  local out = {}
  for i, w in ipairs(words) do out[i] = (tostring(w):gsub("%$USER", user)) end
  return out
end

--- Runs action `id` (shutting the menu first).
function M.run(id)
  local argv = M.command(id)
  M.drawer.set(false)
  if not argv or #argv == 0 then return false end
  if dry_run() then
    morf.log("info", "caelestia: session " .. id .. " (dry run): " .. table.concat(argv, " "))
    return true
  end
  morf.log("info", "caelestia: session " .. id .. ": " .. table.concat(argv, " "))
  morf.run(argv, {}, function(result)
    if result and not result.ok then
      morf.log("warn", "caelestia: session " .. id .. " failed: " .. tostring(result.stderr or result.code))
    end
  end)
  return true
end

-- ------------------------------------------------------------------ build --

local function picture()
  local face = morf.fs.home() .. "/.face"
  local has_face = morf.fs.exists(face)
  return ui.Item {
    id = "session-picture",
    width = BUTTON, height = BUTTON,
    has_face and ui.Image {
      anchors = { fill = true }, source = face, fill_mode = "preserve_aspect_crop",
      mask = ui.Rect { radius = 22, color = "#ffffff" },
    } or ui.Item {
      anchors = { fill = true },
      ui.Path {
        anchors = { fill = true, margins = 4 }, view_box = { 0, 0, 100, 100 },
        d = require("lib.m3shapes").path("cookie9", { segments = false }),
        fill_color = function() return C.primaryContainer end,
      },
      kit.icon("person", 40, function() return C.onPrimaryContainer end, { anchors = { center_in = true }, fill = true }),
    },
  }
end

local buttons = {}
local order = {}
-- The buttons' backgrounds are layers of one field under them: drops that
-- bud out of the frame one after another as the menu opens, swell under
-- the pointer until they fuse with a neighbour, and morph their shape with
-- their state (M3 expressive): a rounded square at rest, a nine-point
-- cookie with the focus (turning slowly), a sunburst while pressed.
-- Soft seams only while the drops bud; at rest the buttons are crisp.
local budding = morf.signal("caelestia.session.budding", false)
local layers = {
  id = "session-field", anchors = { fill = true },
  blend = function() return budding:get() and 20 or 0 end,
  behavior = { blend = { duration = 300, easing = theme.ease.standard } },
}
local swell = kit.spring(420, 16)
for _, item in ipairs(ITEMS) do
  if item.id == "picture" then
    buttons[#buttons + 1] = picture()
  else
    order[#order + 1] = item.id
    local index = #order
    local on = function() return M.focus:get() == index end
    local area
    area = ui.MouseArea {
      id = "session-" .. item.id,
      width = BUTTON, height = BUTTON, cursor = "pointer",
      on_entered = function() M.focus:set(index) end,
      on_clicked = function() M.run(item.id) end,
      scale = function()
        if area and area.pressed then return 0.94 end
        return (area and area.hovered) and 1.14 or 1
      end,
      behavior = { scale = swell },
      stretch = kit.STRETCH,
      kit.icon(item.icon, 36, function() return on() and C.onSecondaryContainer or C.onSurface end, {
        anchors = { center_in = true },
        fill = on,
      }),
    }
    layers[#layers + 1] = kit.sdf_shape {
      id = "session-" .. item.id .. "-shape",
      track = area,
      operation = #layers == 0 and "union" or "smooth_union",
      shape = function()
        if area.pressed then return "sunny" end
        if on() then return "cookie9" end
        return "square"
      end,
      fill_color = function()
        if on() then return C.secondaryContainer end
        return area.hovered and C.surfaceContainerHigh or C.surfaceContainer
      end,
      behavior = { fill_color = { duration = theme.duration.small } },
      loop = function()
        if not (on() and M.drawer and M.drawer.open:get()) then return nil end
        return { rotation = { to = 360, duration = 12000, hold = true } }
      end,
    }
    buttons[#buttons + 1] = area
  end
end

-- Opening, each button drops out of the frame's edge in turn: from beyond
-- it, small, on the expressive spatial curve.
local settle
local function bud()
  budding:set(true)
  if settle then settle:cancel() end
  settle = morf.timer(60 + #buttons * 55 + 420, function() settle = nil budding:set(false) end, false)
  for k, node in ipairs(buttons) do
    morf.animation.play {
      {
        parallel = {
          { node = node, property = "translate_x", from = 110, to = 0, duration = 560,
            easing = theme.ease.spatial, delay = 60 + (k - 1) * 55 },
          { node = node, property = "scale", from = 0.35, to = 1, duration = 560,
            easing = theme.ease.spatial, delay = 60 + (k - 1) * 55 },
        },
      },
    }
  end
end

local keys = ui.TextInput {
  id = "session-keys",
  width = 1, height = 1, opacity = 0,
  on_escape = function() M.drawer.set(false) end,
  on_accepted = function() M.run(order[M.focus:get()]) end,
  on_key_pressed = function(_, _, _, _, key)
    local n = #order
    if key == "Up" or key == "ISO_Left_Tab" then M.focus:set((M.focus:get() - 2) % n + 1) return true end
    if key == "Down" or key == "Tab" then M.focus:set(M.focus:get() % n + 1) return true end
    return true
  end,
}

local content = ui.Item {
  anchors = { fill = true },
  ui.Sdf(layers),
  ui.Column { x = PAD, y = PAD, gap = GAP, table.unpack(buttons) },
  keys,
}

M.drawer = drawer.new {
  name = "session",
  edge = "right",
  width = WIDTH,
  height = HEIGHT,
  content = content,
}

--- The desk dimmed under the open menu; a click on it shuts the menu.
function M.dim()
  return ui.Rect {
    id = "session-dim",
    anchors = { fill = true },
    color = function() return M.drawer.open:get() and "#00000080" or "#00000000" end,
    behavior = { color = { duration = theme.duration.normal, easing = theme.ease.standard } },
    ui.MouseArea {
      anchors = { fill = true },
      visible = function() return M.drawer.open:get() end,
      on_clicked = function() M.drawer.set(false) end,
    },
  }
end

-- Opening puts the focus on the first action and takes the keyboard.
morf.effect("caelestia.session.open", function()
  local open = M.drawer.open:get()
  if open then
    bud()
    M.focus:set(1)
    keys.focus = true
  else
    keys.focus = false
  end
end)

return M
