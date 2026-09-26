-- The registry of control centre blocks: an id and a size in, a card out.
--
-- Port of BlockFace.qml. A face is told its cells and its pixels, so a
-- block with several faces picks one. Every block of the catalogue has a
-- face here; one added later registers its own with `blocks.register(id,
-- build)`, and until it does it shows its symbol and name.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local kit = require("components.kit")
local controls = require("components.controls")
local service = require("services.controls")
local audio = require("services.audio")
local brightness = require("services.brightness")

local C = theme.color
local M = { faces = {} }

--- `build(options)`: options are `key`, `id`, `size`, `cols`, `rows`,
--- `width`, `height`, `on_panel(name)`, `on_dismiss()`.
function M.register(id, build) M.faces[id] = build end

M.register("toggles", function(o)
  return require("bar.controls.toggles").build(o)
end)

-- Sliders are shorter than a cell and centred in it.
M.register("volume", function(o)
  return ui.Item {
    width = o.width, height = o.height,
    controls.slider_row {
      anchors = { vertical_center = true },
      width = o.width, height = math.min(o.height, 48),
      icon = audio.icon, value = audio.volume, available = audio.ready, dimmed = audio.muted,
      on_moved = audio.set_volume, on_icon = audio.toggle_mute,
    },
  }
end)

M.register("brightness", function(o)
  brightness.refresh()
  return ui.Item {
    width = o.width, height = o.height,
    controls.slider_row {
      anchors = { vertical_center = true },
      width = o.width, height = math.min(o.height, 48),
      icon = brightness.icon, value = brightness.percent, from = 1,
      available = brightness.available, on_moved = brightness.set_percent,
    },
  }
end)

M.register("media", function(o)
  return require("bar.controls.media_card").build(o)
end)

M.register("weather", function(o)
  return require("bar.controls.weather_card").build(o)
end)

M.register("tasks", function(o) return require("bar.controls.tasks_block").build(o) end)
M.register("pet", function(o) return require("bar.controls.pet_block").build(o) end)
M.register("games", function(o) return require("bar.controls.games_block").build(o) end)
M.register("impasto", function(o) return require("bar.controls.impasto_block").build(o) end)

-- A note with no card around it: the block's own note, else the newest.
-- Clicking opens it for editing (BlockFace.qml:166-191).
M.register("notes", function(o)
  local notes = require("services.notes")
  local note = function()
    local row = service.entry_of(o.key)
    local named = row and row.note and notes.entry(row.note)
    if named and not named.archived then return named end
    return notes.newest()
  end
  local hovered = controls.signal("notes.block", false)
  return ui.Item {
    width = o.width, height = o.height,
    require("components.sticky").build {
      note = note, width = o.width, height = o.height,
      placeholder = "No notes yet",
      padding = o.cols == 1 and 12 or 14,
      title_size = o.rows >= 4 and theme.size.regular or theme.size.small,
      body_size = o.rows >= 4 and 20 or 16,
    },
    controls.hit { hovered = hovered, on_click = function()
      local n = note()
      notes.open(n and n.key or "")
      o.on_panel("notes")
    end },
  }
end)

M.register("calendar", function(o)
  return require("bar.controls.calendar_card").build(o)
end)

M.register("notifications", function(o)
  return require("bar.controls.notification_list").build(o)
end)

-- The clock. Square: time over the day. Wide: larger time over the full
-- date. Large square: the same, bigger.
M.register("clock", function(o)
  local tall = o.rows >= 4
  local wide = o.cols >= 2 and not tall
  local clock = require("bar.modules.clock")
  local nominal = tall and 64 or (wide and 44 or 30)
  local inner = o.width - 28
  -- Scaled down rather than clipped: with seconds on, the time is wider
  -- than a square. Measured at its own size by an unseen twin.
  local twin = kit.text { text = clock.text, size = nominal, weight = 600, opacity = 0 }
  local fitted = function()
    local w = twin.layout_width or 0
    if w <= inner or w <= 0 then return nominal end
    return math.max(theme.size.large, math.floor(nominal * inner / w))
  end
  return controls.card {
    width = o.width, height = o.height,
    ui.Item { x = 0, y = 0, width = 1, height = 1, ui.ClipRect { width = 1, height = 1,
      color = "#00000000", twin } },
    ui.Column {
      anchors = { center_in = true }, gap = tall and 6 or 2, align = "center",
      kit.text { text = clock.text, size = fitted, weight = 600,
        horizontal_alignment = "center" },
      kit.text {
        text = function()
          morf.hour_clock:get()
          return morf.time.format((wide or tall) and "%A %-d %B" or "%a %-d %b")
        end,
        size = tall and theme.size.medium or theme.size.small, color = C.textMuted,
        horizontal_alignment = "center",
      },
    },
  }
end)

-- The wallpaper, with the palette under it; it opens the appearance panel.
M.register("appearance", function(o)
  local hovered = controls.signal("appearance.hover", false)
  local wallpaper = require("services.wallpaper")
  local thumbnails = require("services.thumbnails")
  local themes = require("services.theme")
  local path = function()
    local p = wallpaper.current:get()
    if p == "" then p = settings.wallpaper or "" end
    if p:sub(1, 1) == "~" then p = morf.fs.home() .. p:sub(2) end
    return p
  end
  -- A 560 x 320 copy rather than the full picture (AppearanceCard.qml:42-43).
  local picture = function()
    local p = path()
    return p ~= "" and thumbnails.of(p, 560, 320) or ""
  end
  -- The palette's name under the title, as Theme.activeName.
  local palette_name = function()
    local id = themes.active_id:get()
    for _, entry in ipairs(themes.available()) do
      if entry.id == id then return entry.name end
    end
    return id
  end
  -- The palette as ColorSwatch draws it: a two by two grid of the
  -- preset's swatches, or of the adaptive palette's ground, surface,
  -- accent and type (AppearanceCard.qml, ThemeService.adaptiveSwatches).
  local palette_colors = function()
    local id = themes.active_id:get()
    for _, preset in ipairs(require("lib.palette").presets) do
      if preset.id == id and preset.swatches then return preset.swatches end
    end
    return { C.background(), C.surface(), C.accent(), C.text() }
  end
  local chevron = kit.glyph {
    glyph = "󰅂", size = 14,
    color = function() return hovered:get() and C.accent() or C.scrimText end,
    opacity = function() return hovered:get() and 1 or 0.7 end,
    behavior = { color = theme.behave("fast") },
  }
  local swatch_row = require("components.swatch").colors { size = 11, colors = palette_colors }
  local text_w = function()
    return o.width - 24 - (swatch_row.layout_width or 63) - 10 - (chevron.layout_width or 14) - 10
  end
  return ui.ClipRect {
    width = o.width, height = o.height, radius = theme.radius_medium,
    color = C.islandSurface,
    ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
      source = picture, visible = function() return picture() ~= "" end,
      scale = function() return hovered:get() and 1.04 or 1 end,
      behavior = { scale = theme.behave("medium") },
    },
    kit.glyph { anchors = { center_in = true }, glyph = "󰏘", size = 28, color = C.textMuted,
      visible = function() return picture() == "" end },
    -- The label gets its own ground, since it sits on a photograph.
    ui.Rect {
      anchors = { left = true, right = true, bottom = true }, height = 52,
      gradient = function()
        return { angle = 180, stops = { "#00000000", { theme.color.scrim, 0.45 }, theme.color.island } }
      end,
    },
    ui.Row {
      anchors = { left = true, bottom = true, left_margin = 12, bottom_margin = 12 },
      gap = 10, align = "center",
      swatch_row,
      ui.Column {
        gap = 0,
        kit.text { text = "Appearance", size = theme.size.small, weight = 600, color = C.scrimText,
          width = text_w, elide = "right" },
        kit.text { text = palette_name, size = theme.size.label, color = C.scrimText, opacity = 0.7,
          width = text_w, elide = "right" },
      },
      chevron,
    },
    ui.Rect { anchors = { fill = true }, radius = theme.radius_medium, color = "#00000000",
      border_width = 1,
      border_color = function() return hovered:get() and C.accent() or C.islandBorder end,
      behavior = { border_color = theme.behave("fast") } },
    controls.hit { hovered = hovered, on_click = function() o.on_panel("appearance") end },
  }
end)

--- A block whose face belongs to a part not ported yet: its symbol and
--- its name, quietly.
function M.placeholder(o)
  local entry = service.entry(o.id) or { icon = "", name = o.id }
  return controls.card {
    width = o.width, height = o.height,
    ui.Column {
      anchors = { center_in = true }, gap = 6, align = "center",
      kit.glyph { glyph = entry.icon, size = 22, color = C.textMuted },
      kit.text { text = entry.name, size = theme.size.small, color = C.textMuted },
    },
  }
end

function M.build(o)
  local face = M.faces[o.id]
  if face then return face(o) end
  return M.placeholder(o)
end

return M
