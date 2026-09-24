-- A module's chip on the bar: its mark and its figure.
--
-- Port of ChipFace.qml and BarChip.qml. The mark is the module's symbol or,
-- in the ring shape (`settings.chipShape = "ring"`, for the modules that
-- measure something), its own gauge; the figure is `modules.value_of`,
-- shown always, never or under the pointer (`settings.chipFigure`). A
-- click opens the module's detail in the island; the chip stays lit while
-- it is open.
--
-- The chip's width follows the reveal on the fast curve, and the figure is
-- clipped to the width opened so far, so a half-open chip never shows half
-- a word the wrong way.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local modules = require("services.modules")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color
local chip = {}

local count = 0

--- The chip face alone: mark and figure, `reveal` a function 0..1.
--- `alone` draws it as its own capsule (larger ring, wider padding).
function chip.face(id, options)
  options = options or {}
  count = count + 1
  local alone = options.alone
  local reveal = options.reveal or function() return 1 end
  local shape = function() return modules.shape_of(id, options.shape) end
  local ring = function() return shape() == "ring" end
  local size = function() return math.floor(theme.capsule_height() * 0.44 + 0.5) end
  local pad = alone and 11 or 8
  local spacing = 5
  local gap = alone and 9 or 7
  local inset = function() return theme.capsule_height() * 0.075 end
  local limit = modules.figure_limit(id)

  local figure = kit.text {
    text = function() return modules.value_of(id) end,
    size = theme.size.small, weight = 600,
    elide = limit > 0 and "right" or nil,
    layout = limit > 0 and { maximum_width = limit } or nil,
  }
  local figured = function() return modules.value_of(id) ~= "" end
  local figure_w = function() return figured() and (figure.layout_width or 0) or 0 end

  local symbol = kit.glyph {
    glyph = function() return modules.glyph_of(id) end,
    size = size,
    color = function() return modules.tint_of(id) end,
    behavior = { color = theme.behave("fast") },
  }
  local mark_w = function()
    if ring() then return theme.capsule_height() end
    return symbol.layout_width or size()
  end

  local provider = modules.providers[id] or {}
  local gauge = ui.Item {
    width = function() return theme.capsule_height() end,
    height = function() return theme.capsule_height() end,
    anchors = { vertical_center = true },
    visible = ring,
    scale = function()
      if alone then return 1 - 0.15 * (figured() and reveal() or 0) end
      return 0.85
    end,
    ui.Loader {
      anchors = { fill = true },
      active = ring,
      source = function() return provider.chip and provider.chip() or ui.Item {} end,
    },
  }

  local width = function()
    local r = reveal()
    if ring() then
      return mark_w() + (figured() and (figure_w() + 2 * gap - inset()) or 0) * r
    end
    return pad + mark_w() + (figured() and (spacing + figure_w()) or 0) * r + pad
  end

  local node = ui.Item {
    width = width,
    behavior = { width = theme.behave("fast") },
    height = function() return theme.capsule_height() end,
    ui.Item {
      x = pad, anchors = { vertical_center = true },
      visible = function() return not ring() end,
      width = mark_w, height = function() return size() + 2 end,
      ui.Item { anchors = { center_in = true }, width = mark_w, height = function() return size() + 2 end,
        ui.Item { anchors = { vertical_center = true }, width = mark_w, height = function() return size() + 4 end,
          symbol } },
    },
    gauge,
    ui.ClipRect {
      x = function()
        if ring() then return mark_w() - inset() + gap end
        return pad + mark_w() + spacing
      end,
      y = 0,
      color = "#00000000",
      width = function() return math.max(1, figure_w() * reveal()) end,
      height = function() return theme.capsule_height() end,
      visible = function() return figured() and reveal() > 0.01 end,
      behavior = { width = theme.behave("fast") },
      ui.Item {
        anchors = { vertical_center = true },
        width = function() return figure_w() end, height = 16,
        opacity = function() return math.max(0, (reveal() - 0.2) / 0.8) end,
        ui.Item { anchors = { vertical_center = true }, height = 16, figure },
      },
    },
  }
  return node
end

--- A piece for the bar: the face, a highlight under the pointer and while
--- the detail is open, and the click that opens it.
function chip.piece(id, options)
  options = options or {}
  local shows = function() return modules.shows(id, options.when) end
  -- Built only while the module shows: a hidden child still takes its room
  -- in a Row, and a width of zero means "no width", so an absent module is
  -- an inactive loader, which has no size at all.
  return ui.Loader {
    active = shows,
    source = function() return chip.chip(id, options) end,
  }
end

function chip.chip(id, options)
  local hovered = controls.signal("chip." .. id, false)
  local reveal_target = function()
    local figure = modules.figure_of(options.figure)
    if figure == "on" then return 1 end
    if figure == "hover" and hovered:get() then return 1 end
    return 0
  end
  -- The reveal is animated by the face's width behaviour; the value here is
  -- the destination, so the figure and the width move together.
  local face = chip.face(id, { reveal = reveal_target, alone = options.alone, shape = options.shape })
  local open = function()
    return modules.open_id:get() == id and require("bar.island_state").open_panel() == "module"
  end
  return ui.Item {
    width = function() return face.layout_width or 1 end,
    height = function() return theme.capsule_height() end,
    ui.Rect {
      anchors = { center_in = true },
      width = function() return math.max(0, (face.layout_width or 0) - (options.alone and 2 or 4)) end,
      height = function() return options.alone and theme.capsule_height() - 2 or theme.capsule_height() - 8 end,
      radius = function() return (options.alone and theme.capsule_height() - 2 or theme.capsule_height() - 8) / 2 end,
      color = C.islandSurfaceHover,
      opacity = function() return (hovered:get() or open()) and 1 or 0 end,
      behavior = { opacity = theme.behave("fast") },
    },
    face,
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() modules.activate(id) end,
    },
  }
end

--- A bar button: a door's glyph that toggles its panel, lit while open.
function chip.button(id)
  local hovered = controls.signal("button." .. id, false)
  local door = function() return modules.buttons()[id] end
  return ui.Item {
    width = function() return theme.capsule_height() end,
    height = function() return theme.capsule_height() end,
    ui.Rect {
      anchors = { center_in = true },
      width = function() return theme.capsule_height() - 4 end,
      height = function() return theme.capsule_height() - 8 end,
      radius = function() return (theme.capsule_height() - 8) / 2 end,
      color = C.islandSurfaceHover,
      opacity = function()
        local d = door()
        return (hovered:get() or (d and d.panel and modules.shown_panel() == d.panel)) and 1 or 0
      end,
      behavior = { opacity = theme.behave("fast") },
    },
    kit.glyph {
      anchors = { center_in = true },
      glyph = function() local d = door() return d and d.glyph or "" end,
      size = function() return math.floor(theme.capsule_height() * 0.44 + 0.5) end,
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function()
        local d = door()
        if not d then return end
        if d.panel then modules.toggle_panel(d.panel) elseif d.action then d.action() end
      end,
    },
  }
end

return chip
