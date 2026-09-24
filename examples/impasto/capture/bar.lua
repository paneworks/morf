-- The capture surface's options: the shape, the kind, and where a photo
-- goes (or, for a video, whether it has sound). Drawn in the island's black
-- at the bottom of the screen as glyphs, each named above the bar while
-- the pointer is on it.
--
-- Port of CaptureBar.qml.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local kit = require("components.kit")
local capture = require("services.capture")
local recorder = require("services.recorder")

local C = theme.color
local M = {}

local GROUP_H = 40
local GLYPH = 20
local SEGMENT = GROUP_H - 6

local fast = function() return theme.behave("fast") end

-- The label of the segment under the pointer, and its centre on the bar.
local tip = morf.signal("impasto.capture.tip", "")
local tip_x = morf.signal("impasto.capture.tip_x", 0)

--- One group: square glyph segments, the chosen one filled.
local function group(values)
  local children = { gap = 2, align = "center" }
  for _, option in ipairs(values.options) do
    local hovered = morf.signal("impasto.capture.segment." .. values.id .. "." .. option.id, false)
    local active = function() return values.current() == option.id end
    local node
    node = ui.Rect {
      width = SEGMENT, height = SEGMENT, radius = theme.radius_small,
      color = function()
        if active() then return C.accent() end
        return hovered:get() and C.islandSurfaceHover or "#00000000"
      end,
      behavior = { color = fast() },
      kit.glyph {
        anchors = { center_in = true }, glyph = option.icon, size = GLYPH,
        color = function()
          if active() then return C.accentText() end
          return hovered:get() and C.text() or C.textMuted()
        end,
        behavior = { color = fast() },
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) tip:set(option.label) end,
        -- Surface coordinates: the segment's centre, wherever the groups put it.
        on_position_changed = function(sx, _, _, _, lx)
          tip:set(option.label)
          tip_x:set(sx - (lx or 0) + SEGMENT / 2)
        end,
        on_exited = function()
          hovered:set(false)
          if tip:get() == option.label then tip:set("") end
        end,
        on_clicked = function() values.on_selected(option.id) end,
      },
    }
    children[#children + 1] = node
  end
  return ui.Row(children)
end

local function rule(visible)
  return ui.Rect { width = 1, height = 22, color = C.islandBorder, visible = visible }
end

function M.build()
  local destinations = {
    { id = "file", label = "Save", icon = "󰆓" },
    { id = "clipboard", label = "Copy", icon = "󰆏" },
  }
  if capture.offers("editor") then destinations[#destinations + 1] = { id = "editor", label = "Annotate", icon = "󰏫" } end
  if capture.offers("text") then destinations[#destinations + 1] = { id = "text", label = "Read text", icon = "󱄽" } end

  local photo = function() return capture.kind() == "photo" end
  local sound_shown = function() return capture.kind() == "video" and recorder.can_audio() end

  -- Positions are read off the row once laid out, so the tip can sit over
  -- the segment it names; the groups are one row, so their x is theirs.
  local row = ui.Row {
    anchors = { center_in = true }, gap = 8, align = "center",
    group {
      id = "shape", current = capture.shape, on_selected = capture.set_shape,
      options = {
        { id = "region", label = "Region", icon = "󰩭" },
        { id = "window", label = "Window", icon = "󰣆" },
        { id = "screen", label = "Screen", icon = "󰍹" },
      },
    },
    rule(true),
    group {
      id = "kind", current = capture.kind, on_selected = capture.set_kind,
      options = {
        { id = "photo", label = "Photo", icon = "󰄀" },
        { id = "video", label = "Video", icon = "󰕧" },
      },
    },
    rule(function() return photo() or sound_shown() end),
    ui.Item {
      width = function() return photo() and (#destinations * (SEGMENT + 2) - 2) or (sound_shown() and (2 * SEGMENT + 2) or 1) end,
      height = SEGMENT,
      ui.Item { visible = photo,
        group { id = "to", current = capture.to, on_selected = capture.set_to, options = destinations } },
      ui.Item { visible = sound_shown,
        group {
          id = "sound",
          current = function() return settings.recorderAudio and "sound" or "mute" end,
          on_selected = function(id) settings.set("recorderAudio", id == "sound") end,
          options = {
            { id = "mute", label = "No sound", icon = "󰖁" },
            { id = "sound", label = "Sound", icon = "󰕾" },
          },
        } },
    },
  }

  return ui.Rect {
    width = function() return (row.layout_width or 0) + 12 end,
    height = 52, radius = 26,
    color = C.island, border_width = 1, border_color = C.islandBorder,
    -- The bar takes its own presses, so a click on it never starts a
    -- selection under it.
    ui.MouseArea { anchors = { fill = true }, z = -1, accepted_buttons = { "left", "right" } },
    row,
  }
end

--- The name of the segment under the pointer, over the bar: a node for the
--- surface's root, in surface coordinates. `top()` is the bar's top edge.
function M.tip(top)
  local label = kit.text { anchors = { center_in = true }, text = function() return tip:get() end, size = theme.size.small, weight = 600 }
  local w = function() return (label.layout_width or 0) + 20 end
  return ui.Rect {
    x = function() return tip_x:get() - w() / 2 end,
    y = function() return top() - 38 end,
    width = w, height = 28, radius = 14,
    color = C.island, border_width = 1, border_color = C.islandBorder,
    opacity = function() return tip:get() ~= "" and 1 or 0 end,
    behavior = { opacity = fast() },
    label,
  }
end

function M.clear_tip() tip:set("") end

return M
