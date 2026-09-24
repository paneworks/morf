-- Puts the dock on this screen: its own layer surface on its edge, and the
-- surface its right-click menu opens in.
--
-- Required by init.lua with everything else in services/auto, before the
-- bar is built. Every screen runs the configuration once, so this is one
-- dock per screen; set to one dock (`dockEverywhere` off), only the screen
-- being worked on -- the one whose island is live -- shows it.
--
-- -- SURFACES ---------------------------------------------------------------
--
-- One per edge, made the first time that edge is chosen: a layer surface's
-- anchors are fixed once it exists, so moving the dock to another edge opens
-- that edge's surface and closes the old one. Its size follows the icon
-- size. It ignores other surfaces' zones (`exclusive_zone = -1`), so the
-- bar's reserved band does not push it inwards, and it reserves none of
-- its own.
--
-- The menu is a full-screen surface on the layer above, open only while
-- the menu is: the dock's band is too shallow for it, and a click anywhere
-- outside it has to close it.

local dock = require("services.dock")
local view = require("dock.dock")
local island = require("bar.island")

local screen = (morf.screens or {})[1] or {}
local WIDTH = tonumber(screen.width) or 1920
local HEIGHT = tonumber(screen.height) or 1080

local M = { surfaces = {} }

local function open_launcher()
  island.toggle("launcher")
end

local function anchors_for(edge)
  if edge == "left" then return { left = true, top = true, bottom = true } end
  if edge == "right" then return { right = true, top = true, bottom = true } end
  return { left = true, right = true, bottom = true }
end

local function surface_for(edge)
  local existing = M.surfaces[edge]
  if existing then return existing end
  local _, _, w, h = view.surface_box(WIDTH, HEIGHT, edge)
  local window = morf.window.layer {
    namespace = "impasto-dock",
    layer = "top",
    anchors = anchors_for(edge),
    width = w,
    height = h,
    exclusive_zone = -1,
    keyboard_focus = "none",
    visible = false,
    root = view.build(WIDTH, HEIGHT, edge, { on_launcher = open_launcher }),
  }
  M.surfaces[edge] = window
  return window
end

-- Nodes cannot be built while an effect runs (the engine holds the
-- reactive graph then), so the surface for the edge chosen now is built
-- here, and one for an edge chosen later on the next tick, from a timer.
surface_for(dock.edge())
local pending = {}

-- An edge surface: nothing without layer-shell (services/layer_shell.lua).
local layers = require("services.layer_shell").available

-- Which surface is open, and how big.
morf.effect("impasto.dock.surface", function()
  local edge = dock.edge()
  local shown = layers:get() and dock.shown(island.state.signals.active:get())
  local _, _, w, h = view.surface_box(WIDTH, HEIGHT, edge)
  for name, window in pairs(M.surfaces) do
    if name ~= edge or not shown then window:close() end
  end
  if not shown then return end
  local window = M.surfaces[edge]
  if not window then
    if not pending[edge] then
      pending[edge] = true
      morf.timer(1, function()
        pending[edge] = nil
        surface_for(edge)
        -- Opened by the effect, which the new surface's reads re-run.
        dock.signals.items:set(dock.signals.items:get() + 1)
      end, false)
    end
    return
  end
  window:size(w, h)
  window:open()
end)

-- The menu's surface: built now, mapped only while the menu is open.
local menu_window = morf.window.layer {
  namespace = "impasto-dock-menu",
  layer = "overlay",
  anchors = { top = true, bottom = true, left = true, right = true },
  width = WIDTH,
  height = HEIGHT,
  exclusive_zone = -1,
  keyboard_focus = "none",
  visible = false,
  root = view.build_menu(WIDTH, HEIGHT),
}
morf.effect("impasto.dock.menu.surface", function()
  if layers:get() and dock.menu_item() ~= nil then menu_window:open() else menu_window:close() end
end)

-- The dock reserves nothing (Dock.qml's `exclusiveZone: 0`): windows pass
-- under it, and the desktop keeps its grid clear of it on its own
-- (`dock.zone`). A reserved band would have to come and go with the dock,
-- re-tiling every window whenever a hand crossed screens.

-- `morf ipc call dock <verb> [key]`: `items` lists the keys, `menu <key>`
-- toggles an icon's menu, `hover <key>` lights one (for screenshots), `pin
-- <id>` and `unpin <id>`.
morf.ipc.dock = function(verb, key)
  verb = verb or "items"
  if verb == "menu" then
    dock.open_menu(key or "")
    return dock.signals.menu:get()
  elseif verb == "hover" then
    dock.signals.hovered:set(key or "")
    return key or ""
  elseif verb == "pin" then
    dock.pin(key)
  elseif verb == "unpin" then
    dock.unpin(key)
  end
  local keys = {}
  for _, entry in ipairs(dock.items()) do
    keys[#keys + 1] = entry.key .. (entry.running and ("*" .. #entry.windows) or "")
  end
  return table.concat(keys, " ")
end

return M
