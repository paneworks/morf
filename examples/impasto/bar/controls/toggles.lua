-- The toggles block: TogglesBlock.qml. A paged grid of quick tiles, as many
-- per page as the block has cells, laid out on the grid's own cells and
-- gutters. Pages turn with the wheel (one detent a page), a sideways drag,
-- the dots or the chevrons beside them.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local service = require("services.controls")

local C = theme.color
local M = {}

--- `key` is the block's row (its own tile list), `cols` x `rows` its cells,
--- `width` x `height` its pixels; `on_panel(name)` opens a tile's list,
--- `on_dismiss()` closes the centre for a one-shot action.
function M.build(options)
  local tiles = service.tiles_of(options.key or "")
  local cols, rows = options.cols, options.rows
  local per_page = math.max(1, cols * rows)
  local pages = math.max(1, math.ceil(#tiles / per_page))
  local paged = pages > 1
  local lane = paged and theme.centre_pager_lane or 0
  local width, height = options.width, options.height
  local page_h = height - lane
  local tile_w = (width - (cols - 1) * theme.centre_gutter) / cols
  local tile_h = (page_h - (rows - 1) * theme.centre_gutter) / rows
  local page = controls.signal("toggles.page", 0)

  local strip = { x = function() return -page:get() * width end, y = 0,
    width = width * pages, height = page_h,
    behavior = { x = theme.behave("morph") } }
  for index, tile in ipairs(tiles) do
    local p = (index - 1) // per_page
    local slot = (index - 1) % per_page
    strip[#strip + 1] = controls.quick_tile {
      x = p * width + (slot % cols) * (tile_w + theme.centre_gutter),
      y = (slot // cols) * (tile_h + theme.centre_gutter),
      width = tile_w, height = tile_h,
      icon = tile.icon, label = tile.label, detail = tile.detail,
      active = tile.active, available = tile.available, expandable = tile.expandable,
      on_toggled = function()
        service.activate(tile)
        if tile.closes and options.on_dismiss then options.on_dismiss() end
      end,
      on_expanded = function()
        if tile.panel ~= "" and options.on_panel then options.on_panel(tile.panel) end
      end,
    }
  end

  local function go(to) page:set(math.max(0, math.min(pages - 1, to))) end

  local children = {
    width = width, height = height,
    -- Beneath the tiles: the wheel and a sideways swipe turn the page.
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_wheel = function(_, _, _, _, step_x, step_y)
        local steps = (step_y ~= 0 and step_y) or step_x or 0
        if paged and steps ~= 0 then go(page:get() + (steps > 0 and 1 or -1)) end
      end,
      on_drag_finished = function(_, _, dx)
        if not paged or not dx then return end
        if dx < -40 then go(page:get() + 1) elseif dx > 40 then go(page:get() - 1) end
      end,
    },
    ui.ClipRect { x = 0, y = 0, width = width, height = page_h, color = "#00000000",
      ui.Item(strip) },
  }

  if paged then
    local dots = { anchors = { horizontal_center = true, bottom = true }, gap = 2, align = "center",
      height = lane }
    local function step(glyph, delta, usable)
      local hovered = controls.signal("toggles.step", false)
      return ui.Item {
        width = 18, height = lane,
        kit.glyph { anchors = { center_in = true }, glyph = glyph, size = 11,
          color = function() return hovered:get() and C.text() or C.textMuted() end,
          opacity = function() return usable() and 1 or 0 end,
          behavior = { opacity = theme.behave("fast"), color = theme.behave("fast") } },
        controls.hit { hovered = hovered, enabled = usable, on_click = function() go(page:get() + delta) end },
      }
    end
    dots[#dots + 1] = step("󰅁", -1, function() return page:get() > 0 end)
    for index = 0, pages - 1 do
      local hovered = controls.signal("toggles.dot", false)
      dots[#dots + 1] = ui.Item {
        width = 14, height = lane,
        ui.Rect { anchors = { center_in = true }, width = 6, height = 6, radius = 3,
          color = function()
            if page:get() == index then return C.text() end
            return hovered:get() and C.textMuted() or C.islandBorder
          end,
          behavior = { color = theme.behave("fast") } },
        controls.hit { hovered = hovered, on_click = function() go(index) end },
      }
    end
    dots[#dots + 1] = step("󰅂", 1, function() return page:get() < pages - 1 end)
    children[#children + 1] = ui.Row(dots)
  end
  return ui.Item(children)
end

return M
