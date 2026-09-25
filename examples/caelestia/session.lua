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
for i, item in ipairs(ITEMS) do
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
      kit.icon(item.icon, 36, function() return on() and C.onSecondaryContainer or C.onSurface end, {
        anchors = { center_in = true },
      }),
    }
    local wash = ui.Rect {
      anchors = { fill = true }, radius = 22, z = -1,
      color = function()
        if on() then return C.secondaryContainer end
        return area.hovered and C.surfaceContainerHigh or C.surfaceContainer
      end,
      behavior = { color = { duration = theme.duration.small } },
    }
    ui.reparent(wash, area)
    buttons[#buttons + 1] = area
  end
end

local keys = ui.TextInput {
  id = "session-keys",
  width = 1, height = 1, opacity = 0,
  on_escape = function() M.drawer.set(false) end,
  on_accepted = function() M.run(order[M.focus:get()]) end,
  on_key_pressed = function(keysym)
    local n = #order
    local key = kit.is_key
    if key(keysym, "Up") or key(keysym, "ISO_Left_Tab") then M.focus:set((M.focus:get() - 2) % n + 1) return true end
    if key(keysym, "Down") or key(keysym, "Tab") then M.focus:set(M.focus:get() % n + 1) return true end
    return true
  end,
}

local content = ui.Item {
  anchors = { fill = true },
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
    M.focus:set(1)
    keys.focus = true
    morf.surface.keyboard_focus = "exclusive"
  else
    keys.focus = false
    -- The launcher takes the keyboard back when it is the one open.
    if not require("drawer").launcher or not require("drawer").launcher.open:get() then
      morf.surface.keyboard_focus = "none"
    end
  end
end)

return M
