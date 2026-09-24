-- impasto, on morf: a Hyprland shell whose colours come from a painting.
--
-- A port of https://github.com/andreumassanet/impasto (Quickshell/QML) to
-- morf, in Lua only. The layout follows the original's folders:
--
--   theme.lua            colours, sizes, type and motion
--   services/            the state of the machine and the shell's own
--   components/          the small pieces everything is built from
--   bar/                 the bar, the island and its panels and modules
--
-- Run it:
--
--   EXAMPLE=examples/impasto/init.lua oslo make run
--
-- Every screen runs this file once. The island is live on the screen being
-- worked on; the others show it at rest.

-- `morf init.lua -- lock` is the lock screen, a process of its own under
-- ext-session-lock; `-- lock window` the same screen held by nothing, to look
-- at. The shell starts the first from `lock.lock()`. See lock/screen.lua.
local operands = morf.operands or {}
if operands[1] == "lock" then
  require("lock.screen").build { hold = operands[2] ~= "window" }
  return
end

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local bar = require("bar.bar")
local island = require("bar.island")
local clock = require("bar.modules.clock")
local kit = require("components.kit")

local C = theme.color
local screen = (morf.screens or {})[1] or {}
local SCREEN_W = tonumber(screen.width) or 1920
local SCREEN_H = tonumber(screen.height) or 1080

-- ---------------------------------------------------------------- surface --

-- One surface along the top of the screen, as tall as the largest panel
-- needs. It is transparent where nothing is drawn, and its input region is
-- only where there are MouseAreas, so windows under the empty part still
-- get their clicks. The bar's own height is reserved; the island opens over
-- the windows.
morf.surface.namespace = "impasto-bar"
morf.surface.anchors = { top = true, left = true, right = true }
morf.surface.height = math.min(SCREEN_H, 820)
morf.surface.layer = "top"
morf.surface.keyboard_focus = "none"
morf.surface.exclusive_zone = -1
morf.surface.reserve = { top = theme.bar_reserve() }
morf.surface.backdrop = false

-- A click beside an open island closes it, and so does Escape; while a panel
-- is open the keyboard is the island's.
-- The keyboard stands down while the capture surface is up, so an open panel
-- waits under it instead of taking its keys (Bar.qml's `holdsKeyboard`).
local capture_ok, capture = pcall(require, "services.capture")
morf.effect("impasto.keyboard", function()
  local open = island.state.expanded()
  local capturing = capture_ok and capture.active and capture.active() or false
  morf.surface.keyboard_focus = (open and island.state.live() and not capturing) and "exclusive" or "none"
  morf.surface.backdrop = open
end)
morf.on_backdrop_click(function() island.close() end)

-- ------------------------------------------------------------------ layers --

island.register_layer("modules", {
  size = function()
    return settings.clockShowsDate and 240 or 150, theme.capsule_height(), 0
  end,
  build = clock.build,
})

-- The control centre, until its port lands: the time and a close button,
-- enough to see the morph.
island.register("controls", {
  size = function() return 560, 420 end,
  build = function()
    return ui.Column {
      gap = 12,
      kit.text { text = clock.text, size = theme.size.widget, weight = 700 },
      kit.text { text = "Control centre", color = C.textMuted },
    }
  end,
})

-- ------------------------------------------------------------------ pieces --

local function button_piece(glyph, panel)
  return {
    build = function()
      return kit.icon_button {
        glyph = glyph, diameter = theme.capsule_height() - 6,
        color = "#00000000", hover_color = C.islandSurfaceHover,
        on_click = function() island.toggle(panel) end,
      }
    end,
  }
end

bar.register("launcher", button_piece("󰍉", "launcher"))
bar.register("overview", button_piece("󰕰", "overview"))
bar.register("notifications", button_piece("󰂚", "notifications"))
bar.register("network", button_piece("󰤨", "wifi"))
bar.register("bluetooth", button_piece("󰂯", "bluetooth"))
bar.register("volume", button_piece("󰕾", "volume"))
bar.register("battery", button_piece("󰁹", "battery"))

-- ----------------------------------------------------------------- plug-ins --

-- Every file in these folders registers itself when required: a panel with
-- `island.register`, a bar piece with `bar.register`, a layer with
-- `island.register_layer`. New parts need no line here, and a part that
-- fails to load is reported by `morf ipc call failed` instead of taking the
-- shell down with it.
local failed = {}
local root = morf.shell_dir()
for _, folder in ipairs { "services/auto", "bar/modules", "bar/pieces", "bar/layers", "bar/panels" } do
  local entries = morf.fs.list(morf.fs.join(root, folder)) or {}
  for _, entry in ipairs(entries) do
    if entry.is_file and entry.extension == "lua" then
      local name = (folder .. "/" .. entry.name:sub(1, -5)):gsub("/", ".")
      local ok, err = pcall(require, name)
      if not ok then
        failed[#failed + 1] = name .. ": " .. tostring(err)
        morf.log("error", "impasto: " .. name .. " did not load: " .. tostring(err))
      end
    end
  end
end
morf.ipc.failed = function() return table.concat(failed, "\n") end

-- -------------------------------------------------------------------- IPC --

-- `morf ipc call <panel>` toggles it, as impasto's keybinds do through qs ipc.
for _, name in ipairs { "controls", "launcher", "overview", "wifi", "bluetooth", "session", "notes", "board", "games", "keys", "stats" } do
  morf.ipc[name] = function()
    island.toggle(name)
    return island.state.open_panel()
  end
end
morf.ipc.close = function() island.close() return "" end

-- The lock, and the idle policy that locks, blanks and suspends.
local lock = require("services.lock")
require("services.idle").start()
require("services.session").watch_sleep()
morf.ipc.lock = function()
  lock.lock()
  return "locking"
end

-- The lid, from logind, on the connection the brightness already holds
-- (services/lid.lua): the laptop's own screen goes dark or lights again as
-- `lidPolicy` says.
do
  local lid = require("services.lid")
  lid.start {
    login = require("services.brightness").lib,
    on_change = function(closed) lid.apply(closed) end,
  }
end

morf.ipc.layer = function() return island.state.layer() end
-- `wallpaper` steps to the next picture; `wallpaper set <path>` (upstream's
-- form, which file managers' "Set as wallpaper" call) or `wallpaper <path>`
-- shows that one.
morf.ipc.wallpaper = function(verb, path)
  local wallpaper = require("services.wallpaper")
  if verb == "set" then
    if not path or path == "" then return "usage: morf ipc call wallpaper set <path>" end
    wallpaper.apply(path)
    return path
  end
  if verb == "next" or verb == nil or verb == "" then
    wallpaper.step(1)
  elseif verb == "previous" then
    wallpaper.step(-1)
  else
    wallpaper.apply(verb)
  end
  return wallpaper.current:get()
end

-- The brightness keys act on the focused screen (shell.qml's brightnessUp
-- and brightnessDown): `brightness up|down [step]`, `brightness set <0-100>`,
-- and with nothing the level now. Only the screen that does the single jobs
-- acts, so one key press is one step.
morf.ipc.brightness = function(verb, value)
  local brightness = require("services.brightness")
  local live = require("services.live")
  local step = tonumber(value) or 5
  if verb == "up" or verb == "down" then
    if live.here() then brightness.step(verb == "up" and step or -step) end
  elseif verb == "set" then
    local level = tonumber(value)
    if not level then return "usage: morf ipc call brightness set <0-100>" end
    if live.here() then brightness.set_percent(level) end
  elseif verb ~= nil and verb ~= "" then
    return "usage: morf ipc call brightness [up|down [step] | set <0-100>]"
  end
  return tostring(brightness.percent())
end

-- `shell reload` (what upstream's `./setup sync` calls once every file has
-- landed): the configuration read again, on every screen.
morf.ipc.shell = function(verb)
  if verb ~= "reload" then return "usage: morf ipc call shell reload" end
  morf.timer(1, function() morf.reload() end, false)
  return "reloading"
end
morf.ipc.theme = function(id)
  if id and id ~= "" then require("services.theme").set_theme(id) end
  return require("services.theme").active_id:get()
end
-- The settings window: `morf ipc call settings [section] [part]` opens it
-- (on that page), and closes it when it is up and no page is named. The
-- control centre's settings door and the bar's settings button ask through
-- `modules.request_settings`.
local settings_window = require("settings.window")
morf.ipc.settings = function(section, part)
  settings_window.toggle(section, part)
  return settings_window.visible() and require("settings.panel").section() or "closed"
end
do
  local modules = require("services.modules")
  local seen = modules.settings_requests:get()
  morf.effect("impasto.settings.requests", function()
    local asked = modules.settings_requests:get()
    if asked == seen then return end
    seen = asked
    morf.timer(1, function()
      island.close()
      settings_window.open()
    end, false)
  end)
end

-- `morf ipc call get <key>` and `morf ipc call set <key> <json>`: one
-- setting, for keybinds and scripts. The value is JSON (`true`, `5`,
-- `"dark"`); a bare word that is not JSON is taken as a string.
morf.ipc.get = function(key)
  if settings.defaults[key] == nil then return "unknown setting " .. tostring(key) end
  return morf.json.encode(settings.get(key))
end
morf.ipc.set = function(key, text)
  if settings.defaults[key] == nil then return "unknown setting " .. tostring(key) end
  local ok, value = pcall(morf.json.decode, text or "")
  if not ok then value = text end
  if not settings.accepts(key, value) then
    return "not a " .. type(settings.defaults[key]) .. ": " .. tostring(text)
  end
  settings.set(key, value)
  return morf.json.encode(settings.get(key))
end

morf.ipc.flash = function(label) island.state.flash("󰕾", label or "Volume", 0.6) return "ok" end

-- ------------------------------------------------------------------- root --

-- Testing in a nested compositor without layer-shell: the wallpaper is
-- drawn here, under the bar, and the root covers the whole screen.
local inline_wallpaper = (morf.env("IMPASTO_INLINE_WALLPAPER") or "") ~= ""
ui.Item {
  width = SCREEN_W,
  height = inline_wallpaper and SCREEN_H or morf.surface.height,
  inline_wallpaper and require("desktop.wallpaper").build(SCREEN_W, SCREEN_H) or ui.Item {},
  bar.build(SCREEN_W),
  -- The desk's arranging board and menu, which have surfaces of their own
  -- above the windows otherwise (services/auto/desktop.lua).
  inline_wallpaper and require("services.auto.desktop").inline(SCREEN_W, SCREEN_H) or ui.Item {},
}
