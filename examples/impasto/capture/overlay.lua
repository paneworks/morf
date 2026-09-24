-- The capture surface: the photo taken when the key was pressed, drawn full
-- screen, with everything outside the selection dimmed and the options bar
-- at the bottom.
--
-- Port of CaptureOverlay.qml. A layer of its own on the overlay layer,
-- over every screen edge and ignoring the bar's reserve, so a drag that runs
-- under the bar stays on it; the keyboard is exclusive, or hovering a
-- window would take Escape away. Working on a still picture means a menu
-- can be captured and nothing has to be frozen.
--
-- Region: press and drag. Window: the window under the pointer lights, a
-- click takes it. Screen: a click (or Enter) takes it all. 1, 2 and 3 pick
-- the shape, Escape or a right click cancels.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local capture = require("services.capture")
local capture_bar = require("capture.bar")

local C = theme.color
local M = {}

local KEY = {
  ESCAPE = 0xff1b, RETURN = 0xff0d, KP_ENTER = 0xff8d, SPACE = 0x20,
  ONE = 0x31, TWO = 0x32, THREE = 0x33,
}

local WASH = "#00000099"

local screen = (morf.screens or {})[1] or {}
local W = tonumber(screen.width) or 1920
local H = tonumber(screen.height) or 1080

local sel = morf.state { x = 0, y = 0, w = 0, h = 0, dragging = false, ox = 0, oy = 0, px = 0, py = 0 }
M.selection = sel

local function whole() return capture.shape() == "screen" end
local function showing() return whole() or (sel.w > 1 and sel.h > 1) end

local function set(x, y, w, h)
  sel.x, sel.y, sel.w, sel.h = x, y, w, h
end

local function clear() set(0, 0, 0, 0) end

local function window_at(x, y)
  local found = capture.window_under(x, y)
  if found then set(found.x, found.y, found.width, found.height) else clear() end
end

--- Takes what is selected; the whole screen passes no box.
function M.take()
  if whole() then capture.fire(0, 0, 0, 0) return end
  if capture.shape() == "window" and sel.w < 4 then window_at(sel.px, sel.py) end
  if sel.w < 4 or sel.h < 4 then return end
  capture.fire(sel.x, sel.y, sel.w, sel.h)
end

-- A fresh surface every opening.
morf.effect("impasto.capture.reset", function()
  capture.signals.opened:get()
  clear()
  sel.dragging = false
end)

local function key(keysym)
  if keysym == KEY.ESCAPE then capture.cancel()
  elseif keysym == KEY.ONE then capture.set_shape("region") clear()
  elseif keysym == KEY.TWO then capture.set_shape("window") window_at(sel.px, sel.py)
  elseif keysym == KEY.THREE then capture.set_shape("screen")
  elseif keysym == KEY.RETURN or keysym == KEY.KP_ENTER or keysym == KEY.SPACE then M.take()
  end
end

local function move(x, y)
  sel.px, sel.py = x, y
  if sel.dragging then
    set(math.min(sel.ox, x), math.min(sel.oy, y), math.abs(x - sel.ox), math.abs(y - sel.oy))
    return
  end
  if capture.shape() == "window" then window_at(x, y) end
end

local function build()
  local bar = capture_bar.build()
  local bar_holder = ui.Item {
    anchors = { bottom = true, bottom_margin = theme.bar_top_margin() + 14 },
    x = function() return (W - (bar.layout_width or 0)) / 2 end,
    width = function() return bar.layout_width or 0 end, height = 52,
    z = 5,
    opacity = function() return sel.dragging and 0 or 1 end,
    behavior = { opacity = theme.behave("fast") },
    bar,
  }
  local wash = function(values)
    values.color = WASH
    return ui.Rect(values)
  end
  local label = kit.text {
    anchors = { center_in = true },
    text = function() return ("%d × %d"):format(math.floor(sel.w + 0.5), math.floor(sel.h + 0.5)) end,
    mono = true, size = theme.size.small, weight = 600,
  }
  local label_w = function() return (label.layout_width or 0) + 20 end

  return ui.Item {
    width = W, height = H,
    ui.Rect { width = W, height = H, color = "#000000" },
    ui.Image {
      x = 0, y = 0, width = W, height = H, fill_mode = "stretch",
      source = function() return capture.photo() end,
    },

    -- Outside the selection: four rectangles, as the hole is always one.
    ui.Item {
      x = 0, y = 0, width = W, height = H,
      visible = function() return not whole() end,
      wash { x = 0, y = 0, width = W, height = function() return math.max(0, showing() and sel.y or H) end },
      wash { x = 0, width = W,
        y = function() return sel.y + sel.h end,
        height = function() return showing() and math.max(0, H - sel.y - sel.h) or 0 end,
        visible = showing },
      wash { x = 0, y = function() return sel.y end,
        width = function() return math.max(0, sel.x) end, height = function() return sel.h end,
        visible = showing },
      wash { x = function() return sel.x + sel.w end, y = function() return sel.y end,
        width = function() return math.max(0, W - sel.x - sel.w) end, height = function() return sel.h end,
        visible = showing },
    },
    -- Screen mode: an outline at the edges says a click takes it all.
    ui.Rect {
      x = 0, y = 0, width = W, height = H, color = "#00000000",
      border_width = 3, border_color = C.accent,
      visible = whole,
    },
    -- The selection's outline and its size.
    ui.Rect {
      x = function() return sel.x end, y = function() return sel.y end,
      width = function() return math.max(1, sel.w) end, height = function() return math.max(1, sel.h) end,
      color = "#00000000", border_width = 2, border_color = C.accent,
      visible = function() return showing() and not whole() end,
    },
    ui.Rect {
      visible = function() return showing() and not whole() end,
      x = function() return math.min(math.max(0, sel.x), W - label_w()) end,
      y = function() return sel.y > 36 and (sel.y - 36) or (sel.y + 8) end,
      width = label_w, height = 28, radius = 14,
      color = C.island, border_width = 1, border_color = C.islandBorder,
      label,
    },

    ui.MouseArea {
      x = 0, y = 0, width = W, height = H,
      cursor = function() return capture.shape() == "region" and "crosshair" or "default" end,
      accepted_buttons = { "left", "right" },
      on_key_pressed = key,
      on_position_changed = function(sx, sy) move(sx, sy) end,
      on_dragged = function(sx, sy) move(sx, sy) end,
      on_pressed = function(sx, sy, _, _, button)
        capture_bar.clear_tip()
        if button == "right" then capture.cancel() return end
        if capture.shape() ~= "region" then return end
        sel.dragging = true
        sel.ox, sel.oy = sx, sy
        set(sx, sy, 0, 0)
      end,
      on_released = function(_, _, _, _, button)
        if button == "right" then return end
        if not sel.dragging then
          -- Window and screen take a single click.
          if capture.shape() ~= "region" then M.take() end
          return
        end
        sel.dragging = false
        -- A click without a drag in region mode is nothing.
        if sel.w < 4 or sel.h < 4 then clear() return end
        M.take()
      end,
    },
    bar_holder,
    capture_bar.tip(function() return bar_holder.layout_y or (H - 66 - 52) end),
  }
end

M.window = morf.window.layer {
  namespace = "impasto-capture",
  layer = "overlay",
  anchors = { top = true, bottom = true, left = true, right = true },
  exclusive_zone = -1,
  keyboard_focus = "exclusive",
  width = W, height = H,
  visible = false,
  root = build(),
}

local open = false
morf.effect("impasto.capture.window", function()
  local want = capture.active()
  if want and not open then M.window:open() open = true
  elseif not want and open then M.window:close() open = false end
end)

return M
