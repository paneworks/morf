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

--- One piece's node, or nil when nothing can draw it. A registered piece
--- draws itself; a module or a door with no piece of its own is drawn as a
--- chip or a button, and a module given its own look always is.
function bar.piece_node(item)
  local modules = require("services.modules")
  local chip = require("bar.modules.chip")
  local own = item.shape ~= "" or item.figure ~= "" or item.when ~= ""
  if own and modules.providers[item.id] and not modules.is_button(item.id) then
    return chip.piece(item.id, { shape = item.shape, figure = item.figure, when = item.when })
  end
  local piece = bar.pieces[item.id]
  if piece then return piece.build(item) end
  if modules.is_button(item.id) then return chip.button(item.id) end
  if modules.providers[item.id] then return chip.piece(item.id) end
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

local function zone(side)
  -- Grouped and spread both draw each run as a capsule; the capsule style
  -- draws the band behind all three instead.
  local chromeless = function() return settings.barStyle == "capsule" end
  local capsules = {}
  for _, group in ipairs(bar.groups_of(bar.items(side))) do
    if group.kind ~= "split" then
      local children = {}
      for _, item in ipairs(group.items) do
        local node = bar.piece_node(item)
        if node then children[#children + 1] = node end
      end
      if #children > 0 then
        local row = ui.Row {
          gap = 2, align = "center",
          height = function() return theme.capsule_height() end,
          table.unpack(children),
        }
        capsules[#capsules + 1] = kit.capsule {
          width = function() return (row.layout_width or 0) + 12 end,
          color = function() return chromeless() and "#00000000" or C.island end,
          border_width = function() return chromeless() and 0 or 1 end,
          ui.Item { anchors = { center_in = true }, width = function() return row.layout_width or 0 end,
            height = function() return theme.capsule_height() end, row },
        }
      end
    end
  end
  local strip = ui.Row {
    gap = function() return chromeless() and 2 or theme.capsule_spacing end,
    height = function() return theme.capsule_height() end,
    table.unpack(capsules),
  }
  -- Anchored to the edge nearest the island, so it grows away from it.
  local anchor
  if settings.barStyle == "spread" then
    anchor = side == "left" and { left = true } or { right = true }
  else
    anchor = side == "left" and { right = true } or { left = true }
  end
  return ui.Item {
    anchors = anchor,
    width = function() return math.max(1, strip.layout_width or 0) end,
    height = function() return theme.capsule_height() end,
    visible = #capsules > 0,
    -- The sides step aside while a panel holds the island, so the panel is
    -- the only thing at the top of the screen.
    opacity = function() return island.state.expanded() and settings.barStyle == "grouped" and 0 or 1 end,
    behavior = { opacity = theme.behave("fast") },
    strip,
  }
end

-- A side is built again whenever its list or the style changes (Settings'
-- layout editor writes both): one row in a model, replaced by a new key.
local function live_zone(side)
  local model = morf.list_model({ { key = 0 } })
  local revision = 0
  morf.effect("impasto.bar.zone." .. side, function()
    settings.get(side == "left" and "barLeft" or "barRight")
    local _ = settings.barStyle
    revision = revision + 1
    if revision > 1 then
      local key = revision
      morf.timer(1, function() model:replace({ { key = key } }, "key") end, false)
    end
  end)
  return ui.Repeater {
    anchors = { fill = true },
    model = model,
    delegate = function() return zone(side) end,
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
    live_zone("left"),
  }
  local right = ui.Item {
    x = function() return island_x() + island_w() + theme.capsule_spacing end,
    y = top,
    width = function() return math.max(0, screen_width - margin() - (island_x() + island_w() + theme.capsule_spacing)) end,
    height = function() return theme.capsule_height() end,
    behavior = { x = theme.behave("morph"), width = theme.behave("morph") },
    live_zone("right"),
  }

  return ui.Item {
    width = screen_width,
    height = function() return theme.bar_band() end,
    left, right, capsule,
  }
end

return bar
