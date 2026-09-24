-- The workspaces on the bar: dots that stretch into a pill for the one you
-- are on.
--
-- Port of WorkspacesWidget.qml. Three states, by shape and weight alone:
--
--     focused    a wide pill
--     occupied   a dot, solid but dim
--     empty      a dot, dimmer still
--
-- The first `workspaceCount` dots are always there; the rest up to
-- `workspaceMax` appear with use. Every slot exists whether shown or not and
-- is placed by hand, so arrivals and departures both slide rather than jump
-- (a Row keeps room for a hidden child). Drawn in the accent: unlike the
-- battery it carries no warning, so it follows the palette. Outside
-- Hyprland the fixed dots are shown, all empty.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local bar = require("bar.bar")
local kit = require("components.kit")
local workspaces = require("services.workspaces")

local C = theme.color

local DOT = 6
local ACTIVE = 22
local SPACING = 8
local PADDING = 6

local function slot_width(id)
  if not workspaces.is_visible(id) then return 0 end
  return (workspaces.active_id() == id and ACTIVE or DOT) + SPACING
end

-- Where slot `id` starts: the shown slots before it, side by side.
local function slot_x(id)
  local x = PADDING
  for before = 1, id - 1 do x = x + slot_width(before) end
  return x
end

local function total_width()
  local width = 2 * PADDING - SPACING
  local most = math.max(settings.workspaceCount, settings.workspaceMax)
  for id = 1, most do width = width + slot_width(id) end
  return math.max(width, DOT)
end

local function slot(id)
  local hovered = kit.hover_signal("workspace")
  local shown = function() return workspaces.is_visible(id) end
  local focused = function() return workspaces.active_id() == id end
  local occupied = function() return workspaces.occupied(id) end
  return ui.Item {
    x = function() return slot_x(id) end,
    width = function() return math.max(1, slot_width(id)) end,
    height = function() return theme.capsule_height() end,
    visible = shown,
    opacity = function() return shown() and 1 or 0 end,
    behavior = {
      x = theme.behave("medium"),
      width = theme.behave("medium"),
      opacity = theme.behave("fast"),
    },
    ui.Rect {
      anchors = { vertical_center = true },
      x = SPACING / 2,
      width = function() return focused() and ACTIVE or DOT end,
      height = DOT, radius = DOT / 2,
      color = function()
        if focused() or hovered:get() or occupied() then return C.accent() end
        return C.indicatorDim
      end,
      -- Dimming separates occupied from focused without a third shape.
      opacity = function()
        if focused() then return 1 end
        return occupied() and 0.55 or 1
      end,
      behavior = {
        width = theme.behave("medium"),
        color = theme.behave("fast"),
        opacity = theme.behave("fast"),
      },
    },
    -- The whole slot, gap included: a 6 px target is too small.
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() workspaces.focus(id) end,
    },
  }
end

local function build()
  local children = {}
  -- Slots up to the most there can be; a setting raised later shows more
  -- only after a reload, as the original's Repeater over a fixed count.
  local most = math.max(settings.workspaceCount, settings.workspaceMax)
  for id = 1, most do children[#children + 1] = slot(id) end
  return ui.Item {
    width = total_width,
    height = function() return theme.capsule_height() end,
    behavior = { width = theme.behave("medium") },
    table.unpack(children),
  }
end

bar.register("workspaces", { build = build })
