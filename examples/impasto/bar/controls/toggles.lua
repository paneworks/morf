-- The toggles block: TogglesBlock.qml. A paged grid of quick tiles, as many
-- per page as the block has cells, laid out on the grid's own cells and
-- gutters. Pages turn with the wheel (one detent a page), a sideways drag
-- (starting on a tile or between them), the dots or the chevrons beside
-- them.
--
-- The tiles follow the block's own list as it changes (the inspector ticks
-- and orders them): every slot the catalogue could fill is built once and
-- reads its tile by index, so nothing is rebuilt and the page is kept.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local service = require("services.controls")

local C = theme.color
local M = {}

-- A sideways drag past this many pixels turns the page.
M.SWIPE = 40

local fast = function() return theme.behave("fast") end

--- One tile, as QuickTile draws it, whose press also takes a swipe: a tap
--- toggles, a drag sideways turns the page. `tile()` is the catalogue row
--- or nil.
local function tile_node(values)
  local tile = values.tile
  local hovered = controls.signal("toggles.tile", false)
  local chevron_hover = controls.signal("toggles.chevron", false)
  local field = function(name, fallback)
    return function()
      local t = tile()
      if not t then return fallback end
      return t[name]()
    end
  end
  local active = field("active", false)
  local available = field("available", false)
  local expandable = field("expandable", false)
  local width, height = values.width, values.height
  local text_w = function() return math.max(10, width - 12 - 34 - 11 - 12 - (expandable() and 16 or 0)) end
  return ui.Rect {
    x = values.x, y = values.y, width = width, height = height,
    visible = function() return tile() ~= nil end,
    radius = theme.radius_medium,
    opacity = function() return available() and 1 or 0.45 end,
    color = function() return hovered:get() and C.islandSurfaceHover or C.islandSurface end,
    border_width = 1,
    border_color = function() return active() and C.accent() or C.islandBorder end,
    behavior = { color = fast(), border_color = fast(), opacity = fast() },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = function() return available() and "pointer" or "default" end,
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function()
        local t = tile()
        if t and available() and values.on_toggled then values.on_toggled(t) end
      end,
      on_drag_finished = function(_, _, dx) values.on_swipe(dx) end,
    },
    ui.Row {
      anchors = { left = true, left_margin = 12, vertical_center = true },
      gap = 11, align = "center",
      ui.Rect {
        width = 34, height = 34, radius = 17,
        color = function() return active() and C.accent() or C.islandSurfaceHover end,
        behavior = { color = fast() },
        kit.glyph {
          anchors = { center_in = true }, glyph = field("icon", ""), size = 16,
          color = function() return active() and C.accentText() or C.textMuted() end,
          behavior = { color = fast() },
        },
      },
      ui.Column {
        gap = 1,
        kit.text {
          text = function() local t = tile() return t and t.label or "" end,
          size = theme.size.small, weight = 600, elide = "right", width = text_w,
        },
        kit.text {
          text = field("detail", ""), size = theme.size.label, elide = "right", width = text_w,
          color = function() return active() and C.accent() or C.textMuted() end,
          behavior = { color = fast() },
        },
      },
    },
    -- Last, so it sits above the tile's own area.
    ui.Item {
      anchors = { right = true, top = true, bottom = true }, width = 28,
      visible = expandable,
      kit.glyph {
        anchors = { center_in = true }, glyph = "󰅂", size = 13,
        color = function() return chevron_hover:get() and C.accent() or C.textMuted() end,
        behavior = { color = fast() },
      },
      controls.hit { hovered = chevron_hover, enabled = available, on_click = function()
        local t = tile()
        if t and values.on_expanded then values.on_expanded(t) end
      end },
    },
  }
end

--- `key` is the block's row (its own tile list), `cols` x `rows` its cells,
--- `width` x `height` its pixels; `on_panel(name)` opens a tile's list,
--- `on_dismiss()` closes the centre for a one-shot action.
function M.build(options)
  local key = options.key or ""
  local tiles = function() return service.tiles_of(key) end
  local cols, rows = options.cols, options.rows
  local per_page = math.max(1, cols * rows)
  local width, height = options.width, options.height
  local slots = #service.tile_catalogue
  local most_pages = math.max(1, math.ceil(slots / per_page))
  local pages = function() return math.max(1, math.ceil(#tiles() / per_page)) end
  local paged = function() return pages() > 1 end
  local lane = theme.centre_pager_lane
  -- The pager takes its lane only while there is more than one page.
  local page_h = function() return height - (paged() and lane or 0) end
  local tile_w = (width - (cols - 1) * theme.centre_gutter) / cols
  local tile_h = function() return (page_h() - (rows - 1) * theme.centre_gutter) / rows end
  local page = controls.signal("toggles.page", 0)
  local function go(to) page:set(math.max(0, math.min(pages() - 1, to))) end
  local function swipe(dx)
    if not paged() or not dx then return end
    if dx < -M.SWIPE then go(page:get() + 1) elseif dx > M.SWIPE then go(page:get() - 1) end
  end
  -- A list that shrank keeps the page on something.
  local current = function() return math.min(page:get(), pages() - 1) end

  local strip = { x = function() return -current() * width end, y = 0,
    width = width * most_pages, height = page_h,
    behavior = { x = theme.behave("morph") } }
  for index = 1, slots do
    local p = (index - 1) // per_page
    local slot = (index - 1) % per_page
    strip[#strip + 1] = tile_node {
      tile = function() return tiles()[index] end,
      x = p * width + (slot % cols) * (tile_w + theme.centre_gutter),
      y = function() return (slot // cols) * (tile_h() + theme.centre_gutter) end,
      width = tile_w, height = tile_h,
      on_swipe = swipe,
      on_toggled = function(t)
        service.activate(t)
        if t.closes and options.on_dismiss then options.on_dismiss() end
      end,
      on_expanded = function(t)
        if t.panel ~= "" and options.on_panel then options.on_panel(t.panel) end
      end,
    }
  end

  local children = {
    width = width, height = height,
    -- Beneath the tiles: the wheel and a sideways swipe between tiles.
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_wheel = function(_, _, _, _, step_x, step_y)
        local steps = (step_y ~= 0 and step_y) or step_x or 0
        if paged() and steps ~= 0 then go(current() + (steps > 0 and 1 or -1)) end
      end,
      on_drag_finished = function(_, _, dx) swipe(dx) end,
    },
    ui.ClipRect { x = 0, y = 0, width = width, height = page_h, color = "#00000000",
      ui.Item(strip) },
  }

  if per_page < slots then
    local dots = { anchors = { horizontal_center = true, bottom = true }, gap = 2, align = "center",
      height = lane, visible = paged }
    local function step(glyph, delta, usable)
      local hovered = controls.signal("toggles.step", false)
      return ui.Item {
        width = 18, height = lane,
        kit.glyph { anchors = { center_in = true }, glyph = glyph, size = 11,
          color = function() return hovered:get() and C.text() or C.textMuted() end,
          opacity = function() return usable() and 1 or 0 end,
          behavior = { opacity = theme.behave("fast"), color = theme.behave("fast") } },
        controls.hit { hovered = hovered, enabled = usable, on_click = function() go(current() + delta) end },
      }
    end
    dots[#dots + 1] = step("󰅁", -1, function() return current() > 0 end)
    for index = 0, most_pages - 1 do
      local hovered = controls.signal("toggles.dot", false)
      dots[#dots + 1] = ui.Item {
        width = 14, height = lane,
        visible = function() return index < pages() end,
        ui.Rect { anchors = { center_in = true }, width = 6, height = 6, radius = 3,
          color = function()
            if current() == index then return C.text() end
            return hovered:get() and C.textMuted() or C.islandBorder
          end,
          behavior = { color = theme.behave("fast") } },
        controls.hit { hovered = hovered, on_click = function() go(index) end },
      }
    end
    dots[#dots + 1] = step("󰅂", 1, function() return current() < pages() - 1 end)
    children[#children + 1] = ui.Row(dots)
  end
  return ui.Item(children)
end

return M
