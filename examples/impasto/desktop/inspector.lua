-- A widget's own look, on a card beside it while arranging.
--
-- Port of Inspector.qml. Its shape (the families it has faces for, the same
-- choice as pulling its corner), its face (the desk's theme, or Modern or
-- Analogue for this widget alone), its style and the capsule's opacity; for
-- a photo the picture (the picker opens in this card's place) and, on an
-- Analogue print, the caption under it; for the spectrum where it is (the
-- grid or an edge) and its look: bars, fill, colours, sizes, lows, peaks and
-- opacity. The first tile of a row follows the desk's setting.
--
-- The card goes to the right of the widget, else its left, else below it,
-- and is kept on the board. It is rebuilt for each widget selected (a
-- Repeater over a one-row model keyed by the selection), so what a module
-- has no use for is never built.
--
-- A note says where it is (the grid or an edge, where it joins the deck)
-- and which note it shows; a deck of notes on an edge, where it is, its
-- place along the edge, whether new notes land on it, and which notes it
-- holds.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local settings = require("services.settings")
local desk = require("services.desktop")
local controls = require("desktop.arrange.controls")
local swatch = require("desktop.theme_swatch")
local deck_service = require("services.deck")

local C = theme.color
local M = {}

M.WIDTH = 312
M.PAD = 14
M.GAP = 14

local inner = M.WIDTH - 2 * M.PAD

local model = controls.keyed("inspector", function()
  if desk.picking:get() ~= "" then return "" end
  return desk.selected:get()
end)

local function pill(values) return require("components.pill_button")(values) end

-- A row of tiles laid out by hand, skipping those `shown` says are out: a
-- Row keeps room for an invisible child.
local function tile_row(items, gap)
  local nodes = {}
  for i, item in ipairs(items) do
    local shown = item.shown or function() return true end
    item.tile_values.visible = shown
    item.tile_values.x = function()
      local x = 0
      for j = 1, i - 1 do
        local other = items[j].shown
        if not other or other() then x = x + items[j].width + gap end
      end
      return x
    end
    nodes[i] = controls.tile(item.tile_values)
  end
  local height = items[1] and items[1].tile_values.height or 48
  return ui.Item { width = inner, height = height, table.unpack(nodes) }
end

-- ------------------------------------------------------------------ pieces --

local function title(key, id)
  local entry = desk.entry(id) or { name = id }
  return ui.Item {
    width = inner, height = 28,
    kit.text {
      x = 0, y = 0, height = 28, vertical_alignment = "center",
      width = inner - 90, elide = "right",
      text = function()
        local row = desk.entry_of(key)
        if desk.is_spectrum(row) then return entry.name .. " · " .. row.edge .. " edge" end
        if desk.is_deck(row) then return entry.name .. " · Notes on the edge" end
        return entry.name
      end,
      size = theme.size.medium, weight = 600, color = C.text,
    },
    ui.Item {
      x = inner - 80, y = 1, width = 80, height = 26,
      ui.Item { anchors = { right = true }, width = 80, height = 26,
        pill { text = "Remove", height = 26, width = 80, on_click = function() desk.remove(key) end } },
    },
  }
end

local function shape_section(key, id)
  local items = {}
  for _, family in ipairs(desk.families) do
    local fid = family.id
    local current = function() return desk.family_of(desk.entry_of(key)) == fid end
    items[#items + 1] = {
      width = family.cols * 9 + 12,
      shown = function()
        local row = desk.entry_of(key)
        return row ~= nil and not desk.is_edge(row) and desk.offers(id, fid, desk.theme_of(row))
      end,
      tile_values = {
        width = family.cols * 9 + 12, height = 48, current = current,
        on_click = function() desk.set_family(key, fid) end,
        ui.Rect {
          x = 6, y = (48 - family.rows * 9) / 2, width = family.cols * 9, height = family.rows * 9, radius = 3,
          color = function() return current() and C.accent() or C.textMuted() end,
        },
      },
    }
  end
  return {
    controls.heading("Shape"),
    tile_row(items, 8),
  }
end

-- A note goes to the grid or to an edge's deck; a deck, the whole of it, to
-- another edge; the spectrum to the grid or to an edge that has none.
local function where_section(key, id)
  local on_spectrum = id == "spectrum"
  local places = {
    { id = "grid", label = "Grid", icon = "󰕰" },
    { id = "left", label = "Left", icon = "󰞕" },
    { id = "right", label = "Right", icon = "󰞘" },
    { id = "bottom", label = "Bottom", icon = "󰞖" },
  }
  local items = {}
  for _, place in ipairs(places) do
    local function current()
      local row = desk.entry_of(key)
      if desk.is_edge(row) then return row.edge == place.id end
      return place.id == "grid"
    end
    local function taken()
      return on_spectrum and not current() and place.id ~= "grid" and not desk.spectrum_takes(place.id)
    end
    items[#items + 1] = {
      width = 64,
      tile_values = {
        width = 64, height = 48, current = current, dim = taken,
        on_click = function()
          if current() or taken() then return end
          local row = desk.entry_of(key)
          if not on_spectrum then
            if desk.is_deck(row) then
              if place.id ~= "grid" then desk.set_deck_edge(key, place.id) end
            elseif place.id ~= "grid" then
              desk.note_to_edge(key, place.id)
            end
            return
          end
          if place.id == "grid" then
            desk.spectrum_to_grid(key)
          elseif desk.is_edge(row) then
            desk.set_spectrum(key, { edge = place.id })
          else
            desk.spectrum_to_edge(key, place.id)
          end
        end,
        ui.Column {
          anchors = { center_in = true }, gap = 3, align = "center",
          kit.glyph { glyph = place.icon, size = 14, color = function() return current() and C.accent() or C.textMuted() end },
          kit.text { text = place.label, size = theme.size.label, color = function() return current() and C.text() or C.textMuted() end },
        },
      },
    }
  end
  return { controls.heading("Where"), tile_row(items, 8) }
end

-- A sample of this spectrum with one thing changed, as a tile.
local function spectrum_tile(key, change, current, on_click)
  local ok, bars = pcall(require, "desktop.spectrum_bars")
  local looks = function()
    local l = desk.spectrum_of(desk.entry_of(key))
    local out = {}
    for k, v in pairs(l) do out[k] = v end
    for k, v in pairs(change) do out[k] = v end
    out.bar, out.gap, out.lows, out.opacity = 4, 2, "along", 100
    return out
  end
  local sample = ok and bars.build {
    x = 6, y = 6, width = 38, height = 32, looks = looks,
    listening = function() return true end, edge = "bottom", sample = true,
  } or ui.Item {}
  return {
    width = 50,
    tile_values = { width = 50, height = 44, current = current, on_click = on_click, sample },
  }
end

local function colour_row(key, field)
  local function chosen()
    local l = desk.spectrum_of(desk.entry_of(key))
    return field == "color" and l.color_name or l.color2_name
  end
  local function choose(value) desk.set_spectrum(key, { [field] = value }) end
  local nodes = {
    controls.tile {
      x = 0, y = 0, width = 84, height = 26, radius = 13,
      current = function() return chosen() == "palette" end,
      on_click = function() choose("palette") end,
      ui.Rect { x = 10, y = 6, width = 14, height = 14, radius = 7, color = C.accent,
        border_color = C.accentText, border_width = 2 },
      kit.text { x = 30, y = 0, height = 26, vertical_alignment = "center", text = "Palette",
        size = theme.size.label, color = function() return chosen() == "palette" and C.accent() or C.text() end },
    },
  }
  local x, y = 90, 0
  for _, entry in ipairs(theme.fixed_colours) do
    if x + 26 > inner then x, y = 0, y + 32 end
    nodes[#nodes + 1] = ui.Item { x = x, y = y, width = 26, height = 26,
      controls.swatch { color = entry.id, current = function() return chosen() == entry.id end,
        on_click = function() choose(entry.id) end } }
    x = x + 32
  end
  return ui.Item { width = inner, height = y + 26, table.unpack(nodes) }
end

local function measure(key, label, field)
  local range = desk.spectrum_ranges[field]
  return ui.Item {
    width = inner, height = 30,
    kit.text { x = 0, y = 0, width = 52, height = 30, vertical_alignment = "center",
      text = label, size = theme.size.label, color = C.textMuted },
    ui.Item { x = 60, y = 0, width = inner - 60, height = 30,
      controls.slider {
        width = inner - 60, height = 30, from = range.from, to = range.to, unit = " px",
        get = function() return desk.spectrum_of(desk.entry_of(key))[field] end,
        set = function(v) desk.set_spectrum(key, { [field] = v }) end,
      } },
  }
end

local function spectrum_sections(key)
  local function looks() return desk.spectrum_of(desk.entry_of(key)) end
  local look_items, fill_items = {}, {}
  for _, entry in ipairs(desk.spectrum_looks) do
    look_items[#look_items + 1] = spectrum_tile(key, { look = entry.id },
      function() return looks().look == entry.id end,
      function() desk.set_spectrum(key, { look = entry.id }) end)
  end
  for _, entry in ipairs(desk.spectrum_fills) do
    fill_items[#fill_items + 1] = spectrum_tile(key, { fill = entry.id },
      function() return looks().fill == entry.id end,
      function() desk.set_spectrum(key, { fill = entry.id }) end)
  end
  local function label_of(list, id)
    for _, e in ipairs(list) do if e.id == id then return e.label end end
    return ""
  end
  local lows = {}
  for _, entry in ipairs { { id = "corners", label = "At the corners" }, { id = "along", label = "Along the edge" } } do
    local current = function() return looks().lows == entry.id end
    lows[#lows + 1] = {
      width = 138,
      tile_values = { width = 138, height = 34, current = current,
        on_click = function() desk.set_spectrum(key, { lows = entry.id }) end,
        kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
          text = entry.label, size = theme.size.label,
          color = function() return current() and C.text() or C.textMuted() end } },
    }
  end
  local blend = function() return looks().fill == "blend" end
  local strip = function() return desk.is_spectrum(desk.entry_of(key)) end
  -- Parts that come and go keep a pixel of room rather than none.
  local function maybe(shown, node, height)
    return ui.Item { width = inner, height = function() return shown() and height or 1 end,
      visible = shown, node }
  end
  local second = colour_row(key, "color2")
  return {
    controls.heading("Look"), tile_row(look_items, 6),
    controls.heading("Fill"), tile_row(fill_items, 6),
    kit.text { width = inner, height = 14, elide = "right", size = theme.size.label, color = C.textMuted,
      text = function()
        return label_of(desk.spectrum_looks, looks().look) .. " · " .. label_of(desk.spectrum_fills, looks().fill)
      end },
    kit.text { height = 14, text = function() return blend() and "From the edge" or "Colour" end,
      size = theme.size.label, weight = 600, color = C.textMuted },
    colour_row(key, "color"),
    maybe(blend, ui.Column { gap = 12, controls.heading("To the tip"), second }, 14 + 12 + 58),
    controls.heading("Size"),
    maybe(strip, measure(key, "Height", "reach"), 30),
    measure(key, "Bars", "bar"),
    measure(key, "Gap", "gap"),
    controls.heading("Lows"),
    tile_row(lows, 8),
    ui.Item { width = inner, height = 28,
      kit.text { x = 0, y = 0, height = 28, vertical_alignment = "center", text = "Peaks",
        size = theme.size.label, weight = 600, color = C.textMuted },
      controls.switch { x = inner - 36, y = 4,
        get = function() return looks().peaks end,
        set = function(on) desk.set_spectrum(key, { peaks = on }) end } },
    controls.slider {
      width = inner, height = 34, icon = "󰊸",
      from = desk.spectrum_ranges.opacity.from, to = desk.spectrum_ranges.opacity.to,
      get = function() return looks().opacity end,
      set = function(v) desk.set_spectrum(key, { opacity = v }) end,
    },
  }
end

-- For a photo: the picture, the picker and a way to empty it; on an
-- Analogue print, the caption written in its chin.
local function photo_sections(key)
  local function path() return desk.picture_of(desk.entry_of(key)) end
  local function captioned()
    local row = desk.entry_of(key)
    return row ~= nil and desk.theme_of(row) == "analogue" and desk.family_of(row) ~= "8x2"
  end
  local caption
  caption = ui.TextInput {
    x = 10, y = 0, width = inner - 20, height = 34, vertical_alignment = "center",
    font_family = function() return theme.font() end, font_size = theme.size.small,
    color = C.text, placeholder = "Written under the picture", placeholder_color = C.textMuted,
    selection_color = C.accent, selected_text_color = C.accentText, max_length = 40,
    text = (function() local row = desk.entry_of(key) return row and type(row.caption) == "string" and row.caption or "" end)(),
    on_text_changed = function(text) desk.update(key, { caption = text ~= "" and text or false }) end,
    on_focus_changed = function(focused) desk.typing:set(focused) end,
    on_accepted = function() caption.focus = false end,
    on_escape = function() caption.focus = false end,
  }
  return {
    controls.heading("Picture"),
    ui.Row {
      gap = 10, align = "center", height = 48,
      ui.ClipRect {
        width = 48, height = 48, radius = 48 * theme.picture_corner, color = C.islandSurface,
        ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
          source_width = 96, source_height = 96,
          source = path, visible = function() return path() ~= "" end },
        kit.glyph { anchors = { fill = true }, vertical_alignment = "center", glyph = "󰋩", size = 16,
          color = C.textMuted, visible = function() return path() == "" end },
      },
      pill { text = "Choose…", height = 26, on_click = function() desk.picking:set(key) end },
      kit.icon_button { glyph = "󰅖", glyph_size = 13, diameter = 26, color = "#00000000",
        hover_color = C.islandSurfaceHover,
        on_click = function() desk.update(key, { picture = false }) end },
    },
    ui.Item {
      width = inner, height = function() return captioned() and 60 or 1 end, visible = captioned,
      controls.heading("Caption"),
      ui.Rect {
        x = 0, y = 26, width = inner, height = 34, radius = theme.radius_small, color = C.islandSurface,
        border_width = 1,
        border_color = function() return desk.typing:get() and C.accent() or C.islandBorder end,
        caption,
      },
    },
  }
end

-- For a notes widget on a cell: the newest (the default when the row names
-- none), then every note, each with its tint.
local function note_sections(key)
  local notes = require("services.notes")
  local choices = { { key = "", title = "The newest" } }
  for _, n in ipairs(notes.live()) do
    choices[#choices + 1] = { key = n.key, title = notes.title_of(n), tint = n.tint }
  end
  local nodes, x, y = {}, 0, 0
  for _, choice in ipairs(choices) do
    local dot = choice.key ~= "" and 12 or 0
    local w = math.min(140, 24 + dot + math.ceil(utf8.len(choice.title) or #choice.title) * 6)
    if x + w > inner then x, y = 0, y + 32 end
    local current = function()
      local row = desk.entry_of(key)
      return (row and row.note or "") == choice.key
    end
    local children = {
      x = x, y = y, width = w, height = 26, radius = theme.radius_pill, current = current,
      on_click = function() desk.update(key, { note = choice.key ~= "" and choice.key or false }) end,
    }
    if dot > 0 then
      children[#children + 1] = ui.Rect { x = 10, y = 9.5, width = 7, height = 7, radius = 3.5,
        color = function() return notes.tint_color(choice.tint) end }
    end
    children[#children + 1] = kit.text {
      x = 10 + dot, y = 0, width = w - 20 - dot, height = 26, vertical_alignment = "center", elide = "right",
      text = choice.title, size = theme.size.label,
      color = function() return current() and C.text() or C.textMuted() end,
    }
    nodes[#nodes + 1] = controls.tile(children)
    x = x + w + 6
  end
  return { controls.heading("Which note"), ui.Item { width = inner, height = y + 26, table.unpack(nodes) } }
end

-- For a deck: its place along the edge (finer placement is the grip before
-- the first tab), whether new notes land on it, and every note ticked on or
-- off it. Ticking a note here moves it from wherever it was.
local function deck_sections(key)
  local notes = require("services.notes")
  local along = {}
  for _, entry in ipairs { { value = 0, label = "Start" }, { value = 0.5, label = "Middle" }, { value = 1, label = "End" } } do
    local current = function() return math.abs(desk.along_of(desk.entry_of(key)) - entry.value) < 0.01 end
    along[#along + 1] = {
      width = 64,
      tile_values = { width = 64, height = 30, current = current,
        on_click = function() desk.set_deck_along(key, entry.value) end,
        kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
          text = entry.label, size = theme.size.label,
          color = function() return current() and C.text() or C.textMuted() end } },
    }
  end
  local ticks = {}
  for _, n in ipairs(notes.live()) do
    local note_key, tint = n.key, n.tint
    local on = function()
      for _, k in ipairs(desk.deck_notes(desk.entry_of(key))) do if k == note_key then return true end end
      return false
    end
    local elsewhere = function()
      if on() then return "" end
      return desk.placement_of(note_key)
    end
    local hovered = controls.signal("inspector.tick", false)
    ticks[#ticks + 1] = ui.Rect {
      width = inner, height = 26, radius = theme.radius_small,
      color = function() return hovered:get() and C.islandSurfaceHover or morf.color("transparent") end,
      ui.Rect {
        x = 6, y = 6, width = 14, height = 14, radius = 4,
        color = function() return on() and C.accent() or morf.color("transparent") end,
        border_color = function() return on() and C.accent() or C.textMuted() end, border_width = 1.5,
        kit.glyph { anchors = { fill = true }, vertical_alignment = "center", glyph = "󰄬", size = 9,
          color = C.accentText, visible = on },
      },
      ui.Rect { x = 28, y = 9, width = 8, height = 8, radius = 4,
        color = function() return notes.tint_color(tint) end },
      kit.text {
        x = 44, y = 0, height = 26, vertical_alignment = "center", elide = "right",
        width = function() return inner - 50 - (elsewhere() ~= "" and 18 or 0) end,
        text = function() local e = notes.entry(note_key) return e and notes.title_of(e) or "" end,
        size = theme.size.label, color = function() return on() and C.text() or C.textMuted() end,
      },
      kit.glyph {
        x = inner - 22, y = 0, width = 16, height = 26, vertical_alignment = "center", size = 10,
        color = C.textMuted, visible = function() return elsewhere() ~= "" end,
        glyph = function() return elsewhere() == "grid" and "󰕰" or "󰞘" end,
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() desk.toggle_deck_note(key, note_key) end,
      },
    }
  end
  return {
    controls.heading("Along the edge"),
    tile_row(along, 8),
    ui.Item {
      width = inner, height = 28,
      kit.text { x = 0, y = 0, height = 28, vertical_alignment = "center", text = "New notes land here",
        size = theme.size.label, weight = 600, color = C.textMuted },
      controls.switch { x = inner - 36, y = 4,
        get = function() return desk.takes_new(desk.entry_of(key)) end,
        set = function(on) desk.set_takes_new(key, on) end },
    },
    controls.heading("Which notes"),
    ui.Column { width = inner, gap = 4, height = #ticks * 30 - 4, table.unpack(ticks) },
  }
end

local function face_section(key)
  local items = {}
  for _, entry in ipairs { { id = "", label = "Default" }, { id = "modern" }, { id = "analogue" } } do
    local current = function()
      local row = desk.entry_of(key)
      return (row and row.theme or "") == entry.id
    end
    local shown_theme = entry.id ~= "" and entry.id or settings.desktopTheme
    local children = {
      swatch.build(shown_theme, 0.19, { x = (50 - swatch.side(0.19)) / 2, y = (48 - swatch.side(0.19)) / 2 }),
    }
    if entry.id == "" then children[#children + 1] = controls.default_dot(50) end
    local values = { width = 50, height = 48, current = current,
      on_click = function() desk.set_theme(key, entry.id ~= "" and entry.id or false) end }
    for _, c in ipairs(children) do values[#values + 1] = c end
    items[#items + 1] = { width = 50, tile_values = values }
  end
  return { controls.heading("Face"), tile_row(items, 8) }
end

-- A widget style in miniature: the capsule in its ink with "Aa" on it.
local function style_swatch(id, style)
  local ink = desk.ink_for(function() return { id = id, style = style } end)
  local on_picture = style == "bare" or style == "outline"
  return ui.Rect {
    x = 8, y = 11, width = 34, height = 26, radius = 7,
    color = function() return on_picture and morf.color("transparent") or ink.ground() end,
    border_width = (style == "accent" or style == "bare") and 0 or 1,
    border_color = function()
      if style == "outline" then return ink.text():alpha(0.55) end
      if style == "capsule" then return ink.border() end
      return morf.color("transparent")
    end,
    kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
      text = "Aa", size = 11, weight = 600, color = ink.text },
  }
end

local function style_sections(key, id)
  local items = {}
  local list = { { id = "" } }
  for _, s in ipairs(desk.styles) do list[#list + 1] = s end
  for _, entry in ipairs(list) do
    local current = function()
      local row = desk.entry_of(key)
      return (row and row.style or "") == entry.id
    end
    local values = { width = 50, height = 48, current = current,
      on_click = function() desk.set_style(key, entry.id ~= "" and entry.id or false) end,
      style_swatch(id, entry.id ~= "" and entry.id or settings.desktopStyle) }
    if entry.id == "" then values[#values + 1] = controls.default_dot(50) end
    items[#items + 1] = { width = 50, tile_values = values }
  end
  local function label_of(list_, value)
    for _, e in ipairs(list_) do if e.id == value then return e.label end end
    return ""
  end
  local function own_opacity()
    local row = desk.entry_of(key)
    return row ~= nil and type(row.opacity) == "number"
  end
  return {
    controls.heading("Style"),
    tile_row(items, 8),
    kit.text { width = inner, height = 14, elide = "right", size = theme.size.label, color = C.textMuted,
      text = function()
        local row = desk.entry_of(key)
        local own = row and (row.theme or row.style)
        return label_of(desk.themes, desk.theme_of(row)) .. " · " .. label_of(desk.styles, desk.style_of(row))
          .. (own and "" or " · default")
      end },
    ui.Item {
      width = inner, height = 40,
      controls.slider {
        width = inner - 34, height = 40, icon = "󰊸", from = 20, to = 100,
        get = function() return desk.opacity_of(desk.entry_of(key)) end,
        set = function(v) desk.set_opacity(key, v) end,
      },
      ui.Item { x = inner - 28, y = 7, width = 26, height = 26,
        opacity = function() return own_opacity() and 1 or 0.3 end,
        kit.icon_button { glyph = "󰦛", glyph_size = 13, diameter = 26, color = "#00000000",
          hover_color = C.islandSurfaceHover,
          on_click = function() if own_opacity() then desk.set_opacity(key, nil) end end } },
    },
  }
end

-- ---------------------------------------------------------------------- card --

local function card_for(key)
  local row = desk.entry_of(key)
  if not row then return ui.Item {} end
  local id = row.id
  local on_note, on_photo, on_spectrum = id == "notes", id == "photo", id == "spectrum"
  local styled = not on_note and not on_photo and not on_spectrum

  local on_deck = desk.is_deck(row)
  local children = { title(key, id), controls.rule(inner) }
  local function add(list) for _, n in ipairs(list) do children[#children + 1] = n end end
  if not desk.is_edge(row) then add(shape_section(key, id)) end
  if on_note or on_spectrum then add(where_section(key, id)) end
  if on_spectrum then add(spectrum_sections(key)) end
  if on_deck then add(deck_sections(key)) end
  if on_note and not on_deck then add(note_sections(key)) end
  if on_photo then add(photo_sections(key)) end
  if not on_note and not on_spectrum then add(face_section(key)) end
  if styled then add(style_sections(key, id)) end

  local column = ui.Column { x = M.PAD, y = M.PAD, width = inner, gap = 12, table.unpack(children) }

  -- The widget's box on the board; a strip's is measured from the screen.
  local function box()
    local r = desk.entry_of(key)
    if desk.is_spectrum(r) then
      local b, s = desk.board(), desk.spectrum_box(r)
      return { x = s.x - b.x, y = s.y - b.y, width = s.width, height = s.height }
    end
    if desk.is_deck(r) then
      -- The whole strip at full depth.
      local board, D = desk.board(), deck_service
      local n = #desk.deck_notes(r)
      local length = r.edge == "bottom" and board.width or board.height
      local start = D.start_of(n, desk.along_of(r), length)
      local strip = D.strip_length(n)
      if r.edge == "bottom" then
        return { x = start, y = board.height - D.tab_depth, width = strip, height = D.tab_depth }
      end
      return { x = r.edge == "right" and board.width - D.tab_depth or 0, y = start, width = D.tab_depth, height = strip }
    end
    return desk.geometry(key) or { x = 0, y = 0, width = 0, height = 0 }
  end
  -- Every section has a height of its own, so the card is their sum.
  local function height()
    local total = 0
    for _, child in ipairs(children) do total = total + (tonumber(child.height) or 0) end
    return total + 12 * (#children - 1) + 2 * M.PAD
  end
  local function right_fits()
    local b = box()
    return b.x + b.width + M.GAP + M.WIDTH <= desk.board().width - theme.desktop_gutter
  end
  local function left_fits() return box().x - M.GAP - M.WIDTH >= theme.desktop_gutter end
  -- Anything on the bottom edge has no side: the card goes above.
  local function above()
    local r = desk.entry_of(key)
    return desk.is_edge(r) and r.edge == "bottom"
  end
  local gutter = theme.desktop_gutter
  return ui.Rect {
    x = function()
      local b, board = box(), desk.board()
      if above() then return math.max(gutter, b.x + gutter) end
      if right_fits() then return b.x + b.width + M.GAP end
      if left_fits() then return b.x - M.GAP - M.WIDTH end
      return math.max(gutter, math.min(board.width - gutter - M.WIDTH, b.x))
    end,
    y = function()
      local b, board = box(), desk.board()
      if above() then return math.max(gutter, b.y - M.GAP - height()) end
      local want = (right_fits() or left_fits()) and b.y or b.y + b.height + M.GAP
      return math.max(gutter, math.min(board.height - gutter - height(), want))
    end,
    width = M.WIDTH, height = height,
    radius = theme.radius_large, color = C.island, border_color = C.islandBorder, border_width = 1,
    behavior = { x = theme.behave("medium"), y = theme.behave("medium") },
    -- Takes every click on the card, so the board under it keeps the selection.
    ui.MouseArea { anchors = { fill = true }, accepted_buttons = { "left", "right" } },
    column,
  }
end

--- The inspector, filling the board.
function M.build()
  return ui.Item {
    anchors = { fill = true },
    ui.Repeater {
      model = model,
      delegate = function(r) return card_for(r.id) end,
    },
  }
end

return M
