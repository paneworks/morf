-- The bar: the island in the middle, a zone of pieces on either side.
--
-- Port of Bar.qml and BarZone.qml, in its three styles:
--
--   grouped   each side sits against the island and moves aside as it grows
--   spread    each side runs to its screen edge and holds still
--   capsule   everything in one band (impasto's "island" style); the band
--             morphs into whatever the island opens
--
-- A piece is a module (it tells you something and opens its detail in the
-- island) or a button (it opens a panel or does something). Pieces are
-- registered by id with a builder; the settings say which go where.
--
-- Every screen draws this bar and one of them is live (island_state.live,
-- services/auto/live_screen.lua). The others are the same bar at rest, or
-- not painted at all when `barEverywhere` is off; nothing is built or torn
-- down when the island changes hands.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local island = require("bar.island")

local C = theme.color
local bar = { pieces = {} }

--- Registers a piece: `build(item, options)` returns a node of the
--- capsule's height. `options.alone` says it is the only thing in its
--- capsule (a chip is then the capsule itself), `options.chromeless` that
--- it sits in the one-capsule band.
function bar.register(id, piece)
  bar.pieces[id] = piece
end

bar.default_left = { "workspaces", "launcher", "overview" }
bar.default_right = { "notifications", "network", "bluetooth", "volume", "battery" }

-- Inset of each side from the band's end, clear of its curve.
local HOSTED_INSET = 12

--- A side's pieces as `{ id, shape, figure, when }`: the saved list (whose
--- entries are ids, or tables for a piece given its own look), or the
--- catalogue's default side.
function bar.items(side)
  local kept = settings.get(side == "left" and "barLeft" or "barRight")
  local list = type(kept) == "table" and kept or (side == "left" and bar.default_left or bar.default_right)
  local out = {}
  for _, entry in ipairs(list) do
    if type(entry) == "table" and type(entry.id) == "string" then
      out[#out + 1] = { id = entry.id, shape = entry.shape or "", figure = entry.figure or "",
        when = entry.when or "" }
    elseif type(entry) == "string" then
      out[#out + 1] = { id = entry, shape = "", figure = "", when = "" }
    end
  end
  return out
end

--- Whether anything can draw a piece.
local function drawable(id)
  local modules = require("services.modules")
  return bar.pieces[id] ~= nil or modules.is_button(id) or modules.providers[id] ~= nil
end

--- Whether a piece is there on this machine now (BarZone.qml's `present`):
--- no battery on a desktop, a "running" piece while nothing runs.
function bar.present(item)
  if not drawable(item.id) then return false end
  return require("services.modules").shows(item.id, item.when)
end

--- One piece's node, or nil when nothing can draw it. A registered piece
--- draws itself; a module or a door with no piece of its own is drawn as a
--- chip or a button, and a module given its own look always is.
function bar.piece_node(item, options)
  options = options or {}
  local modules = require("services.modules")
  local chip = require("bar.modules.chip")
  local look = { shape = item.shape, figure = item.figure, when = item.when, alone = options.alone }
  local own = item.shape ~= "" or item.figure ~= "" or item.when ~= ""
  if own and modules.providers[item.id] and not modules.is_button(item.id) then
    return chip.piece(item.id, look)
  end
  local piece = bar.pieces[item.id]
  if piece then return piece.build(item, look) end
  if modules.is_button(item.id) then return chip.button(item.id) end
  if modules.providers[item.id] then return chip.piece(item.id, look) end
  return nil
end

--- A side split into what the bar draws: runs of modules and buttons that
--- share a capsule, the workspace strip on its own, and a split between.
function bar.groups_of(items)
  local out, run = {}, {}
  local function flush()
    if #run > 0 then out[#out + 1] = { kind = "chips", items = run } end
    run = {}
  end
  for index, item in ipairs(items) do
    local piece = { id = item.id, shape = item.shape, figure = item.figure, when = item.when, index = index }
    if item.id == "split" then
      flush()
      out[#out + 1] = { kind = "split", items = { piece } }
    elseif item.id == "workspaces" then
      flush()
      out[#out + 1] = { kind = "workspaces", items = { piece } }
    else
      run[#run + 1] = piece
    end
  end
  flush()
  return out
end

-- ----------------------------------------------------------------- shadow --

-- The window shadow's numbers, scaled down for the bar (Theme.qml's
-- `shadowBar*`), when `windowShadow` is on.
local function shadow_color(on)
  if on == false or not settings.windowShadow then return morf.color("#00000000") end
  return morf.color("#000000"):alpha(theme.shadow.opacity)
end
local function shadow_spread()
  return math.floor(theme.shadow.spread * theme.shadow.bar_scale + 0.5)
end
local function shadow_blur()
  return math.floor(theme.shadow.range * theme.shadow.bar_scale + 0.5) - shadow_spread()
end
bar.shadow = { color = shadow_color, spread = shadow_spread, blur = shadow_blur }

-- ------------------------------------------------------------------- zone --

--- The chips one capsule holds, or nil when none of them is here. A capsule
--- holding a single item takes that item's shape (BarChip's `alone`): a
--- ring is its own outline, so it has none, unless its figure is always out.
local function chips_capsule(group, chromeless)
  local modules = require("services.modules")
  local present = {}
  for _, item in ipairs(group.items) do
    if bar.present(item) then present[#present + 1] = item end
  end
  if #present == 0 then return nil end
  local alone = #present == 1
  local children = {}
  for _, item in ipairs(present) do
    local node = bar.piece_node(item, { alone = alone })
    if node then children[#children + 1] = node end
  end
  if #children == 0 then return nil end
  local only = present[1]
  local bare = function()
    if not alone or modules.is_button(only.id) then return false end
    return modules.shape_of(only.id, only.shape) == "ring" and modules.figure_of(only.figure) ~= "on"
  end
  local pad = (chromeless or alone) and 0 or 4
  local row = ui.Row {
    x = pad,
    align = "center",
    height = function() return theme.capsule_height() end,
    table.unpack(children),
  }
  return ui.Rect {
    width = function() return math.max(1, (row.layout_width or 0) + 2 * pad) end,
    height = function() return theme.capsule_height() end,
    radius = function() return theme.capsule_height() / 2 end,
    color = chromeless and "#00000000" or C.island,
    border_color = C.islandBorder,
    border_width = function() return (chromeless or bare()) and 0 or 1 end,
    -- Each capsule casts its own; in the band the bar casts one for all.
    shadow_color = function() return shadow_color(not chromeless) end,
    shadow_blur = shadow_blur,
    shadow_spread = shadow_spread,
    row,
  }
end

--- One side (BarZone.qml): its capsules in layout order, `gap` apart, and
--- its width reported to `width_signal`.
local zone_count = 0
local function zone(side, width_signal)
  zone_count = zone_count + 1
  local chromeless = settings.barStyle == "capsule"
  local capsules = {}
  for _, group in ipairs(bar.groups_of(bar.items(side))) do
    local node
    if group.kind == "workspaces" then
      local piece = bar.pieces.workspaces
      if piece then node = piece.build(group.items[1], { chromeless = chromeless }) end
    elseif group.kind == "chips" then
      node = chips_capsule(group, chromeless)
    end
    if node then capsules[#capsules + 1] = node end
  end
  local strip = ui.Row {
    gap = chromeless and 14 or theme.capsule_spacing,
    height = function() return theme.capsule_height() end,
    table.unpack(capsules),
  }
  -- Anchored to the edge nearest the island (grouped), the screen edge
  -- (spread) or the band's end (capsule), so it grows away from it.
  local anchor
  if settings.barStyle == "grouped" then
    anchor = side == "left" and { right = true } or { left = true }
  else
    anchor = side == "left" and { left = true } or { right = true }
  end
  local holder = ui.Item {
    anchors = anchor,
    width = function() return math.max(1, strip.layout_width or 0) end,
    height = function() return theme.capsule_height() end,
    visible = #capsules > 0,
    strip,
    -- Under the pieces: the pointer coming to a side claims the island for
    -- this screen, as it does coming to the island.
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_entered = function() if island.state.claim then island.state.claim() end end,
    },
  }
  morf.effect("impasto.bar.zone.width." .. side .. "." .. zone_count, function()
    width_signal:set(#capsules > 0 and (strip.layout_width or 0) or 0)
  end, { owner = holder })
  return holder
end

-- A side is built again whenever its list, the style or what is present
-- changes (Settings' layout editor writes the first two; a battery or a
-- running timer the last): one row in a model, replaced by a new key.
local function live_zone(side, width_signal)
  local model = morf.list_model({ { key = 0 } })
  local revision = 0
  local signature
  morf.effect("impasto.bar.zone." .. side, function()
    local items = bar.items(side)
    local parts = { settings.barStyle }
    for _, item in ipairs(items) do
      local shown = item.id == "split" or item.id == "workspaces" or bar.present(item)
      parts[#parts + 1] = item.id .. "/" .. item.shape .. "/" .. item.figure .. "/" .. item.when
        .. (shown and "+" or "-")
    end
    local next = table.concat(parts, " ")
    if next == signature then return end
    signature = next
    revision = revision + 1
    if revision > 1 then
      local key = revision
      morf.timer(1, function() model:replace({ { key = key } }, "key") end, false)
    end
  end)
  return ui.Repeater {
    anchors = { fill = true },
    model = model,
    delegate = function() return zone(side, width_signal) end,
  }
end

-- ------------------------------------------------------------ notch fillet --

--- The concave corner where the attached island meets the screen edge
--- (NotchFillet.qml): the square less a disc on its far corner, mirrored for
--- the left side.
function bar.notch_fillet(values)
  local r = theme.radius_notch
  local size = 2 * r
  local d = values.mirrored
    and string.format("M%d 0 L0 0 A%d %d 0 0 1 %d %d Z", size, size, size, size, size)
    or string.format("M0 0 L%d 0 A%d %d 0 0 0 0 %d Z", size, size, size, size)
  return ui.Path {
    x = values.x, y = values.y or 0,
    width = size, height = size,
    view_box = { 0, 0, size, size },
    d = d,
    fill_color = values.color,
    visible = values.visible,
    behavior = values.behavior,
  }
end

-- ------------------------------------------------------------------- build --

function bar.build(screen_width)
  local W = screen_width
  local state = island.state
  local modules = require("services.modules")
  local spacing = theme.capsule_spacing

  local margin = function() return settings.barSideMargin end
  local style = function() return settings.barStyle end
  local unified = function() return style() == "capsule" end
  local spread = function() return style() == "spread" end
  local grouped = function() return not unified() and not spread() end
  local full_width = function() return unified() and settings.barFullWidth end

  local left_w = morf.signal("impasto.bar.left.width", 0)
  local right_w = morf.signal("impasto.bar.right.width", 0)
  local widest = function() return math.max(left_w:get(), right_w:get()) end

  -- A panel's room: spread, the sides stay put and a panel gets the room
  -- between them; otherwise the sides move aside and it gets the whole bar.
  island.room = function()
    if spread() then return W - 2 * (margin() + widest() + spacing) end
    return W - 2 * margin()
  end

  -- Where the island will be once it has finished moving, so everything
  -- round it goes straight to its place instead of chasing it.
  local island_w = function() local w = island.size() return w end
  local island_left = function() return (W - island_w()) / 2 end
  local island_right = function() return island_left() + island_w() end

  -- Attached (notch mode) only the island reaches the screen edge; the sides
  -- stay capsules, centred on the lane: half a margin up, on the island's
  -- raised midline.
  local island_top = function() return settings.islandAttached and 0 or theme.bar_top_margin() end
  local lane_y = function()
    return settings.islandAttached and theme.bar_top_margin() / 2 or theme.bar_top_margin()
  end

  -- ------------------------------------------------------------- band --

  -- The island is showing more than the clock: a panel, a detail, the
  -- glance or a notification. An OSD fits in the band and does not count.
  local taken = function()
    local layer = state.layer()
    return state.expanded() or layer == "summary" or layer == "notification"
  end
  -- The island's rest width inside the band; an OSD is wider than the clock
  -- and pushes the sides out rather than overlapping them.
  local rest_w = function()
    local layer = state.layer()
    if layer == "osd" or layer == "modules" then return math.max(modules.rest_width(), island_w()) end
    return modules.rest_width()
  end
  -- The same slot at both ends, so the clock stays centred on the screen.
  local slot = function() return HOSTED_INSET + widest() + spacing * 2 end
  local body_w = function()
    if full_width() then return W - 2 * margin() end
    return rest_w() + 2 * slot()
  end
  local body_x = function()
    if full_width() then return margin() end
    return (W - body_w()) / 2
  end
  -- Taken, the band is the island's shape; at rest, the body.
  local band_w = function() return taken() and island_w() or body_w() end
  local band_x = function() return (W - band_w()) / 2 end
  local band_row = function() return theme.capsule_height() + island.notch_pad() end
  local band_h = function() return math.max(band_row(), island.outer_height()) end
  local band_corner = function() return settings.islandAttached and 0 or island.radius() end

  local band = ui.Rect {
    x = band_x, y = island_top,
    width = band_w, height = band_h,
    visible = unified,
    radius = island.radius,
    top_left_radius = band_corner, top_right_radius = band_corner,
    color = island.surface_color,
    shadow_color = function() return shadow_color() end,
    shadow_blur = shadow_blur, shadow_spread = shadow_spread,
    behavior = {
      x = theme.behave("morph"), width = theme.behave("morph"), height = theme.behave("morph"),
      radius = theme.behave("medium"), top_left_radius = theme.behave("medium"),
      top_right_radius = theme.behave("medium"), color = theme.behave("fast"),
    },
    -- Clicking the band opens the island; the sides sit above it and take
    -- their own clicks.
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() if state.claim then state.claim() end end,
      on_clicked = function() if not state.expanded() then island.open("controls") end end,
    },
  }

  -- The island's hairline, drawn round the band once it has the island's
  -- shape. Above both, since the opaque island covers the band's own edge.
  -- None at rest and none in paper mode.
  local outline = ui.Rect {
    x = band_x, y = island_top,
    width = band_w, height = band_h,
    visible = function() return unified() and island.paper() == nil end,
    radius = island.radius,
    top_left_radius = band_corner, top_right_radius = band_corner,
    color = "#00000000",
    border_width = 1,
    border_color = function() return taken() and C.islandBorder or morf.color("#00000000") end,
    behavior = {
      x = theme.behave("morph"), width = theme.behave("morph"), height = theme.behave("morph"),
      radius = theme.behave("medium"), top_left_radius = theme.behave("medium"),
      top_right_radius = theme.behave("medium"), border_color = theme.behave("medium"),
    },
  }

  -- ----------------------------------------------------------- island --

  local capsule = island.build {
    -- Centred on the width it is heading for, and moved on the same curve
    -- as that width, so it stays centred through the whole morph.
    x = island_left,
    y = island_top,
    -- In the band the island casts no shadow of its own at rest, where it
    -- would darken the band round the clock; the band casts it.
    shadow_color = function() return shadow_color(not unified() or taken()) end,
    shadow_blur = shadow_blur,
    shadow_spread = shadow_spread,
  }

  -- Notch fillets flaring from the island's edges out along the screen
  -- edge. In the band they follow whichever edge is further out, the
  -- band's or the island's.
  local fillet_motion = { x = theme.behave("morph"), fill_color = theme.behave("fast") }
  local notch_left = bar.notch_fillet {
    mirrored = true,
    x = function()
      local edge = unified() and math.min(band_x(), island_left()) or island_left()
      return edge - 2 * theme.radius_notch
    end,
    color = island.surface_color,
    visible = function() return settings.islandAttached end,
    behavior = fillet_motion,
  }
  local notch_right = bar.notch_fillet {
    x = function()
      return unified() and math.max(band_x() + band_w(), island_right()) or island_right()
    end,
    color = island.surface_color,
    visible = function() return settings.islandAttached end,
    behavior = fillet_motion,
  }

  -- ------------------------------------------------------------ sides --

  -- Grouped, the sides make room for a module detail but go for a panel.
  -- In the band they go whenever the island takes it.
  local away = function()
    if unified() then return taken() end
    return state.expanded() and grouped() and state.open_panel() ~= "module"
  end

  -- Each side is a holder its zone is anchored in: from the screen edge to
  -- the island's edge (grouped, spread), or the band's slot (capsule).
  local left = ui.Item {
    x = function()
      if unified() then return body_x() + HOSTED_INSET end
      return margin()
    end,
    y = lane_y,
    width = function()
      if unified() then return math.max(1, slot() - HOSTED_INSET) end
      return math.max(1, island_left() - spacing - margin())
    end,
    height = function() return theme.capsule_height() end,
    opacity = function() return away() and 0 or 1 end,
    enabled = function() return not away() end,
    behavior = { x = theme.behave("morph"), width = theme.behave("morph"), opacity = theme.behave("medium") },
    live_zone("left", left_w),
  }
  local right = ui.Item {
    x = function()
      if unified() then return body_x() + body_w() - slot() end
      return island_right() + spacing
    end,
    y = lane_y,
    width = function()
      if unified() then return math.max(1, slot() - HOSTED_INSET) end
      return math.max(1, W - margin() - (island_right() + spacing))
    end,
    height = function() return theme.capsule_height() end,
    opacity = function() return away() and 0 or 1 end,
    enabled = function() return not away() end,
    behavior = { x = theme.behave("morph"), width = theme.behave("morph"), opacity = theme.behave("medium") },
    live_zone("right", right_w),
  }

  -- ------------------------------------------------------------- face --

  -- Painted on the live screen, and on every screen unless `barEverywhere`
  -- is off. No input at all where nothing is painted, nor while the desktop
  -- is being arranged: dragging a widget over the bar would hand the
  -- pointer to this surface and the desktop would drop it.
  local desktop_ok, desktop = pcall(require, "services.desktop")
  local painted = function() return state.live() or settings.barEverywhere end

  -- The band windows keep clear of, on every screen whether painted or not
  -- (BarReserve.qml), so no window is re-tiled when the island changes
  -- hands; it follows the bar's height and margin.
  morf.effect("impasto.bar.reserve", function()
    morf.surface.reserve = { top = theme.bar_reserve() }
  end)
  local inert = function()
    if not painted() then return true end
    return desktop_ok and desktop.editing and desktop.editing:get() or false
  end

  return ui.Item {
    width = W,
    height = function() return theme.bar_band() end,
    opacity = function() return painted() and 1 or 0 end,
    enabled = function() return not inert() end,
    behavior = { opacity = theme.behave("fast") },
    band, capsule, outline, notch_left, notch_right, left, right,
  }
end

return bar
