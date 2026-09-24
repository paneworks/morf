-- Puts the desk's two surfaces above the windows: the board while arranging
-- and the right-click menu. The widgets at rest are drawn with the
-- wallpaper (desktop/wallpaper.lua), on its background layer.
--
-- Required by init.lua with everything else in services/auto. Both surfaces
-- cover the screen, ignore the other surfaces' zones and are mapped only
-- while they are in use. The arranging board holds the keyboard while it is
-- up, so Escape ends it. In a nested compositor without layer shell
-- (IMPASTO_INLINE_WALLPAPER) init.lua draws both inline instead, through
-- `M.inline`.
--
-- `morf ipc call desk <verb> [args]`:
--   edit | done            start or end arranging
--   add <module> [col row] put a widget on the board
--   remove <key>           take one off
--   select <key>           open a widget's inspector (arranging)
--   family <key> <family>  change a widget's shape
--   menu [key]             open the right-click menu (at the board's middle)
--   theme <modern|analogue>
--   list                   the rows, one per line

local ui = require("morf.ui")
local desk = require("services.desktop")
local settings = require("services.settings")

local M = {}

local screen = (morf.screens or {})[1] or {}
local WIDTH = tonumber(screen.width) or 1920
local HEIGHT = tonumber(screen.height) or 1080
local inline_mode = (morf.env and morf.env("IMPASTO_INLINE_WALLPAPER") or "") ~= ""

-- The arranging board exists only while arranging: built when it starts,
-- dropped when it ends, so an idle desk carries no handles.
local function arranging_root(width, height)
  -- A Repeater over a model of none or one row rather than a Loader: a
  -- Loader whose `active` binding flipped from an IPC handler never built
  -- its source here.
  return ui.Repeater {
    width = width, height = height,
    model = desk.board_model,
    delegate = function() return require("desktop.desk").arranging(width, height) end,
  }
end

local function menu_root(width, height)
  return require("desktop.desk").menu(width, height)
end

--- Both, for drawing inside another surface.
function M.inline(width, height)
  return ui.Item { width = width, height = height, arranging_root(width, height), menu_root(width, height) }
end

if not inline_mode then
  local board = morf.window.layer {
    namespace = "impasto-desktop",
    layer = "top",
    anchors = { top = true, bottom = true, left = true, right = true },
    width = WIDTH, height = HEIGHT,
    exclusive_zone = -1,
    keyboard_focus = "on_demand",
    visible = false,
    root = arranging_root(WIDTH, HEIGHT),
  }
  local menu = morf.window.layer {
    namespace = "impasto-desktop-menu",
    layer = "top",
    anchors = { top = true, bottom = true, left = true, right = true },
    width = WIDTH, height = HEIGHT,
    exclusive_zone = -1,
    keyboard_focus = "none",
    visible = false,
    root = menu_root(WIDTH, HEIGHT),
  }
  morf.effect("impasto.desk.surfaces", function()
    if desk.editing:get() then board:open() else board:close() end
    if desk.menu_open:get() and not desk.editing:get() then menu:open() else menu:close() end
  end)
end

-- -------------------------------------------------------------------- IPC --

morf.ipc.desk = function(verb, a, b, c)
  verb = verb or "list"
  if verb == "edit" then
    desk.edit(true)
  elseif verb == "done" then
    desk.edit(false)
  elseif verb == "add" then
    return desk.add(a or "", tonumber(b), tonumber(c))
  elseif verb == "remove" then
    desk.remove(a or "")
  elseif verb == "select" then
    if not desk.editing:get() then desk.edit(true) end
    desk.select(a or "")
    return desk.selected:get()
  elseif verb == "pick" then
    if not desk.editing:get() then desk.edit(true) end
    desk.select(a or "")
    desk.picking:set(a or "")
    return desk.picking:get()
  elseif verb == "family" then
    desk.set_family(a or "", b or "")
  elseif verb == "menu" then
    local board = desk.board()
    desk.open_menu(a or "", tonumber(b) or board.width / 2, tonumber(c) or board.height / 2)
    return "open"
  elseif verb == "theme" then
    settings.set("desktopTheme", a == "analogue" and "analogue" or "modern")
    return a or "modern"
  elseif verb == "style" then
    settings.set("desktopStyle", a or "capsule")
    return a or "capsule"
  end
  local out = {}
  for _, row in ipairs(desk.rows()) do
    local spot = (not desk.is_edge(row)) and desk.spot_of(row) or nil
    out[#out + 1] = row.key .. " " .. (row.edge and ("edge " .. row.edge)
      or (desk.family_of(row) .. " @" .. spot.col .. "," .. spot.row))
  end
  return table.concat(out, "\n")
end

return M
