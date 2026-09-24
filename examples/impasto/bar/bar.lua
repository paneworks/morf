-- The bar: the island in the middle, a zone of pieces on either side.
--
-- Port of Bar.qml, in its three styles:
--
--   grouped   each side is a capsule close beside the island
--   spread    each side runs to its screen edge
--   capsule   everything inside one band, the island hosted in it
--
-- A piece is a module (it tells you something and opens its detail in the
-- island) or a button (it opens a panel or does something). Pieces are
-- registered by id with a builder; the settings say which go where.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local island = require("bar.island")
local kit = require("components.kit")

local C = theme.color
local bar = { pieces = {} }

--- Registers a piece: `build()` returns a node of the capsule's height.
function bar.register(id, piece)
  bar.pieces[id] = piece
end

bar.default_left = { "workspaces", "launcher", "overview" }
bar.default_right = { "notifications", "network", "bluetooth", "volume", "battery" }

local function side_ids(side)
  local kept = settings.get(side == "left" and "barLeft" or "barRight")
  if type(kept) == "table" then return kept end
  return side == "left" and bar.default_left or bar.default_right
end

local function zone(side)
  local children = {}
  for _, id in ipairs(side_ids(side)) do
    local piece = bar.pieces[id]
    if piece then children[#children + 1] = piece.build() end
  end
  local row = ui.Row {
    gap = 2, align = "center",
    height = function() return theme.capsule_height() end,
    table.unpack(children),
  }
  -- Grouped and spread both draw each side as a capsule; the capsule style
  -- draws the band behind all three instead. Anchored to the edge nearest
  -- the island, so it grows away from it.
  local anchor
  if settings.barStyle == "spread" then
    anchor = side == "left" and { left = true } or { right = true }
  else
    anchor = side == "left" and { right = true } or { left = true }
  end
  return kit.capsule {
    anchors = anchor,
    width = function() return (row.layout_width or 0) + 12 end,
    visible = #children > 0,
    color = function() return settings.barStyle == "capsule" and "#00000000" or C.island end,
    border_width = function() return settings.barStyle == "capsule" and 0 or 1 end,
    -- The sides step aside while a panel holds the island, so the panel is
    -- the only thing at the top of the screen.
    opacity = function() return island.state.expanded() and settings.barStyle == "grouped" and 0 or 1 end,
    behavior = { opacity = theme.behave("fast") },
    ui.Item { anchors = { center_in = true }, width = function() return row.layout_width or 0 end,
      height = function() return theme.capsule_height() end, row },
  }
end

function bar.build(screen_width)
  local margin = function() return settings.barSideMargin end
  local top = function() return settings.islandAttached and 0 or theme.bar_top_margin() end
  -- Where the island will be once it has finished moving, so the sides go
  -- straight to their places instead of chasing it.
  local island_w = function() local w = island.size() return w end
  local island_x = function() return (screen_width - island_w()) / 2 end
  local spread = function() return settings.barStyle == "spread" end

  local capsule = island.build {
    -- Centred on the width it is heading for, and moved on the same curve
    -- as that width, so it stays centred through the whole morph.
    x = island_x,
    y = top,
  }
  -- Each side is a holder from the screen edge to the island's edge; its
  -- capsule sits against the island (grouped) or the screen edge (spread).
  local left = ui.Item {
    x = function() return margin() end,
    y = top,
    width = function() return math.max(0, island_x() - theme.capsule_spacing - margin()) end,
    height = function() return theme.capsule_height() end,
    behavior = { width = theme.behave("morph") },
    zone("left"),
  }
  local right = ui.Item {
    x = function() return island_x() + island_w() + theme.capsule_spacing end,
    y = top,
    width = function() return math.max(0, screen_width - margin() - (island_x() + island_w() + theme.capsule_spacing)) end,
    height = function() return theme.capsule_height() end,
    behavior = { x = theme.behave("morph"), width = theme.behave("morph") },
    zone("right"),
  }

  return ui.Item {
    width = screen_width,
    height = function() return theme.bar_band() end,
    left, right, capsule,
  }
end

return bar
