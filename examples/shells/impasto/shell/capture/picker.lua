-- The colour picker's surface: the screen as it was when the key was
-- pressed, a magnifier under the pointer and the colour it is over. A
-- click takes it, Escape or a right click lets it go.
--
-- The lens that hyprpicker drew for the original, drawn by the shell over
-- a still picture (services/picker.lua) instead.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local picker = require("services.picker")

local C = theme.color
local M = {}

local KEY = { ESCAPE = 0xff1b, RETURN = 0xff0d, KP_ENTER = 0xff8d }

local screen = (morf.screens or {})[1] or {}
local W = tonumber(screen.width) or 1920
local H = tonumber(screen.height) or 1080

local LENS = 132
local ZOOM = 8

local at = morf.state { x = W / 2, y = H / 2, seen = false }
M.pointer = at

local function build()
  local half = LENS / 2
  -- The lens sits below and right of the pointer, and flips near an edge.
  local lens_x = function()
    local x = at.x + 28
    if x + LENS > W then x = at.x - 28 - LENS end
    return x
  end
  local lens_y = function()
    local y = at.y + 28
    if y + LENS + 40 > H then y = at.y - 28 - LENS - 40 end
    return y
  end
  local reading = ui.Row {
    anchors = { center_in = true }, gap = 6, align = "center",
    ui.Rect {
      width = 12, height = 12, radius = 6,
      color = function() return picker.hover() ~= "" and picker.hover() or "#00000000" end,
      border_width = 1, border_color = C.islandBorder,
    },
    kit.text {
      text = function() return picker.hover() ~= "" and picker.hover() or "…" end,
      mono = true, size = theme.size.small, weight = 600,
    },
  }
  return ui.Item {
    width = W, height = H,
    ui.Rect { width = W, height = H, color = "#000000" },
    ui.Image { x = 0, y = 0, width = W, height = H, fill_mode = "stretch", source = picker.photo },
    ui.Item {
      x = lens_x, y = lens_y, width = LENS, height = LENS + 40,
      visible = function() return at.seen end,
      ui.ClipRect {
        x = 0, y = 0, width = LENS, height = LENS, radius = half,
        color = "#000000", border_width = 3, border_color = C.island,
        -- The picture at its own size, scaled by a transform from its
        -- corner: a texture eight times the screen would cost a frame.
        ui.Image {
          x = 0, y = 0, width = W, height = H, fill_mode = "stretch",
          transform_origin_x = 0, transform_origin_y = 0, scale = ZOOM,
          translate_x = function() return half - at.x * ZOOM - ZOOM / 2 end,
          translate_y = function() return half - at.y * ZOOM - ZOOM / 2 end,
          source = picker.photo,
        },
        -- The pixel being read.
        ui.Rect {
          x = half - ZOOM / 2 - 1, y = half - ZOOM / 2 - 1, width = ZOOM + 2, height = ZOOM + 2,
          color = "#00000000", border_width = 1, border_color = "#ffffff",
        },
      },
      ui.Rect {
        x = 0, y = 0, width = LENS, height = LENS, radius = half,
        color = "#00000000", border_width = 2,
        border_color = function() return picker.hover() ~= "" and picker.hover() or C.islandBorder end,
      },
      ui.Rect {
        anchors = { horizontal_center = true },
        y = LENS + 8, height = 28, radius = 14,
        -- "#rrggbb" in the mono face is always the same width.
        width = 104,
        color = C.island, border_width = 1, border_color = C.islandBorder,
        reading,
      },
    },
    ui.MouseArea {
      x = 0, y = 0, width = W, height = H,
      cursor = "crosshair",
      accepted_buttons = { "left", "right" },
      on_key_pressed = function(keysym)
        if keysym == KEY.ESCAPE then picker.cancel()
        elseif keysym == KEY.RETURN or keysym == KEY.KP_ENTER then picker.take(at.x, at.y) end
      end,
      on_position_changed = function(sx, sy)
        at.x, at.y, at.seen = sx, sy, true
        picker.look(sx, sy)
      end,
      on_clicked = function(sx, sy, _, _, button)
        if button == "right" then picker.cancel() return end
        picker.take(sx, sy)
      end,
    },
  }
end

M.window = morf.window.layer {
  -- Mixed as Qt mixes, so translucent colours and type match the original.
  blend = require("theme").blend,
  namespace = "impasto-picker",
  layer = "overlay",
  anchors = { top = true, bottom = true, left = true, right = true },
  exclusive_zone = -1,
  keyboard_focus = "exclusive",
  width = W, height = H,
  visible = false,
  root = build(),
}

local open = false
morf.effect("impasto.picker.window", function()
  local want = picker.active()
  if want and not open then M.window:open() open = true
  elseif not want and open then M.window:close() open = false end
end)

--- For a test bench: put the lens at a point as a pointer would.
function M.hover_at(x, y)
  at.x, at.y, at.seen = x, y, true
  picker.look(x, y)
end

return M
