-- The launcher: a panel floating in the middle of the screen, opened by a
-- key (`morf ipc call launcher`), with a search field and the best matches
-- above it. Typing the action prefix (">") lists the
-- shell's own actions instead, and some of them open pickers: "> scheme "
-- and "> variant " (the colours, rebuilt through lib/material.lua and put
-- in place live), "> wallpaper " (a carousel of pictures, the drawer
-- widening for it), "> calc " (calc.lua). Up and Down (Left and Right in
-- the carousel) move the highlight, Return runs it, Escape shuts the
-- drawer.
--
-- Measured off the reference: 630 wide; rows 57 tall, 8 apart, a 32 px
-- icon; a 48 px search field; the drawer as tall as the rows it shows (up
-- to seven), easing to a new height as the results change. The wallpaper
-- carousel: 1270 wide and 203 tall, pictures 224 x 126 on 248 px slots,
-- the chosen one 280 x 158 on a 304 px slot, with its name under it.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local apps = require("apps")
local drawer = require("drawer")

local C = theme.color
local M = {}

local WIDTH, WIDE = 630, 1270
local PAD = 15
local ROW, ROW_GAP = 57, 8
local SEARCH = 48
local EMPTY = 90
local CAROUSEL = 203
local SLOT, SLOT_ON = 248, 304

M.query = morf.signal("caelestia.launcher.query", "")
M.selected = morf.signal("caelestia.launcher.selected", 1)
M.results = morf.list_model({})
M.count = morf.signal("caelestia.launcher.count", 0)
M.mode = morf.signal("caelestia.launcher.mode", "apps")
-- The carousel's pictures: `{ path, name }` each, and how many.
M.walls = {}
M.wall_count = morf.signal("caelestia.launcher.walls", 0)

local function max_shown() return config.get("launcher.max_shown") end

local by_key = {}

--- The full row behind a model entry.
local function row_of(entry) return entry and by_key[entry.key] end

-- The results follow the query.
morf.effect("caelestia.launcher.search", function()
  local q = M.query:get()
  local found, mode = apps.search(q, max_shown())
  if mode == "wallpapers" then
    M.walls = found
    local current = require("wallpaper").current:get()
    local at = 1
    for i, w in ipairs(found) do
      by_key["wallpaper:" .. w.id] = w
      if w.path == current then at = i end
    end
    M.results:replace({}, "key")
    M.count:set(0)
    M.wall_count:set(#found)
    M.mode:set(mode)
    M.selected:set(at)
    return
  end
  local out = {}
  for i = 1, math.min(#found, max_shown()) do
    local row = found[i]
    local key = row.kind .. ":" .. row.id
    -- A model row is plain data; the row itself (an action's function
    -- among it) stays in Lua, by key.
    by_key[key] = row
    out[i] = {
      key = key .. (row.kind == "calc" and (":" .. row.name) or ""),
      kind = row.kind, id = row.id, name = row.name,
      description = row.description, icon = row.icon,
    }
    by_key[out[i].key] = row
  end
  M.results:replace(out, "key")
  M.count:set(#out)
  M.wall_count:set(0)
  M.mode:set(mode)
  M.selected:set(1)
end)

local function list_height(n)
  if n <= 0 then return EMPTY end
  return n * ROW + (n - 1) * ROW_GAP
end

local function wide() return M.mode:get() == "wallpapers" end

local function width() return wide() and WIDE or WIDTH end
M.width = width

local function height()
  local body = wide() and CAROUSEL or list_height(M.count:get())
  return PAD + body + 16 + SEARCH + 6
end

-- ------------------------------------------------------------------- rows --

local function row_icon(row)
  if row.kind == "action" or row.kind == "variant" then
    return kit.centred(32, 32, kit.icon(row.icon, 34, function() return C.onSurfaceVariant end))
  end
  if row.kind == "calc" then
    return kit.centred(32, 32, kit.icon("function", 36, function() return C.onSurface end))
  end
  if row.kind == "scheme" then
    if not row.color then
      return kit.centred(32, 32, kit.icon("wallpaper", 30, function() return C.onSurfaceVariant end))
    end
    local ok, color = pcall(morf.color, row.color)
    return kit.centred(32, 32, ui.Rect {
      width = 28, height = 28, radius = 14, color = ok and color or row.color,
      border_width = 2, border_color = function() return C.outlineVariant end,
    })
  end
  local hit = apps.icon(row.icon)
  if hit and hit.name then
    return ui.Icon { width = 32, height = 32, name = hit.name, source_width = 64, source_height = 64 }
  elseif hit and hit.path then
    return ui.Image { width = 32, height = 32, source = hit.path, fill_mode = "preserve_aspect_fit" }
  end
  return kit.centred(32, 32, kit.icon("apps", 28, function() return C.onSurfaceVariant end))
end

local function is_selected(key)
  local sel = M.results:get(M.selected:get())
  return sel and sel.key == key
end

local function delegate(entry)
  local row = row_of(entry) or entry
  local body
  if row.kind == "calc" then
    -- One line: the expression and its answer, and a button that copies
    -- the answer.
    body = {
      ui.Row {
        x = 12, y = (ROW - 32) / 2, gap = 17, align = "center",
        row_icon(row),
        kit.text {
          id = "launcher-calc", text = row.name, font_size = theme.size.normal + 1,
          width = WIDTH - 2 * PAD - 150, elide = "right",
          color = function() return row.failed and C.onSurfaceVariant or C.onSurface end,
        },
      },
      kit.hover(ui.MouseArea {
        id = "launcher-calc-copy",
        anchors = { right = true, right_margin = 12, vertical_center = true },
        width = 54, height = 44, cursor = "pointer",
        visible = not row.failed,
        on_clicked = function() M.activate(row) end,
        kit.icon("open_in_new", 24, function() return C.onTertiaryContainer end, { anchors = { center_in = true } }),
      }, function(hovered)
        return hovered and C.tertiaryContainer:mix(C.onTertiaryContainer, 0.08) or C.tertiaryContainer
      end, 12),
    }
  else
    body = {
      ui.Row {
        x = 12, y = (ROW - 32) / 2, gap = (row.kind == "app") and 13 or 17,
        align = "center",
        row_icon(row),
        ui.Column {
          gap = 3,
          kit.text { id = "launcher-name", text = row.name, font_size = theme.size.larger, color = function() return C.onSurface end },
          kit.text {
            text = row.description, font_size = theme.size.smaller,
            color = function() return C.onSurfaceVariant end,
            width = WIDTH - 2 * PAD - 80 - (row.current and 30 or 0), elide = "right",
          },
        },
      },
      row.current and kit.icon("check", 22, function() return C.primary end, {
        anchors = { right = true, right_margin = 16, vertical_center = true },
      }) or nil,
    }
  end
  local area = ui.MouseArea {
    id = "launcher-row-" .. entry.key,
    enter = { opacity = 0, scale = 0.96, duration = theme.duration.small, easing = theme.ease.standard_decel },
    exit = { opacity = 0, scale = 0.96, duration = 150, easing = theme.ease.standard_accel },
    width = WIDTH - 2 * PAD, height = ROW, cursor = "pointer",
    on_entered = function()
      for i = 1, M.results:len() do
        if M.results:get(i).key == entry.key then M.selected:set(i) end
      end
    end,
    on_clicked = function() M.activate(row) end,
    ui.Rect {
      anchors = { fill = true },
      radius = 14,
      -- The selection is drawn once, under the rows (the highlight below).
      color = function()
        if row.kind == "calc" then return C.surfaceContainer end
        return C.onSurface:alpha(0)
      end,
      behavior = { color = { duration = theme.duration.small } },
    },
    table.unpack(body),
  }
  return area
end

-- --------------------------------------------------------------- carousel --

local MAX_SLOTS = 64

local function carousel()
  local slots = {}
  for i = 1, MAX_SLOTS do
    local function wall() return i <= M.wall_count:get() and M.walls[i] or nil end
    local function on() return M.selected:get() == i end
    -- Only pictures near the chosen one are loaded.
    local function near() return math.abs(M.selected:get() - i) <= 4 end
    local motion = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }
    local slot
    slot = ui.MouseArea {
      id = "launcher-wallpaper-" .. i,
      height = CAROUSEL, cursor = "pointer",
      width = function() return on() and SLOT_ON or SLOT end,
      visible = function() return wall() ~= nil end,
      behavior = { width = motion },
      on_clicked = function()
        if on() then
          local w = wall()
          if w then M.activate(w) end
        else
          M.selected:set(i)
        end
      end,
      ui.Column {
        anchors = { horizontal_center = true }, gap = 6, align = "center",
        y = function() return on() and 14 or 32 end,
        behavior = { y = motion },
        ui.Rect {
          radius = 10, clip = true,
          width = function() return on() and 280 or 224 end,
          height = function() return on() and 158 or 126 end,
          behavior = { width = motion, height = motion },
          color = function() return C.surfaceContainerHigh end,
          ui.Image {
            anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
            source = function()
              local w = wall()
              return (w and near()) and w.path or ""
            end,
          },
        },
        kit.text {
          text = function() local w = wall() return w and w.name or "" end,
          font_size = function() return on() and theme.size.normal + 1 or theme.size.small end,
          font_weight = 500,
          width = function() return on() and 280 or 224 end,
          horizontal_alignment = "center", elide = "right",
        },
      },
    }
    local wash = ui.Rect {
      anchors = { fill = true, top_margin = 3 }, radius = 12, z = -1,
      color = function()
        if on() then return C.onSurface:alpha(0.07) end
        return slot.hovered and C.onSurface:alpha(0.04) or C.onSurface:alpha(0)
      end,
      behavior = { color = { duration = theme.duration.small } },
    }
    ui.reparent(wash, slot)
    slots[i] = slot
  end
  local row = ui.Row {
    gap = 0,
    translate_x = function()
      local sel = math.max(1, M.selected:get())
      return (WIDE - 2 * PAD) / 2 - ((sel - 1) * SLOT + SLOT_ON / 2)
    end,
    behavior = { translate_x = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel } },
    table.unpack(slots),
  }
  return ui.Item {
    id = "launcher-wallpapers",
    x = PAD, y = PAD, width = WIDE - 2 * PAD, height = CAROUSEL, clip = true,
    visible = wide,
    row,
    kit.text {
      anchors = { center_in = true }, text = "No wallpapers in ~/Pictures/Wallpapers",
      font_size = theme.size.larger, color = function() return C.onSurfaceVariant end,
      visible = function() return M.wall_count:get() == 0 end,
    },
  }
end

-- ------------------------------------------------------------------ build --

local field

function M.activate(row)
  local next_step = apps.activate(row)
  if next_step == "close" then
    M.drawer.set(false)
  elseif type(next_step) == "string" and next_step ~= "keep" then
    field.text = next_step
    field.cursor_position = #next_step
    M.query:set(next_step)
  end
end

local function move(delta)
  local n = wide() and M.wall_count:get() or M.results:len()
  if n == 0 then return end
  M.selected:set(math.max(1, math.min(n, M.selected:get() + delta)))
end

local function chosen()
  if wide() then return M.walls[M.selected:get()] end
  return row_of(M.results:get(M.selected:get()))
end

field = ui.TextInput {
  id = "launcher-search",
  height = SEARCH,
  anchors = { left = true, right = true, left_margin = 48, right_margin = 44 },
  vertical_alignment = "center",
  font_family = theme.font, font_size = theme.size.normal,
  color = function() return C.onSurface end,
  placeholder = 'Type ">" for commands',
  placeholder_color = function() return C.onSurfaceVariant end,
  caret_color = function() return C.onSurface end,
  selection_color = function() return C.primary:alpha(0.4) end,
  on_text_changed = function(text) M.query:set(text) end,
  on_accepted = function() M.activate(chosen()) end,
  on_escape = function() M.drawer.set(false) end,
  on_key_pressed = function(_, _, _, _, key)
    if key == "Up" then move(-1) return true end
    if key == "Down" or key == "Tab" then move(1) return true end
    if wide() and key == "Left" then move(-1) return true end
    if wide() and key == "Right" then move(1) return true end
  end,
}

local clear
clear = ui.MouseArea {
  id = "launcher-clear",
  width = 36, height = 36, cursor = "pointer",
  anchors = { right = true, right_margin = 10, top = true, top_margin = 6 },
  visible = function() return M.query:get() ~= "" end,
  on_clicked = function() field.text = "" M.query:set("") end,
  kit.icon("close", 20, function() return C.onSurfaceVariant end, { anchors = { center_in = true } }),
}

local empty = ui.Row {
  id = "launcher-empty",
  anchors = { center_in = true }, gap = 14, align = "center",
  visible = function() return M.count:get() == 0 end,
  kit.icon("manage_search", 40, function() return C.onSurfaceVariant end),
  ui.Column {
    gap = 0,
    kit.text { text = "No results", font_size = theme.size.large, color = function() return C.onSurface end },
    kit.text {
      text = "Try searching for something else", font_size = theme.size.larger,
      color = function() return C.onSurfaceVariant end,
    },
  },
}

-- The selection: one rounded box in a distance field under the rows,
-- tracking an item that springs from row to row -- sliding, squashing and
-- stretching on the way -- rather than a highlight that jumps.
local highlight = ui.Item {
  id = "launcher-highlight",
  x = 0, width = WIDTH - 2 * PAD, height = ROW,
  y = function() return (math.max(1, M.selected:get()) - 1) * (ROW + ROW_GAP) end,
  behavior = { y = kit.spring(380, 26) },
  stretch = { stiffness = 300, damping = 15, scale = 0.1, max = 0.22 },
  visible = function()
    local first = M.results:get(1)
    return M.count:get() > 0 and not (first and first.kind == "calc")
  end,
}
local selection = ui.Sdf {
  id = "launcher-selection",
  anchors = { fill = true }, z = -1,
  ui.SdfShape {
    shape = "box", radius = 14, track = highlight,
    fill_color = function() return C.onSurface:alpha(0.15) end,
  },
}

local content = ui.Item {
  anchors = { fill = true },
  -- The results, bottom-up from the search field.
  ui.Item {
    y = PAD, width = WIDTH - 2 * PAD,
    anchors = { horizontal_center = true },
    height = function() return list_height(M.count:get()) end,
    visible = function() return not wide() end,
    clip = true,
    selection,
    highlight,
    ui.Repeater {
      as = "column", gap = ROW_GAP,
      model = M.results,
      delegate = delegate,
    },
    empty,
  },
  carousel(),
  kit.card {
    id = "launcher-field",
    height = SEARCH,
    anchors = { left = true, right = true, left_margin = PAD, right_margin = PAD, bottom = true, bottom_margin = 6 },
    radius = SEARCH / 2,
    kit.icon("search", 20, function() return C.onSurfaceVariant end, { x = 16, y = (SEARCH - 20) / 2 }),
    field,
    clear,
  },
}

M.drawer = drawer.new {
  name = "launcher",
  edge = "center",
  width = width,
  height = height,
  content = content,
  props = {
    behavior = {
      width = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel },
      height = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel },
    },
  },
}

-- Opening starts afresh; the field takes the keyboard while it is open.
morf.effect("caelestia.launcher.open", function()
  local open = M.drawer.open:get()
  if open then
    apps.refresh()
    field.text = ""
    M.query:set("")
    field.focus = true
    morf.surface.keyboard_focus = "exclusive"
  else
    field.focus = false
    morf.surface.keyboard_focus = "none"
  end
end)

return M
