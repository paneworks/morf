-- The island: one black capsule in the middle of the bar that changes shape
-- to fit whatever it shows.
--
-- Port of DynamicIsland.qml. `island_state` picks the layer; this sizes the
-- capsule for it and shows that layer's contents, the shape morphing while
-- the contents cross-fade. Panels are registered by name with a declared
-- size (the capsule must reach its final shape before the panel inside it
-- exists) and a builder:
--
--     island.register("controls", {
--       size = function() return 560, 420 end,
--       build = function(island) return ui.Item { ... } end,
--     })

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local state = require("bar.island_state")
local kit = require("components.kit")

local C = theme.color
local island = { state = state, panels = {}, layers = {} }

--- Registers a panel the island can open.
function island.register(name, panel)
  island.panels[name] = panel
end

--- Registers what a layer below `panel` shows: `modules`, `summary`, `osd`,
--- `notification`. `size()` returns width, height; `build()` a node.
function island.register_layer(name, layer)
  island.layers[name] = layer
end

island.open = state.open
island.close = state.close
island.toggle = state.toggle

-- ----------------------------------------------------------------- size --

local function panel_size()
  local panel = island.panels[state.open_panel()]
  if panel and panel.size then return panel.size() end
  return 560, 420
end

--- The capsule's target size and inner padding for the current layer.
function island.size()
  local layer = state.layer()
  if layer == "panel" then
    local w, h = panel_size()
    return w, h, theme.panel_padding
  end
  local entry = island.layers[layer]
  if entry and entry.size then
    local w, h, pad = entry.size()
    return w, h, pad or 0
  end
  if layer == "osd" then return 260, theme.capsule_height(), 10 end
  if layer == "notification" then return 430, 68, 13 end
  if layer == "summary" then return 384, 116, 0 end
  return settings.clockShowsDate and 240 or 150, theme.capsule_height(), 0
end

local function width() local w = island.size() return w end
local function height() local _, h = island.size() return h end

-- ---------------------------------------------------------------- glance --

-- Hovering opens the glance after a short intent delay, leaving closes it
-- after a grace period, and a click opens the control centre. 80 ms is
-- under a perceptible wait but longer than a pointer crossing the clock;
-- the grace period stops a pointer on the edge from flapping it.
local hovered = kit.hover_signal("island")
local held = morf.signal("impasto.island.held", false)
local hover_generation = 0

local function can_summarise()
  local layer = state.layer()
  return settings.islandSummary and not held:get()
    and (layer == "modules" or layer == "summary")
end

local function on_hover(over)
  hovered:set(over)
  hover_generation = hover_generation + 1
  local mine = hover_generation
  if over then
    morf.timer(80, function()
      if mine == hover_generation and hovered:get() and can_summarise() then state.set_summary(true) end
    end, false)
  else
    held:set(false)
    morf.timer(260, function()
      if mine == hover_generation and not hovered:get() then state.set_summary(false) end
    end, false)
  end
end

-- --------------------------------------------------------------- layers --

-- A layer's contents stay built a moment after it stops showing, so they
-- fade out rather than vanish; a layer that comes back in that moment is
-- the same node.
local function mounted(predicate, name)
  local shown = morf.signal("impasto.island.mounted." .. name, predicate())
  local generation = 0
  morf.effect("impasto.island.mount." .. name, function()
    local want = predicate()
    generation = generation + 1
    local mine = generation
    if want then
      shown:set(true)
    else
      morf.timer(theme.duration_morph() + 40, function()
        if mine == generation and not predicate() then shown:set(false) end
      end, false)
    end
  end)
  return shown
end

local function layer_loader(name, predicate, build)
  local keep = mounted(predicate, name)
  return ui.Item {
    anchors = { fill = true },
    opacity = function() return predicate() and 1 or 0 end,
    behavior = { opacity = { duration = 110, easing = "out_cubic" } },
    enter = { opacity = 0 },
    ui.Loader {
      anchors = { fill = true },
      active = function() return keep:get() end,
      source = build,
    },
  }
end

function island.build(place)
  place = place or {}
  local children = {}
  for _, name in ipairs { "modules", "summary", "osd", "notification" } do
    children[#children + 1] = layer_loader(name,
      function() return state.layer() == name end,
      function()
        local layer = island.layers[name]
        if layer and layer.build then return layer.build(island) end
        return ui.Item {}
      end)
  end
  -- One loader per panel, so switching panels swaps contents while the
  -- capsule morphs between their two sizes.
  for name, panel in pairs(island.panels) do
    children[#children + 1] = layer_loader("panel." .. name,
      function() return state.open_panel() == name end,
      function()
        -- At the panel's declared size, not the capsule's: the capsule is
        -- still small when this is built and grows around it (clipping it
        -- meanwhile), so the panel is laid out once, at its final size,
        -- rather than squeezed to nothing and reflowed on every frame of
        -- the morph.
        if panel.size then
          local pad = theme.panel_padding
          return ui.Item {
            x = pad, y = pad,
            width = function() local w = panel.size() return math.max(1, w - 2 * pad) end,
            height = function() local _, h = panel.size() return math.max(1, h - 2 * pad) end,
            panel.build(island),
          }
        end
        return ui.Inset {
          anchors = { fill = true },
          margin = theme.panel_padding,
          panel.build(island),
        }
      end)
  end

  local capsule = ui.ClipRect {
    x = place.x, y = place.y,
    width = width,
    height = height,
    radius = function()
      if state.expanded() then return theme.radius_large end
      return math.min(height() / 2, theme.radius_large + 4)
    end,
    color = function()
      if state.expanded() then return C.island end
      local layer = state.layer()
      if hovered:get() and (layer == "modules" or layer == "summary") then return C.islandSurfaceHover end
      return settings.islandAttached and C.island or C.islandSurface
    end,
    border_width = 1,
    border_color = C.islandBorder,
    behavior = {
      x = theme.behave("morph"),
      width = theme.behave("morph"),
      height = theme.behave("morph"),
      radius = theme.behave("medium"),
      color = theme.behave("fast"),
    },
    -- Under the layers, so a layer's own controls get clicks first; this
    -- still sees hover everywhere on the island.
    ui.MouseArea {
      anchors = { fill = true },
      z = -1,
      cursor = function() return state.expanded() and "default" or "pointer" end,
      on_entered = function() on_hover(true) end,
      on_exited = function() on_hover(false) end,
      on_clicked = function()
        if state.expanded() then return end
        held:set(true)
        state.open("controls")
      end,
    },
    table.unpack(children),
  }
  island.node = capsule
  return capsule
end

island.hovered = hovered

return island
