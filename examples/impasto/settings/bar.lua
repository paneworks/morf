-- Settings, Bar & Island: how the bar is drawn and what the island shows at
-- rest, the pieces either side of it, the workspaces, and notifications
-- (BarSection and ModulesPart). Notifications live here because the shell
-- is the notification daemon and a notification takes over the island.
--
-- The bar itself is the preview for the sizes: it repaints over this window
-- as the sliders move.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local modules = require("services.modules")
local notifications = require("services.notifications")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local bar_preview = require("components.bar_preview")
local chip = require("bar.modules.chip")
local layout_editor = require("settings.layout_editor")

local C = theme.color

local M = {}

M.styles = {
  { id = "grouped", label = "Grouped", note = "The workspaces, the island and the modules together in the middle." },
  { id = "spread", label = "Spread", note = "The workspaces at one edge, the modules at the other, the island between them." },
  { id = "capsule", label = "One island", note = "Everything inside a single capsule." },
}
M.clock_formats = {
  { id = "%H:%M", label = "24-hour" },
  { id = "%I:%M %p", label = "12-hour" },
}
M.chip_shapes = {
  { id = "icon", label = "Icon", note = "The module's symbol." },
  { id = "ring", label = "Ring", note = "A gauge around the symbol, for the modules that measure something." },
}
M.chip_figures = {
  { id = "off", label = "No", note = "No figure beside it." },
  { id = "hover", label = "On hover", note = "The figure while the pointer is on the chip." },
  { id = "on", label = "Always", note = "The figure always beside it." },
}
M.beside_defaults = { "timer", "media" }

local function note_of(list, id)
  for _, entry in ipairs(list) do if entry.id == id then return entry.note end end
  return ""
end

function M.beside(id)
  local kept = settings.islandActivities
  local list = type(kept) == "table" and kept or M.beside_defaults
  for _, each in ipairs(list) do if each == id then return true end end
  return false
end

function M.set_beside(id, on)
  local kept = settings.islandActivities
  local list = type(kept) == "table" and kept or M.beside_defaults
  local next = {}
  for _, each in ipairs(list) do if each ~= id then next[#next + 1] = each end end
  if on then next[#next + 1] = id end
  settings.set("islandActivities", next)
end

-- ------------------------------------------------------------- the island --

local function island_part(W)
  local style_tiles = {}
  for _, style in ipairs(M.styles) do
    style_tiles[#style_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, caption = style.label,
        selected = function() return settings.barStyle == style.id end,
        on_picked = function() settings.set("barStyle", style.id) end,
        stage = function()
          return bar_preview { anchors = { center_in = true }, style = style.id,
            attached = function() return settings.islandAttached end }
        end,
      }
    end
  end
  local attach_tiles = {}
  for _, choice in ipairs { { false, "Floating" }, { true, "Notch" } } do
    attach_tiles[#attach_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, caption = choice[2],
        selected = function() return settings.islandAttached == choice[1] end,
        on_picked = function() settings.set("islandAttached", choice[1]) end,
        stage = function()
          return bar_preview { anchors = { center_in = true }, attached = choice[1],
            style = function() return settings.barStyle end }
        end,
      }
    end
  end
  local format_tiles = {}
  for _, format in ipairs(M.clock_formats) do
    format_tiles[#format_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, caption = format.label,
        selected = function() return settings.clockFormat == format.id end,
        on_picked = function() settings.set("clockFormat", format.id) end,
        stage = function()
          return kit.text {
            anchors = { center_in = true }, size = theme.size.large, weight = 600,
            text = function()
              morf.clock:get()
              local pattern = format.id
              if settings.clockShowsSeconds then pattern = pattern:gsub("%%M", "%%M:%%S", 1) end
              return morf.time.format(pattern)
            end,
          }
        end,
      }
    end
  end
  local beside_rows = {
    width = W, title = "Beside the time",
    note = "What is running sits either side of the time, two at most.",
    hint = "A recording is always there and comes first. Then a countdown, then media; either still works from its chip on the bar when kept off the island.",
  }
  for _, id in ipairs(M.beside_defaults) do
    beside_rows[#beside_rows + 1] = setting.switch_row {
      width = W, label = modules.entry(id).name,
      reading = function() return M.beside(id) and "On the island while it runs" or "Only where its chip is put" end,
      checked = function() return M.beside(id) end,
      on_toggled = function(on) M.set_beside(id, on) end,
    }
  end
  local screens = #(morf.screens or {})
  return {
    setting.group {
      width = W, title = "Shape",
      note = "How the bar is drawn, and where the island meets the top edge.",
      hint = "The style keeps what is on the bar and only changes how it is drawn. What each side carries is arranged in The bar.",
      setting.tiles { width = W, label = "Style", tiles = style_tiles,
        reading = function() return note_of(M.styles, settings.barStyle) end },
      setting.tiles { width = W, label = "Island", tiles = attach_tiles,
        reading = function()
          return settings.islandAttached and "Cut into the top edge" or "Floating below the top edge"
        end },
      setting.switch_row { width = W, label = "Span the whole screen",
        reading = function()
          return settings.barFullWidth and "As wide as the bar can be" or "As wide as the island needs"
        end,
        locked = function() return settings.barStyle ~= "capsule" end,
        reason = "Only one island can span the screen",
        checked = function() return settings.barFullWidth end,
        on_toggled = function(on) settings.set("barFullWidth", on) end },
      setting.switch_row { width = W, label = "On every screen",
        reading = function()
          return settings.barEverywhere and "One on each, and the one you are on is the live one"
            or "Only on the screen you are on"
        end,
        locked = screens < 2, reason = "Only one screen is on",
        checked = function() return settings.barEverywhere end,
        on_toggled = function(on) settings.set("barEverywhere", on) end },
      setting.switch_row { width = W, label = "A glance on hover",
        reading = function()
          return settings.islandSummary and "Resting the pointer on the island opens it" or "Only a click opens anything"
        end,
        checked = function() return settings.islandSummary end,
        on_toggled = function(on) settings.set("islandSummary", on) end },
    },
    setting.group {
      width = W, title = "Clock",
      note = "Shown on the resting island, and larger in the glance.",
      hint = "Seconds make the clock repaint sixty times as often.",
      setting.tiles { width = W, label = "Clock format", tiles = format_tiles },
      setting.switch_row { width = W, label = "Show the date",
        reading = function() return settings.clockShowsDate and "Beside the time" or "The time alone" end,
        checked = function() return settings.clockShowsDate end,
        on_toggled = function(on) settings.set("clockShowsDate", on) end },
      setting.switch_row { width = W, label = "Show seconds",
        checked = function() return settings.clockShowsSeconds end,
        on_toggled = function(on) settings.set("clockShowsSeconds", on) end },
    },
    setting.group(beside_rows),
    setting.group {
      width = W, title = "Scale",
      note = "The bar itself is the preview: it repaints as the sliders move.",
      hint = "Everything on the bar scales with its height. The top margin is the gap to the screen edge (the island ignores it in notch mode), and the side margin is the inset from the left and right edges.",
      setting.slider { width = W, label = "Bar height", from = 24, to = 48, unit = " px",
        value = function() return settings.barHeight end,
        on_moved = function(v) settings.set("barHeight", v) end },
      setting.slider { width = W, label = "Top margin", from = 0, to = 32, unit = " px",
        value = function() return settings.barMargin end,
        on_moved = function(v) settings.set("barMargin", v) end },
      setting.slider { width = W, label = "Side margin", from = 0, to = 48, unit = " px",
        value = function() return settings.barSideMargin end,
        on_moved = function(v) settings.set("barSideMargin", v) end },
    },
  }
end

-- ---------------------------------------------------------------- the bar --

--- A capsule of real chip faces at a shape and figure, so the preview
--- always matches the bar.
local function sample(ids, shape, reveal)
  local faces = { align = "center" }
  for _, id in ipairs(ids) do
    faces[#faces + 1] = chip.face(id, { shape = shape, reveal = reveal })
  end
  local row = ui.Row(faces)
  return kit.capsule {
    anchors = { center_in = true },
    width = function() return (row.layout_width or 0) + 8 end,
    ui.Item { x = 4, width = function() return row.layout_width or 0 end,
      height = function() return theme.capsule_height() end, row },
  }
end

local function modules_part(W)
  local shape_tiles = {}
  for _, shape in ipairs(M.chip_shapes) do
    shape_tiles[#shape_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, caption = shape.label,
        selected = function() return settings.chipShape == shape.id end,
        on_picked = function() settings.set("chipShape", shape.id) end,
        stage = function(hovered)
          return sample({ "volume", "battery", "brightness" }, shape.id, function()
            local figure = settings.chipFigure
            return (figure == "on" or (figure == "hover" and hovered:get())) and 1 or 0
          end)
        end,
      }
    end
  end
  local figure_tiles = {}
  for _, figure in ipairs(M.chip_figures) do
    figure_tiles[#figure_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, caption = figure.label,
        selected = function() return settings.chipFigure == figure.id end,
        on_picked = function() settings.set("chipFigure", figure.id) end,
        -- The hover tile opens when pointed at, which is the setting.
        stage = function(hovered)
          return sample({ "volume", "battery" }, "", function()
            return (figure.id == "on" or (figure.id == "hover" and hovered:get())) and 1 or 0
          end)
        end,
      }
    end
  end
  return {
    setting.group {
      width = W, title = "Chips",
      note = "Every piece on the bar follows these unless it was given its own.",
      hint = "Icon shows the module's symbol; Ring draws the gauge of a module that measures something as a circle around it, and the rest keep their symbol. On hover shows the figure only while the pointer is over the chip.",
      setting.tiles { width = W, label = "Shape", tiles = shape_tiles,
        reading = function() return note_of(M.chip_shapes, settings.chipShape) end },
      setting.tiles { width = W, label = "Figure", tiles = figure_tiles,
        reading = function() return note_of(M.chip_figures, settings.chipFigure) end },
    },
    setting.group {
      width = W, title = "Layout", bare = true,
      note = "Drag a piece from the catalogue onto the bar.",
      hint = "Drop a piece on either half of the bar to place it on that side of the island; drag it along to move it or off the bar to remove it, and click it to give it its own shape and figure. Adjacent modules share a capsule, and a split starts a new one.",
      layout_editor.new { width = W },
    },
  }
end

-- ------------------------------------------------------------- workspaces --

local function workspaces_part(W)
  local dots = {}
  for i = 1, 20 do
    local kept = function() return i <= settings.workspaceCount end
    dots[#dots + 1] = ui.Rect {
      y = 11,
      x = function()
        local x = 12
        for j = 1, i - 1 do x = x + (j == 1 and 22 or 6) + 8 end
        return x
      end,
      width = i == 1 and 22 or 6, height = 6, radius = 3,
      visible = function() return i <= settings.workspaceMax end,
      color = function()
        if i == 1 then return C.accent() end
        return kept() and C.indicatorDim or "#00000000"
      end,
      border_width = function() return kept() and 0 or 1 end,
      border_color = C.indicatorDim,
      opacity = function() return kept() and 1 or 0.45 end,
    }
  end
  dots.height = 28
  dots.radius = 14
  dots.color = C.island
  dots.border_width = 1
  dots.border_color = C.islandBorder
  dots.width = function() return 24 + 22 + (settings.workspaceMax - 1) * 14 end
  dots.behavior = { width = theme.behave("medium") }
  local strip = ui.Rect(dots)
  return {
    setting.group {
      width = W, title = "Workspaces",
      note = "The shown workspaces are always drawn; the rest, up to the available count, appear only while they have windows.",
      setting.block { width = W, align = "center",
        ui.Item { width = W - 28, height = 28,
          ui.Item { anchors = { center_in = true }, width = function() return strip.width end, height = 28, strip } } },
      setting.slider { width = W, label = "Workspaces shown", from = 1, to = 20,
        value = function() return settings.workspaceCount end,
        on_moved = function(v) settings.set("workspaceCount", math.min(v, settings.workspaceMax)) end },
      setting.slider { width = W, label = "Workspaces available", from = 4, to = 20,
        value = function() return settings.workspaceMax end,
        on_moved = function(v)
          settings.set("workspaceMax", v)
          -- The ceiling cannot end up below the floor.
          if settings.workspaceCount > v then settings.set("workspaceCount", v) end
        end },
    },
  }
end

-- ---------------------------------------------------------- notifications --

local function notifications_part(W)
  return {
    setting.group {
      width = W, title = "Notifications",
      note = "New notifications appear briefly in the island.",
      hint = "The shell is the notification daemon: notifications without their own timeout use the time below, and critical ones stay until dismissed. Do not disturb only keeps them off the screen; they still collect in the control centre.",
      setting.slider { width = W, label = "How long one stays", from = 2, to = 15, unit = " s",
        value = function() return math.floor(settings.notificationTimeout / 1000 + 0.5) end,
        locked = function() return settings.doNotDisturb end,
        reason = "Nothing is shown while Do not disturb is on",
        on_moved = function(v) settings.set("notificationTimeout", v * 1000) end },
      setting.switch_row { width = W, label = "Do not disturb",
        reading = function() return settings.doNotDisturb and "Nothing takes the screen" or "Everything is shown" end,
        checked = function() return settings.doNotDisturb end,
        on_toggled = function() notifications.toggle_dnd() end },
      setting.row { width = W, label = "Kept",
        reading = function()
          local count = #notifications.history()
          return count > 0 and (count .. " in this session") or "Nothing kept"
        end,
        control = controls.pill { text = "Clear", icon = "󰜉", height = 30, width = 92,
          enabled = function() return #notifications.history() > 0 end,
          on_click = function() notifications.clear() end } },
    },
  }
end

function M.build(page)
  return setting.parts(page, {
    { id = "island", build = island_part },
    { id = "modules", build = modules_part },
    { id = "workspaces", build = workspaces_part },
    { id = "notifications", build = notifications_part },
  })
end

return M
