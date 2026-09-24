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
--   day <key> [yyyy-mm-dd]  a calendar widget's day view (none: the month)
--   note_edge <key> <edge> | note_grid <note> <col> <row> | deck_edge <key> <edge>
--   deck_add <edge> | along <deck> <0..1> | takes_new <deck> <1|0> | light <edge>
--   view <photo key>       the photo in the picture viewer
--   picture <photo key> <path>  a photo widget's picture
--                          what the pointer does while arranging, for a bench
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
    -- Mapped only while the menu is open, and holding the keyboard then,
    -- so Escape closes it (the original's took none, and only a click did).
    keyboard_focus = "exclusive",
    visible = false,
    root = menu_root(WIDTH, HEIGHT),
  }
  morf.effect("impasto.desk.surfaces", function()
    if desk.editing:get() then board:open() else board:close() end
    if desk.menu_open:get() and not desk.editing:get() then menu:open() else menu:close() end
  end)
end

-- ---------------------------------------------------------- every screen --
--
-- Arranging is one mode on every screen, as the original's one `editing`
-- flag is; but each screen here is a process of its own. Each owns a name
-- on the session bus, says when it starts or ends arranging, and follows
-- what the others say. Only with more than one screen, and only while the
-- bus answers: a lone screen needs none of it.
local PATH, INTERFACE = "/io/impasto/Desk", "io.impasto.Desk"
local function bus_name(screen_entry)
  return "io.impasto.Desk.S" .. (tostring(screen_entry.name or ""):gsub("[^%w]", "_"))
end
M.shared = false
do
  local screens = morf.screens or {}
  local ok, service, outcome = false, nil, nil
  if #screens > 1 and morf.dbus and morf.dbus.serve then
    ok, service, outcome = pcall(morf.dbus.serve, "session", bus_name(screens[1]), PATH, true)
  end
  if ok and service and outcome == "owned" then
    M.shared = true
    -- What another screen said last, so it is not said back to it.
    local heard = nil
    local first = true
    morf.effect("impasto.desk.share", function()
      local on = desk.editing:get()
      if first then first = false return end
      if heard == on then heard = nil return end
      pcall(service.emit, service, PATH, INTERFACE, "Editing", { on })
    end)
    for i = 2, #screens do
      local okp, proxy = pcall(morf.dbus.proxy, "session", bus_name(screens[i]), PATH, INTERFACE)
      if okp and proxy then
        pcall(proxy.subscribe, proxy, "Editing", function(body)
          local on = type(body) == "table" and body[1] == true
          if desk.editing:get() ~= on then
            heard = on
            desk.edit(on)
          end
        end)
      end
    end
  elseif #screens > 1 then
    morf.log("warn", "impasto: arranging stays on this screen: " .. tostring(outcome or service))
  end
end

-- The keyboard going to the shell's own surface (the island, the bar) while
-- arranging is a click elsewhere: the original's focus grab ends the mode
-- then. Drawn inline, the board is that surface, and losing the keyboard is
-- the click elsewhere. A window.layer surface reports no focus of its own,
-- so a click on another application's surface is not heard.
if morf.on_keyboard_focus then
  morf.on_keyboard_focus(function(active)
    if not desk.editing:get() then return end
    if inline_mode and not active then desk.edit(false)
    elseif not inline_mode and active then desk.edit(false) end
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
  elseif verb == "day" then
    -- A calendar widget's day view, as pressing a day would open it ("" or
    -- nothing puts the month back).
    local pick = require("desktop.faces.day_tasks").by_widget[a or ""]
    if not pick then return "no calendar face for " .. tostring(a) end
    pick.pick(b or "")
    return pick.day()
  -- The drops the pointer makes while arranging, for a bench without one.
  elseif verb == "note_edge" then
    desk.note_to_edge(a or "", b or "")
    return desk.placement_of((require("desktop.sources").notes.note_for(desk.entry_of(a or "")) or {}).key or "")
  elseif verb == "note_grid" then
    return tostring(desk.note_to_grid(a or "", tonumber(b) or 0, tonumber(c) or 0))
  elseif verb == "deck_edge" then
    desk.set_deck_edge(a or "", b or "")
  elseif verb == "deck_add" then
    desk.add_deck(a or "")
  elseif verb == "along" then
    desk.set_deck_along(a or "", tonumber(b) or 0)
  elseif verb == "takes_new" then
    desk.set_takes_new(a or "", b ~= "0" and b ~= "false")
  elseif verb == "view" then
    -- A photo widget's picture in the viewer, as a click at rest opens it.
    desk.open_picture(desk.entry_of(a or ""))
    return desk.picture_of(desk.entry_of(a or ""))
  elseif verb == "picture" then
    -- A photo widget's picture, as the picker sets it.
    desk.set_picture(a or "", b or "")
    return desk.picture_of(desk.entry_of(a or ""))
  elseif verb == "light" then
    require("services.deck").receiving:set(a or "")
    return a or ""
  elseif verb == "grid" then
    local g = desk.grid()
    return g.columns .. "x" .. g.rows
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
