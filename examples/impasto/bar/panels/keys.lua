-- The key sheet: one field over one list, grouped, the action on the left
-- and its keys as keycaps on the right. The field narrows the list by
-- action, group or key; the arrows and the wheel scroll it.
--
-- Port of KeysPanel.qml. Read-only, as the original's panel was; the
-- original rebound keys in its settings window by rewriting the user's
-- Hyprland key file, which this port does not touch.
--
-- `morf ipc call keys` toggles it; `keys.sample` fills it with a sample
-- bind list (a test bench has no Hyprland), `keys.find <text>` types.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local shortcuts = require("services.shortcuts")

local C = theme.color
local K = shortcuts

local PAD = theme.panel_padding
local INNER_W = K.sheet_width - 2 * PAD
local INNER_H = K.sheet_height - 2 * PAD
local LIST_H = INNER_H - K.field_height - 10 - 1 - 10

local KEY = { UP = 0xff52, DOWN = 0xff54, PAGE_UP = 0xff55, PAGE_DOWN = 0xff56 }

local query = morf.signal("impasto.keys.query", "")
local scroll = morf.signal("impasto.keys.scroll", 0)
local content_h = morf.signal("impasto.keys.content", 0)

local entries = morf.list_model({})
local shown = morf.signal("impasto.keys.shown", 0)

morf.effect("impasto.keys.entries", function()
  local list = K.find(query:get())
  local rows, count = {}, 0
  local height = 0
  for index, entry in ipairs(list) do
    rows[index] = {
      key = (entry.heading and "h:" or "r:") .. (entry.name or entry.action) .. ":" .. table.concat(entry.caps or {}, "+"),
      heading = entry.heading, name = entry.name or "", action = entry.action or "", caps = entry.caps or {},
    }
    if not entry.heading then count = count + 1 end
    height = height + (entry.heading and K.heading_height or K.row_height)
  end
  entries:replace(rows, "key")
  shown:set(count)
  content_h:set(height)
  scroll:set(0)
end)

local function scroll_by(pixels)
  local bottom = math.max(0, content_h:get() - LIST_H)
  scroll:set(math.max(0, math.min(bottom, scroll:get() + pixels)))
end

-- A key drawn as a keycap. An arrow is a glyph only the mono face carries.
local function cap(label)
  local glyph = utf8.codepoint(label, 1) >= 0xE000
  local text = kit.text {
    anchors = { center_in = true }, text = label, weight = 600,
    mono = glyph, size = glyph and theme.size.regular or theme.size.small,
  }
  return ui.Rect {
    width = function() return math.max(24, (text.layout_width or 0) + 14) end,
    height = 22, radius = theme.radius_small - 2,
    color = C.islandSurface, border_width = 1, border_color = C.islandBorder,
    text,
  }
end

local function delegate(row)
  if row.heading then
    return ui.Item {
      width = INNER_W, height = K.heading_height,
      kit.text {
        anchors = { left = true, bottom = true, left_margin = 10, bottom_margin = 6 },
        text = row.name:upper(), size = theme.size.label, weight = 600,
        letter_spacing = 0.8, color = C.accent,
      },
    }
  end
  local hovered = kit.hover_signal("keys.row")
  local caps = { gap = 4, align = "center", anchors = { right = true, right_margin = 8, vertical_center = true } }
  for _, label in ipairs(row.caps) do caps[#caps + 1] = cap(label) end
  local keys = ui.Row(caps)
  return ui.Rect {
    width = INNER_W, height = K.row_height, radius = theme.radius_small,
    color = function() return hovered:get() and C.islandSurface or "#00000000" end,
    behavior = { color = theme.behave("fast") },
    kit.text {
      anchors = { left = true, left_margin = 10, vertical_center = true },
      text = row.action, size = theme.size.regular, elide = "right",
      width = function() return INNER_W - 10 - 16 - 8 - (keys.layout_width or 0) end,
    },
    keys,
    ui.MouseArea {
      anchors = { fill = true },
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
    },
  }
end

-- Opening reads the binds again, so a rebind since the last opening shows;
-- closing clears the field for the next time.
local field
morf.effect("impasto.keys.lifecycle", function()
  local open = island.state.open_panel() == "keys"
  morf.timer(1, function()
    if open then K.load() else query:set("") field = nil end
  end, false)
end)

local function build()
  local input
  input = ui.TextInput {
    anchors = { left = true, vertical_center = true, left_margin = 29 },
    width = INNER_W - 29 - 110, height = K.field_height,
    vertical_alignment = "center",
    placeholder = "Find a key, or what it does…", placeholder_color = C.textMuted,
    font_family = function() return theme.font() end, font_size = 16,
    color = C.text, caret_color = C.accent,
    selection_color = C.accent, selected_text_color = C.accentText,
    focus = true,
    on_text_changed = function(text) query:set(text) end,
    on_escape = function() island.close() end,
    on_key_pressed = function(keysym)
      if keysym == KEY.UP then scroll_by(-K.row_height)
      elseif keysym == KEY.DOWN then scroll_by(K.row_height)
      elseif keysym == KEY.PAGE_UP then scroll_by(-LIST_H * 0.8)
      elseif keysym == KEY.PAGE_DOWN then scroll_by(LIST_H * 0.8) end
    end,
  }
  field = input
  local column = ui.Repeater { as = "column", model = entries, delegate = delegate }
  return ui.Item {
    width = INNER_W, height = INNER_H,
    ui.Column {
      gap = 10,
      ui.Item {
        width = INNER_W, height = K.field_height,
        kit.glyph { anchors = { left = true, vertical_center = true }, glyph = "󰌌", size = 17, color = C.accent, width = 20 },
        input,
        kit.text {
          anchors = { right = true, vertical_center = true },
          text = function()
            if query:get() == "" then return K.count() .. " keys" end
            return shown:get() .. " of " .. K.count()
          end,
          size = theme.size.small, color = C.textMuted,
        },
      },
      ui.Rect { width = INNER_W, height = 1, color = C.islandBorder },
      ui.ClipRect {
        width = INNER_W, height = LIST_H, color = "#00000000",
        ui.Item {
          width = INNER_W,
          translate_y = function() return -scroll:get() end,
          behavior = { translate_y = theme.behave("fast") },
          column,
        },
        kit.text {
          anchors = { center_in = true },
          visible = function() return entries:len() == 0 or shown:get() == 0 end,
          text = function()
            local status = K.status()
            if K.count() == 0 then
              if status == "reading" or status == "" then return "Reading the keys…" end
              return status
            end
            return "No key does that."
          end,
          size = theme.size.regular, color = C.textMuted,
          width = INNER_W - 80, wrap = true, horizontal_alignment = "center",
        },
        ui.MouseArea {
          anchors = { fill = true }, z = -1,
          on_wheel = function(_, _, _, pixels_y, _, steps_y)
            local step = (steps_y and steps_y ~= 0) and steps_y * K.row_height or (pixels_y or 0)
            scroll_by(step)
          end,
        },
      },
    },
  }
end

island.register("keys", {
  size = function() return K.sheet_width, K.sheet_height end,
  build = build,
})

morf.ipc["keys.sample"] = function()
  K.sample()
  return tostring(K.count())
end
morf.ipc["keys.find"] = function(...)
  local text = table.concat({ ... }, " ")
  query:set(text)
  if field then pcall(function() field.text = text end) end
  return tostring(shown:get())
end
