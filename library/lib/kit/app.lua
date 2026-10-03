-- Applications: a configuration run as a program with windows rather than
-- as a shell (`morf app <app.lua>`; Stage 16 of the plan).
--
--     local app = require("lib.kit.app")
--     local kit = app.kit()                         -- the theme, or the default look
--     app.application {
--       title = "Settings", app_id = "dev.morf.Settings",
--       width = 960, height = 640, minimum_width = 360, minimum_height = 480,
--       build = function(win) return root end,      -- the window's content
--     }
--
-- `application` opens the main window, decorated by the application itself
-- (client-side: a header bar from `lib.kit.composites.header_bar`, edges
-- that resize, a press on the header that moves), and ends the process
-- when it closes. Under `morf app` the configuration's own surface is
-- shrunk to nothing on the background layer -- the windows are the
-- application -- and only one runtime runs; under `morf check`, `render`
-- and `test` the window is an ordinary floating surface.
local morf = require("morf")
local ui = require("morf.ui")

local M = {}

--- Whether this configuration runs as an application (`morf app`).
function M.is_app() return morf.env("MORF_APP") == "1" end

--- The kit an application draws with: the configuration's own `kit`
--- module when it has one, else the default look
--- (lib.kit.skins.default), installed as `require("kit")` so composites
--- find it. `options` go to the default look's `make`.
function M.kit(options)
  local loaded = package.loaded.kit
  if type(loaded) == "table" then return loaded end
  local ok, own = pcall(require, "kit")
  if ok and type(own) == "table" then return own end
  package.loaded.kit = nil
  options = options or {}
  -- `MORF_KIT_VARIANT` (dark, light, high_contrast) picks a variant over
  -- the desktop's preference.
  if options.variant == nil then
    local asked = morf.env("MORF_KIT_VARIANT")
    if asked and asked ~= "" then options.variant = asked end
  end
  local kit = require("lib.kit.skins.default").make(options)
  package.loaded.kit = kit
  return kit
end

-- The edges a decorated window resizes from, and their cursors.
local EDGES = {
  { "top", { left = true, right = true, top = true }, "ns_resize" },
  { "bottom", { left = true, right = true, bottom = true }, "ns_resize" },
  { "left", { top = true, bottom = true, left = true }, "ew_resize" },
  { "right", { top = true, bottom = true, right = true }, "ew_resize" },
  { "top_left", { top = true, left = true }, "nwse_resize" },
  { "bottom_right", { bottom = true, right = true }, "nwse_resize" },
  { "top_right", { top = true, right = true }, "nesw_resize" },
  { "bottom_left", { bottom = true, left = true }, "nesw_resize" },
}

--- A window: `title`, `app_id`, `width`, `height`, `minimum_width`,
--- `minimum_height`, `decorated` (true: resize edges), `build(win)`
--- returning its content, `on_closed`. The content is laid out at the
--- window's size as the compositor configures it (`win.width`,
--- `win.height`). Returns the window.
function M.window(spec)
  local holder = ui.Item {}
  local win
  win = morf.window.floating {
    root = holder, title = spec.title or "morf", app_id = spec.app_id or "morf",
    width = spec.width or 800, height = spec.height or 600,
    minimum_width = spec.minimum_width, minimum_height = spec.minimum_height,
    visible = spec.visible ~= false,
    on_closed = function() if spec.on_closed then spec.on_closed() end end,
  }
  holder.width = function() return win.width end
  holder.height = function() return win.height end
  local content = spec.build and spec.build(win)
  if content then ui.reparent(content, holder) end
  if spec.decorated ~= false then
    local GRIP, CORNER = 5, 12
    for _, edge in ipairs(EDGES) do
      local name, anchors, cursor = edge[1], edge[2], edge[3]
      local corner = name:find("_") ~= nil
      local props = { anchors = anchors, z = 100, cursor = cursor,
        on_pressed = function() win:start_system_resize(name) end }
      if corner then props.width, props.height = CORNER, CORNER
      elseif name == "top" or name == "bottom" then props.height = GRIP
      else props.width = GRIP end
      ui.reparent(ui.MouseArea(props), holder)
    end
  end
  return win
end

--- The main window of an application (see the head); its closing ends
--- the process. Returns the window.
function M.application(spec)
  if M.is_app() then
    -- The configuration's own surface is no part of an application.
    morf.surface.layer = "background"
    morf.surface.width, morf.surface.height = 1, 1
    morf.surface.exclusive_zone = 0
    morf.surface.keyboard_focus = "none"
  end
  -- The configuration's own surface needs a root, though an application
  -- draws nothing there: an empty one (its windows are surfaces of their
  -- own).
  ui.Item { width = 1, height = 1 }
  local given = spec.on_closed
  local options = {}
  for k, v in pairs(spec) do options[k] = v end
  options.on_closed = function()
    if given then given() end
    if M.is_app() then morf.quit() end
  end
  return M.window(options)
end

return M
