-- The card beside a block clicked while arranging: BlockInspector.qml.
--
-- Its name and Remove; the sizes the block offers, drawn as cell
-- footprints (the corner handle and the wheel do the same); and for the
-- toggles block every switch in the catalogue, ticked when it is on this
-- block, a newly ticked one going to the end and the arrows ordering them.
--
-- It goes right of the block, else left of it, else below, kept on the
-- board. Board pixels; the panel puts it over the grid.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local service = require("services.controls")

local C = theme.color
local M = {}

M.WIDTH = 268
M.PAD = 14
M.GAP = 14
local INNER = M.WIDTH - 2 * M.PAD

local function key() return service.selected:get() end
local function block() return service.entry_of(key()) end
local function block_id() local b = block() return b and b.id or "" end
local function on_toggles() return block_id() == "toggles" end

--- Every size the catalogue offers anywhere, one tile each, shown when the
--- selected block offers it.
local function size_tiles()
  local all, seen = {}, {}
  for _, entry in ipairs(service.catalogue) do
    for _, size in ipairs(entry.sizes) do
      if not seen[size] then seen[size] = true all[#all + 1] = size end
    end
  end
  table.sort(all, function(a, b)
    local ac, ar = service.parse(a)
    local bc, br = service.parse(b)
    if ac ~= bc then return ac < bc end
    return ar < br
  end)
  local flow = { direction = "row", wrap = true, gap = 8, width = INNER }
  for _, size in ipairs(all) do
    local cols, rows = service.parse(size)
    local hovered = controls.signal("inspector.size", false)
    local offered = function() return service.offers(block_id(), size) end
    local current = function() return service.size_of(block()) == size end
    flow[#flow + 1] = ui.Rect {
      visible = offered,
      width = math.max(48, cols * 11 + 16), height = 58,
      radius = theme.radius_small,
      color = function() return current() and C.islandSurfaceHover or "#00000000" end,
      border_width = 1,
      border_color = function()
        if current() then return C.accent() end
        return hovered:get() and C.islandBorder or C.hairline
      end,
      ui.Rect {
        anchors = { horizontal_center = true, bottom = true, bottom_margin = 6 + 12 + 6 },
        width = cols * 11, height = rows * 4, radius = 2,
        color = function() return current() and C.accent() or C.textMuted() end,
      },
      kit.text {
        anchors = { horizontal_center = true, bottom = true, bottom_margin = 6 },
        text = service.label(size), mono = true, size = theme.size.label,
        color = function() return current() and C.text() or C.textMuted() end,
      },
      controls.hit { hovered = hovered, on_click = function() service.set_size(key(), size) end },
    }
  end
  return ui.Flex(flow)
end

--- One switch of the catalogue: its tick, glyph and name, and while it is
--- on this block, the arrows that move it.
local function tick_row(index)
  local tile = function() return service.tile_rows_of(key())[index] end
  local on = function() local t = tile() return t ~= nil and service.shows_tile_in(key(), t.key) end
  local at = function()
    local t = tile()
    if not t then return 0 end
    for i, each in ipairs(service.toggle_keys_of(key())) do if each == t.key then return i end end
    return 0
  end
  local hovered = controls.signal("inspector.tick", false)
  local function arrow(glyph, delta)
    local arrow_hover = controls.signal("inspector.arrow", false)
    local usable = function()
      if delta < 0 then return at() > 1 end
      return at() > 0 and at() < #service.toggle_keys_of(key())
    end
    return ui.Rect {
      width = 20, height = 20, radius = theme.radius_small,
      color = function() return (arrow_hover:get() and usable()) and C.islandSurface or "#00000000" end,
      opacity = function() return usable() and 1 or 0.3 end,
      kit.glyph { anchors = { center_in = true }, glyph = glyph, size = 11 },
      -- Always takes the press, so a tap never falls through to the row.
      controls.hit { hovered = arrow_hover, on_click = function()
        local t = tile()
        if t and usable() then service.move_tile_in(key(), t.key, delta) end
      end },
    }
  end
  return ui.Rect {
    width = INNER, height = 24, radius = theme.radius_small,
    visible = function() return tile() ~= nil end,
    color = function() return hovered:get() and C.islandSurfaceHover or "#00000000" end,
    controls.hit { hovered = hovered, on_click = function()
      local t = tile()
      if t then service.toggle_tile_in(key(), t.key) end
    end },
    ui.Row {
      anchors = { left = true, left_margin = 6, vertical_center = true }, gap = 8, align = "center",
      ui.Rect {
        width = 14, height = 14, radius = 4,
        color = function() return on() and C.accent() or "#00000000" end,
        border_width = 1.5,
        border_color = function() return on() and C.accent() or C.textMuted() end,
        kit.glyph { anchors = { center_in = true }, glyph = "󰄬", size = 9, color = C.accentText,
          visible = on },
      },
      kit.glyph { width = 16, size = 12,
        glyph = function() local t = tile() return t and t.icon() or "" end,
        color = function() return on() and C.accent() or C.textMuted() end },
      kit.text {
        width = function() return INNER - 12 - 46 - (on() and 44 or 0) end, elide = "right",
        text = function() local t = tile() return t and t.label or "" end,
        size = theme.size.label,
        color = function() return on() and C.text() or C.textMuted() end,
      },
    },
    ui.Row {
      anchors = { right = true, right_margin = 4, vertical_center = true }, gap = 2,
      visible = on,
      arrow("󰅃", -1), arrow("󰅀", 1),
    },
  }
end

local function section(text, visible)
  return kit.text { text = text, size = theme.size.label, weight = 600, color = C.textMuted,
    visible = visible }
end

--- The card over the board, `board_w` by `board_h`.
function M.build(board_w, board_h)
  local ticks = { gap = 2, width = INNER, visible = on_toggles }
  for index = 1, #service.tile_catalogue do ticks[#ticks + 1] = tick_row(index) end
  local remove = controls.pill { text = "Remove", height = 26, on_click = function() service.remove(key()) end }
  local column = ui.Column {
    x = M.PAD, y = M.PAD, width = INNER, gap = 12,
    ui.Item {
      width = INNER, height = 28,
      kit.text {
        anchors = { left = true, vertical_center = true },
        text = function() local e = service.entry(block_id()) return e and e.name or "" end,
        size = theme.size.medium, weight = 600,
      },
      ui.Item {
        anchors = { right = true, vertical_center = true },
        width = function() return remove.layout_width or 70 end, height = 26,
        remove,
      },
    },
    controls.hairline { width = INNER },
    section("Size"),
    size_tiles(),
    section("Toggles", on_toggles),
    ui.Column(ticks),
  }
  local height = function() return (column.layout_height or 0) + 2 * M.PAD end
  local box = function()
    local b = block()
    return b and service.geometry(b) or { x = 0, y = 0, width = 0, height = 0 }
  end
  local right_fits = function() local g = box() return g.x + g.width + M.GAP + M.WIDTH <= board_w end
  local left_fits = function() local g = box() return g.x - M.GAP - M.WIDTH >= 0 end
  return ui.Rect {
    x = function()
      local g = box()
      if right_fits() then return g.x + g.width + M.GAP end
      if left_fits() then return g.x - M.GAP - M.WIDTH end
      return math.max(0, math.min(board_w - M.WIDTH, g.x))
    end,
    y = function()
      local g = box()
      local want = (right_fits() or left_fits()) and g.y or (g.y + g.height + M.GAP)
      return math.max(0, math.min(board_h - height(), want))
    end,
    width = M.WIDTH, height = height,
    radius = theme.radius_large, color = C.island, border_width = 1, border_color = C.islandBorder,
    behavior = { x = theme.behave("medium"), y = theme.behave("medium") },
    -- Takes every press on the card, or the ground under it would close
    -- the inspector.
    ui.MouseArea { anchors = { fill = true }, accepted_buttons = { "left", "right" } },
    column,
  }
end

return M
