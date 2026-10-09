-- The on-screen keyboard: a drawer of its own along the bottom edge --
-- beside the capture drawer, not in it -- carrying lib.board's keys, which
-- type into whatever has the focus through a virtual keyboard. The drawer
-- never takes the focus itself, so the text goes where it was going.
--
-- It comes up by itself when a program asks for text (a text field is
-- focused: the input-method protocol says so) and no real keyboard is
-- attached (lib.keyboards), and goes when the field does -- unless it was
-- opened by hand. By hand: `keyboard [toggle|open|close]` over IPC, which a
-- key can be bound to. `keyboard.auto = false` in the settings stops the
-- coming up by itself. Hiding it by hand also suppresses automatic opening
-- for that application until it is opened by hand again, for this login.

local morf = require("morf")
local config = require("config")
local osk = require("lib.util.osk")
local keyboards = require("lib.services.keyboards")
local hyprland = require("lib.integrations.hyprland")
local M = { opened = morf.signal("caelestia.keyboard.shown", false) }

-- Application IDs, not window addresses or titles: two Kitty windows share
-- the choice, while Firefox keeps its own. An unknown app never blocks all
-- apps. The compositor's session ID keeps an old choice out of a new login.
local function application()
  local name = hyprland.state.active_window.class or ""
  name = name:match("^%s*(.-)%s*$"):lower()
  return name ~= "" and name or nil
end
local runtime = morf.env("XDG_RUNTIME_DIR")
local session = morf.env("HYPRLAND_INSTANCE_SIGNATURE")
if not session or session == "" then session = morf.env("XDG_SESSION_ID") end
if not session or session == "" then session = morf.env("WAYLAND_DISPLAY") end
local preferences
local hidden = morf.signal("caelestia.keyboard.hidden_apps", {})
if runtime and runtime:sub(1, 1) == "/" and session and session ~= "" then
  preferences = require("lib.util.settings").open {
    name = "caelestia.keyboard.session",
    path = runtime .. "/morf/keyboard-hidden-" .. session:gsub("[^%w_.-]", "_") .. ".json",
    defaults = { hidden = {} },
  }
end
local function hidden_apps() return preferences and preferences.get("hidden") or hidden:get() end
local function suppressed(app)
  if not app then return false end
  for _, name in ipairs(hidden_apps()) do if name == app then return true end end
  return false
end
local function remember(on)
  local app = application()
  if not app or suppressed(app) == (not on) then return end
  local apps = {}
  for _, name in ipairs(hidden_apps()) do if name ~= app then apps[#apps + 1] = name end end
  if not on then apps[#apps + 1] = app end
  if preferences then
    preferences.set("hidden", apps)
    preferences.flush()
  else hidden:set(apps) end
end

local field = false
local send = osk.sender { ime = function() return field end }
function M.active() return M.opened:get() end
function M.send(event)
  if not M.active() then return end
  local dry = morf.env("CAELESTIA_DRY_RUN")
  if dry and dry ~= "" and dry ~= "0" then return end
  send(event)
end
function M.close() M.set(false) end
function M.desk_size()
  local _, _, w, h = require("bar").desk()
  return w, h
end

local view = require("themes").view("keyboard").build(M)
M.keys, M.WIDTH = view.keys, view.width
M.drawer = require("drawer").new {
  name = "keyboard", edge = view.edge or "bottom", width = view.width,
  height = view.height, content = view.content, props = view.props,
}
-- It belongs to the physical bottom edge, outside the shrinking desktop.
M.drawer.docked = true
morf.effect("caelestia.keyboard.reserve",function()
  require("themes.keyboard").inset:set(M.drawer.open:get()
    and math.ceil(view.height()+require("theme").BORDER) or 0)
end)

local asked = false
function M.set(on)
  asked = false
  remember(on)
  M.drawer.set(on)
end
function M.toggle() M.set(not M.drawer.open:get()) end
function M.show(mode)
  local valid = mode == nil
  for _, name in ipairs(osk.MODES) do if mode == name then valid = true end end
  if not valid then return false end
  asked = false
  if mode then M.keys.mode:set(mode) end
  M.keys.reset()
  M.set(true)
  return true
end

local function auto_show()
  if suppressed(application()) then
    asked = false
    M.drawer.set(false)
    return
  end
  if config.get("keyboard.auto") == false then return end
  if field then
    if not M.drawer.open:get() and not keyboards.attached() then
      asked = true
      M.keys.reset()
      M.drawer.set(true)
    end
  elseif asked then
    asked = false
    M.drawer.set(false)
  end
end
if morf.input_method and morf.input_method.subscribe then
  pcall(morf.input_method.subscribe, function(active)
    field = active == true
    auto_show()
  end)
end
-- A text-capable app can replace another without an IME inactive/active
-- pair. Recheck on app changes too, including a keyboard left open in Firefox
-- when returning to Kitty. Automatic closes never become a user preference.
local previous_app, previous_hidden
morf.effect("caelestia.keyboard.application", function()
  local app = application()
  local blocked = suppressed(app)
  if app == previous_app and blocked == previous_hidden then return end
  previous_app, previous_hidden = app, blocked
  auto_show()
end)
morf.effect("caelestia.keyboard.hand", function()
  local on = M.drawer.open:get()
  M.opened:set(on)
  if not on then asked = false end
  if view.shown then view.shown(on) end
end)
return M
