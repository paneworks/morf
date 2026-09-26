-- A picture on its own, in a window of the shell's: what a photo widget
-- opens at rest.
--
-- The original handed the file to imv. Here it is a floating window with
-- the picture fitted to it: the wheel zooms about the pointer, a drag pans,
-- a double click puts it back, and Escape (or the compositor's close)
-- closes it. One window, reused for the next picture.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color
local M = {}

local ESCAPE = 0xff1b

local shown = morf.signal("impasto.viewer.path", "")
local zoom = morf.signal("impasto.viewer.zoom", 1)
local pan_x = morf.signal("impasto.viewer.pan.x", 0)
local pan_y = morf.signal("impasto.viewer.pan.y", 0)
local window

local function reset()
  zoom:set(1)
  pan_x:set(0)
  pan_y:set(0)
end

local function size_for(path)
  local screen = (morf.screens or {})[1] or {}
  local sw, sh = tonumber(screen.width) or 1920, tonumber(screen.height) or 1080
  local ok, info = pcall(morf.image.info, path)
  local iw, ih = 4, 3
  if ok and info and info.width and info.height and info.width > 0 and info.height > 0 then
    iw, ih = info.width, info.height
  end
  local fit = math.min(sw * 0.8 / iw, sh * 0.8 / ih, 1)
  return math.max(320, math.floor(iw * fit + 0.5)), math.max(240, math.floor(ih * fit + 0.5))
end

local function root()
  local press_x, press_y = 0, 0
  local last_click = 0
  local area
  area = ui.MouseArea {
    anchors = { fill = true }, focus = true,
    cursor = function() return area and area.pressed and "grabbing" or "grab" end,
    on_pressed = function() press_x, press_y = pan_x:get(), pan_y:get() end,
    on_dragged = function(_, _, dx, dy)
      pan_x:set(press_x + dx)
      pan_y:set(press_y + dy)
    end,
    -- Two clicks close together: the picture fitted again.
    on_clicked = function()
      local now = morf.time.now_ms()
      if now - last_click < 350 then reset() last_click = 0 else last_click = now end
    end,
    -- Zoom about the pointer: the point under it stays under it.
    on_wheel = function(_, _, _, py, _, steps, lx, ly)
      -- Up, away from you, zooms in.
      local notches = steps ~= 0 and -steps or -(py or 0) / 120
      if notches == 0 then return end
      local z = zoom:get()
      local next_z = math.max(1, math.min(16, z * (1.2 ^ notches)))
      if next_z == z then return end
      -- The size the window was given, which a compositor may have made
      -- other than the size asked for (a tiling one, or fullscreen): the
      -- picture is centred in that.
      local w = area.layout_width or (window and window.width) or 0
      local h = area.layout_height or (window and window.height) or 0
      local px, py_ = (lx or w / 2) - w / 2, (ly or h / 2) - h / 2
      local k = next_z / z
      pan_x:set(px - k * (px - pan_x:get()))
      pan_y:set(py_ - k * (py_ - pan_y:get()))
      if next_z == 1 then pan_x:set(0) pan_y:set(0) end
      zoom:set(next_z)
    end,
    on_key_pressed = function(keysym)
      if keysym == ESCAPE then M.close() return true end
    end,
  }
  return ui.Rect {
    anchors = { fill = true }, color = C.island,
    ui.ClipRect {
      anchors = { fill = true }, color = "#00000000",
      ui.Image {
        anchors = { fill = true }, fill_mode = "preserve_aspect_fit",
        source = function() return shown:get() end,
        scale = function() return zoom:get() end,
        translate_x = function() return pan_x:get() end,
        translate_y = function() return pan_y:get() end,
        behavior = { scale = { duration = 90, easing = "out_quad" } },
      },
    },
    kit.text {
      anchors = { center_in = true }, size = theme.size.small, color = C.textMuted,
      text = "Picture not found",
      visible = function()
        return require("desktop.faces.common").lost(shown:get())
      end,
    },
    area,
  }
end

--- Shows `path` in the viewer, opening it if it is not up.
function M.open(path, title)
  if not path or path == "" then return end
  shown:set(path)
  reset()
  local w, h = size_for(path)
  if not window then
    window = morf.window.floating {
      -- Mixed as Qt mixes, so translucent colours and type match the original.
      blend = require("theme").blend,
      title = title or "Picture", app_id = "impasto-picture",
      width = w, height = h, root = root(), visible = false,
      on_closed = function() shown:set("") end,
    }
  end
  window:open()
end

function M.close()
  if window then window:close() end
end

return M
