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
--
-- A panel may also say, while it is open, that the island is paper rather
-- than black -- `paper = function() return colour or nil end` -- and how far
-- its contents sit from the rim -- `padding = function() return 0 end`. An
-- open note is both: the island becomes the note, to the edge. Every panel
-- is laid out at its declared size from the first frame, not resized with
-- the capsule as it grows.
--
-- The bar tells the island two things about where it sits: how wide a panel
-- may be (`island.room`, the room between spread sides) and whether it is
-- hosted in the one-capsule band (`island.hosted`), where it draws no
-- hairline and does not light at rest. Attached (notch mode, the
-- `islandAttached` setting) its top reaches the screen edge: the shape grows
-- by the bar's top margin (`island.notch_pad`) and its upper corners go
-- square, while the contents keep their place.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local state = require("bar.island_state")
local kit = require("components.kit")

local C = theme.color
local island = { state = state, panels = {}, layers = {} }

local ESCAPE = 0xff1b

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

--- The widest a panel may be; the bar replaces it (Bar.qml's `panelRoom`).
island.room = function() return math.huge end

--- Whether the island sits inside the one-capsule band.
function island.hosted() return settings.barStyle == "capsule" end

--- How far the shape reaches above its contents when attached.
function island.notch_pad()
  return settings.islandAttached and theme.bar_top_margin() or 0
end

-- ----------------------------------------------------------------- size --

local function panel_size()
  local panel = island.panels[state.open_panel()]
  if panel and panel.size then return panel.size() end
  return 560, 420
end

--- A panel's inner margin: its own `padding` when it declares one -- a
--- number, or a function when it changes while open (a note goes to the
--- edge) -- else the theme's.
local function panel_padding(panel)
  local padding = panel and panel.padding
  if type(padding) == "function" then return padding() end
  if type(padding) == "number" then return padding end
  return theme.panel_padding
end

function island.panel_padding(name)
  return panel_padding(island.panels[name or state.open_panel()])
end

--- The paper colour the open panel asks for, or nil for the island's black.
function island.paper()
  if state.layer() ~= "panel" then return nil end
  local panel = island.panels[state.open_panel()]
  if panel and panel.paper then return panel.paper() end
  return nil
end

--- The capsule's target size and inner padding for the current layer, not
--- counting the notch (`island.outer_height` does).
function island.size()
  local layer = state.layer()
  if layer == "panel" then
    local w, h = panel_size()
    -- Only the overview is ever wide enough to meet the limit.
    return math.min(w, math.max(1, island.room())), h, panel_padding(island.panels[state.open_panel()])
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
local function height() local _, h = island.size() return h + island.notch_pad() end

--- The shape's full height, notch included.
island.outer_height = height

--- A pill at capsule height, a rounded card once it is taller; an open panel
--- other than a module's detail is always the card.
function island.radius()
  if state.expanded() and not (island.panels[state.open_panel()] or {}).pill then
    return theme.radius_large
  end
  return math.min(height() / 2, theme.radius_large + 4)
end

-- ---------------------------------------------------------------- glance --

-- Hovering opens the glance after a short intent delay, leaving closes it
-- after a grace period, and a click opens the control centre. 80 ms is
-- under a perceptible wait but longer than a pointer crossing the clock;
-- the grace period stops a pointer on the edge from flapping it.
--
-- Three things hold it off: a click (until the pointer leaves, so a panel
-- just dismissed is not replaced by the glance), a side of the resting
-- island under the pointer (`rest_busy`, so a click on the recording's stop
-- never lands on a glance that opened under it), and the island landing
-- back at rest from anything bigger, since opening the glance mid-morph
-- would turn the shape round halfway.
local hovered = kit.hover_signal("island")
local held = morf.signal("impasto.island.held", false)
local landing = morf.signal("impasto.island.landing", false)
island.rest_busy = morf.signal("impasto.island.rest.busy", false)
local dwell_generation, grace_generation = 0, 0

local function can_summarise()
  local layer = state.layer()
  local ok, modules = pcall(require, "services.modules")
  local detail = ok and modules.open_id:get() or ""
  return settings.islandSummary and state.live() and not held:get() and not landing:get()
    and detail == ""
    and (layer == "modules" or layer == "summary")
end

local function dwell()
  dwell_generation = dwell_generation + 1
  local mine = dwell_generation
  morf.timer(80, function()
    if mine == dwell_generation and hovered:get() and can_summarise() and not island.rest_busy:get() then
      state.set_summary(true)
    end
  end, false)
end

local function on_hover(over)
  hovered:set(over)
  if over then
    grace_generation = grace_generation + 1
    if state.claim then state.claim() end
    if not state.signals.summary:get() then dwell() end
  else
    dwell_generation = dwell_generation + 1
    held:set(false)
    grace_generation = grace_generation + 1
    local mine = grace_generation
    morf.timer(260, function()
      if mine == grace_generation and not hovered:get() then state.set_summary(false) end
    end, false)
  end
end

-- The glance goes as soon as it may not be shown.
morf.effect("impasto.island.glance.guard", function()
  if not can_summarise() and state.signals.summary:get() then state.set_summary(false) end
end)

-- A side coming under the pointer stops the dwell; leaving it restarts it.
morf.effect("impasto.island.glance.busy", function()
  local busy = island.rest_busy:get()
  if busy then
    dwell_generation = dwell_generation + 1
  elseif hovered:get() and not state.signals.summary:get() then
    morf.timer(1, dwell, false)
  end
end)

-- Landing: the island shrinking back to rest from a panel, a detail, a
-- notification or an OSD. The glance's own close is not one, since coming
-- back should reverse it. Over once the morph has had its time.
do
  local last = state.layer()
  local generation = 0
  morf.effect("impasto.island.landing", function()
    local layer = state.layer()
    if layer == "modules" and last ~= "summary" and last ~= "modules" then
      landing:set(true)
      generation = generation + 1
      local mine = generation
      morf.timer(theme.duration_morph() + 20, function()
        if mine ~= generation then return end
        landing:set(false)
        if hovered:get() and not state.signals.summary:get() then dwell() end
      end, false)
    elseif layer ~= "modules" then
      generation = generation + 1
      landing:set(false)
    end
    last = layer
  end)
end

--- Lit where a click does something: the clock at rest (not in the band,
--- where the band and the island are one shape) and the glance; never
--- while landing.
function island.lit()
  if not hovered:get() or landing:get() then return false end
  local layer = state.layer()
  return (layer == "modules" and not island.hosted()) or layer == "summary"
end

--- The shape's colour when no panel is open, which the band and the notch
--- fillets share.
function island.surface_color()
  if island.lit() then return C.islandSurfaceHover end
  return settings.islandAttached and C.island or C.islandSurface
end

--- The shape's colour now.
function island.color()
  if state.expanded() then return island.paper() or C.island end
  return island.surface_color()
end

-- ---------------------------------------------------------------- escape --

--- Escape: out of the control centre's arranging first, else the island
--- closes (DynamicIsland.qml's one handler, ControlsPanel.qml's first).
function island.escape()
  if state.open_panel() == "controls" then
    local ok, controls = pcall(require, "services.controls")
    if ok and controls.editing and controls.editing:get() then
      controls.edit(false)
      return
    end
  end
  state.close()
end

-- --------------------------------------------------------------- layers --

-- A layer's contents stay built a moment after it stops showing, so they
-- fade out rather than vanish; a layer that comes back in that moment is
-- the same node.
local function mounted(predicate, name)
  local shown = morf.signal("impasto.island.mounted." .. name, predicate())
  local generation = 0
  -- Mirrors `shown` without reading it, so the effect follows only the
  -- predicate: a predicate that re-runs while staying false must not arm a
  -- fresh timer each time.
  local mounted_now = shown:get()
  morf.effect("impasto.island.mount." .. name, function()
    local want = predicate()
    generation = generation + 1
    local mine = generation
    if want then
      mounted_now = true
      shown:set(true)
    elseif mounted_now then
      morf.timer(theme.duration_morph() + 40, function()
        if mine == generation and not predicate() then
          mounted_now = false
          shown:set(false)
        end
      end, false)
    end
  end)
  return shown
end

local function layer_loader(name, predicate, build, anchors)
  local keep = mounted(predicate, name)
  return ui.Item {
    anchors = anchors or { fill = true },
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
  -- Attached, a layer is centred in the full height, on the bar's raised
  -- midline (Bar.qml's `laneY`).
  local half_notch = function() return island.notch_pad() / 2 end
  for _, name in ipairs { "modules", "summary", "osd", "notification" } do
    children[#children + 1] = layer_loader(name,
      function() return state.layer() == name end,
      function()
        local layer = island.layers[name]
        if layer and layer.build then return layer.build(island) end
        return ui.Item {}
      end,
      function()
        local pad = half_notch()
        return { fill = true, top_margin = pad, bottom_margin = pad }
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
        -- inside its padding, rather than squeezed to nothing and reflowed
        -- on every frame of the morph. Attached, it starts below the notch.
        if panel.size then
          return ui.Item {
            x = function() return panel_padding(panel) end,
            y = function() return panel_padding(panel) + island.notch_pad() end,
            width = function()
              local w = panel.size()
              return math.max(1, math.min(w, island.room()) - 2 * panel_padding(panel))
            end,
            height = function() local _, h = panel.size() return math.max(1, h - 2 * panel_padding(panel)) end,
            panel.build(island),
          }
        end
        return ui.Inset {
          anchors = { fill = true },
          margin = function() return panel_padding(panel) end,
          panel.build(island),
        }
      end)
  end

  local corner = function()
    -- Attached, the upper corners are square: NotchFillet adds the curve
    -- outside the shape instead.
    return settings.islandAttached and 0 or island.radius()
  end
  local capsule = ui.ClipRect {
    x = place.x, y = place.y,
    width = width,
    height = height,
    radius = island.radius,
    top_left_radius = corner,
    top_right_radius = corner,
    color = island.color,
    -- No hairline in paper mode, nor in the band, where at rest its edges
    -- run through the band and growing, the band is the outer edge (the bar
    -- draws the outline round both).
    border_width = function() return (island.paper() or island.hosted()) and 0 or 1 end,
    border_color = C.islandBorder,
    shadow_color = place.shadow_color,
    shadow_blur = place.shadow_blur,
    shadow_spread = place.shadow_spread,
    behavior = {
      x = theme.behave("morph"),
      width = theme.behave("morph"),
      height = theme.behave("morph"),
      radius = theme.behave("medium"),
      top_left_radius = theme.behave("medium"),
      top_right_radius = theme.behave("medium"),
      color = theme.behave("fast"),
    },
    -- Under the layers, so a layer's own controls get clicks first; this
    -- still sees hover everywhere on the island. It is also where the keys
    -- go while a panel is open and nothing in it has asked for them: one
    -- Escape for every panel.
    ui.MouseArea {
      anchors = { fill = true },
      z = -1,
      accepted_buttons = "all",
      cursor = function() return state.expanded() and "default" or "pointer" end,
      on_entered = function() on_hover(true) end,
      on_exited = function() on_hover(false) end,
      on_clicked = function(button)
        if state.expanded() or button ~= "left" then return end
        held:set(true)
        state.open("controls")
      end,
      on_key_pressed = function(keysym)
        if keysym == ESCAPE and state.expanded() then island.escape() end
      end,
    },
    table.unpack(children),
  }
  island.node = capsule
  return capsule
end

island.hovered = hovered
island.landing = landing

return island
