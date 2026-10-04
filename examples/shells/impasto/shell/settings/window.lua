-- The settings window: an ordinary, movable window rather than an island
-- panel, so the bar and the island stay visible and react live while their
-- own settings change.
--
-- Port of SettingsWindow.qml. It is an xdg-shell toplevel
-- (`morf.window.toplevel`), which the compositor moves, focuses and closes
-- like any other window; its title is "Settings" and its app id
-- "impasto-settings", for a window rule to float it. The page is built when
-- the window opens and let go when it closes, so pages read the state they
-- need when they are made.
--
-- `morf ipc call settings [section] [part]` opens it (on that page).

local ui = require("morf.ui")
local panel = require("settings.panel")
local theme = require("theme")
local tr = require("services.tr")

local M = {}

local shown = morf.signal("impasto.settings.shown", false)
local window

-- The page is a fixed size; a compositor that makes the window larger
-- (a tiling layout, a kiosk) gets it centred on the island's colour, and
-- the root follows the window's configured size through `on_resize`.
local frame

local function root()
  frame = ui.Rect {
    width = panel.WIDTH, height = panel.HEIGHT, color = theme.color.island,
    ui.Item {
      width = panel.WIDTH, height = panel.HEIGHT,
      anchors = { center_in = true },
      ui.Loader {
        active = function() return shown:get() end,
        source = function() return panel.build(M.close) end,
      },
    },
  }
  return frame
end

local function fit(width, height)
  frame.width = math.max(width, panel.WIDTH)
  frame.height = math.max(height, panel.HEIGHT)
end

--- The window surface, for mapping a node to window coordinates
--- (`window:item_rect(node)`), or nil before it first opens.
function M.handle() return window end

--- Where `node` is in the window, `{ x, y, width, height }`, or nil.
function M.rect_of(node)
  if not window or not node then return nil end
  local ok, rect = pcall(window.item_rect, window, node)
  if ok then return rect end
  return nil
end

--- Whether the window is up. The compositor can close it behind our back
--- (its close button, a keybind); the window's own flag says so.
function M.visible()
  if not window then return false end
  local ok, visible = pcall(window.visible, window)
  return ok and visible and shown:get()
end

function M.open(section, part)
  if section and section ~= "" then panel.go(section, part) end
  if not window then
    window = morf.window.toplevel {
      -- Mixed as Qt mixes, so translucent colours and type match the original.
      blend = require("theme").blend,
      title = tr("Settings"), app_id = "impasto-settings",
      width = panel.WIDTH, height = panel.HEIGHT,
      minimum_width = panel.WIDTH, minimum_height = panel.HEIGHT,
      maximum_width = panel.WIDTH, maximum_height = panel.HEIGHT,
      root = root(),
      visible = false,
      on_resize = fit,
      -- The compositor's close button: the page is let go, as `close` does.
      on_closed = function() shown:set(false) end,
    }
  end
  shown:set(true)
  window:open()
end

function M.close()
  shown:set(false)
  if window then window:close() end
end

--- TESTING ONLY: `morf ipc call settings_scroll <pixels>` scrolls the page
--- as the wheel would, for a headless compositor with no pointer.
morf.ipc.settings_scroll = function(pixels)
  local setting = require("components.setting")
  if setting.scroll then setting.scroll(0, tonumber(pixels) or 0) end
  return tostring(panel.state.scroll:get())
end

function M.toggle(section, part)
  if M.visible() and (not section or section == "") then M.close() else M.open(section, part) end
end

return M
