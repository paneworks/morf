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
-- (a Row keeps room for a hidden child); raising `workspaceMax` adds slots
-- at once. Drawn in the accent: unlike the battery it carries no warning, so
-- it follows the palette. Outside Hyprland the fixed dots are shown, all
-- empty.
--
-- The strip is about the screen it is drawn on: the ten are shared, and
-- the pill marks the one this screen is showing, which on a screen not
-- being worked on is not the focused workspace. In the one-capsule band it
-- drops its own capsule and padding.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local bar = require("bar.bar")
local kit = require("components.kit")
local workspaces = require("services.workspaces")

local ok_lib, hyprland = pcall(require, "lib.integrations.hyprland")
if not ok_lib then hyprland = nil end

local C = theme.color

local DOT = 6
local ACTIVE = 22
local SPACING = 8
-- The capsule's padding either side of the row of slots (whose own half
-- gaps make it look like 10).
local PADDING = 6

local own_screen = ((morf.screens or {})[1] or {}).name or ""

--- The workspace this screen shows: its monitor's active one, or the
--- focused one off a screen of its own (and in the demo).
local function active_id()
  local focused = workspaces.active_id()
  if not hyprland or own_screen == "" or workspaces.demo:get() then return focused end
  -- The focused monitor's changes on the event; another's on the refetch.
  if hyprland.state.focused_monitor == own_screen then return focused end
  for _, monitor in ipairs(workspaces.monitors()) do
    if monitor.name == own_screen and (monitor.active_workspace or 0) > 0 then
      return monitor.active_workspace
    end
  end
  return focused
end

local function most()
  return math.max(1, settings.workspaceCount, settings.workspaceMax)
end

local function slot_width(id)
  if not workspaces.is_visible(id) then return 0 end
  return (active_id() == id and ACTIVE or DOT) + SPACING
end

local function slots_width()
  local width = 0
  for id = 1, most() do width = width + slot_width(id) end
  return width
end

local function slot(id, origin)
  local hovered = kit.hover_signal("workspace")
  local shown = function() return workspaces.is_visible(id) end
  local focused = function() return active_id() == id end
  local occupied = function() return workspaces.occupied(id) end
  return ui.Item {
    x = function()
      local x = origin()
      for before = 1, id - 1 do x = x + slot_width(before) end
      return x
    end,
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

local function build(_, options)
  local chromeless = options and options.chromeless or false
  -- Chromeless, the row's outer half gaps hang over the ends.
  local pad = chromeless and -SPACING / 2 or PADDING
  local origin = function() return pad end
  -- A slot per workspace up to the most there can be, following the
  -- settings: one row per id, kept by key, so a raised maximum adds slots
  -- and a lowered one removes them without rebuilding the rest.
  local model = morf.list_model({})
  local function rows()
    local out = {}
    for id = 1, most() do out[#out + 1] = { id = id } end
    return out
  end
  model:replace(rows(), "id")
  local count = most()
  local node
  node = ui.Rect {
    width = function() return math.max(DOT, slots_width() + 2 * pad) end,
    height = function() return theme.capsule_height() end,
    radius = function() return theme.capsule_height() / 2 end,
    color = chromeless and "#00000000" or C.island,
    border_color = C.islandBorder,
    border_width = chromeless and 0 or 1,
    shadow_color = function() return bar.shadow.color(not chromeless) end,
    shadow_blur = bar.shadow.blur,
    shadow_spread = bar.shadow.spread,
    behavior = { width = theme.behave("medium") },
    ui.Repeater {
      anchors = { fill = true },
      model = model,
      delegate = function(row) return slot(row.id, origin) end,
    },
  }
  morf.effect("impasto.bar.workspaces.slots", function()
    local want = most()
    if want == count then return end
    count = want
    morf.timer(1, function() model:replace(rows(), "id") end, false)
  end, { owner = node })
  return node
end

bar.register("workspaces", { build = build })
