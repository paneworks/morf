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

-- After Raycast: a wide panel hanging from a fixed point near the top of
-- the screen, the search on top in large type, the results under it in
-- sections, an answer as a card of its own, and a bar at the foot saying
-- what Return and Tab do.
local WIDTH, WIDE = 720, 1270
local PAD = 8
local ROW, ROW_GAP = 50, 2
local HEADER = 30
local HERO = 116
local SEARCH = 60
local FOOTER = 38
local EMPTY = 90
local CAROUSEL = 203
local SLOT, SLOT_ON = 248, 304

M.query = morf.signal("caelestia.launcher.query", "")
M.selected = morf.signal("caelestia.launcher.selected", 1)
M.results = morf.list_model({})
M.count = morf.signal("caelestia.launcher.count", 0)
M.mode = morf.signal("caelestia.launcher.mode", "apps")
-- The row whose actions are listed instead of the results (Tab, Ctrl+K),
-- by key; "" for none.
M.acting = morf.signal("caelestia.launcher.acting", "")
-- The carousel's pictures: `{ path, name }` each, and how many.
M.walls = {}
M.wall_count = morf.signal("caelestia.launcher.walls", 0)

local function max_shown() return config.get("launcher.max_shown") end

local by_key = {}
local section_of -- below

--- The full row behind a model entry.
local function row_of(entry) return entry and by_key[entry.key] end

-- Which section a row belongs under: the providers name theirs; apps,
-- commands and the fallbacks are named here.
section_of = function(row, q)
  if row.section then return row.section end
  if tostring(row.id):match("^fallback:") then return "Use “" .. q .. "” with…" end
  if row.kind == "app" then return q == "" and "Suggestions" or "Applications" end
  if row.kind == "action" or row.kind == "scheme" or row.kind == "variant" then return "Commands" end
  if row.id == "address" then return "Open" end
  return "Results"
end

-- The results follow the query.
morf.effect("caelestia.launcher.search", function()
  local q = M.query:get()
  local menus = require("menus")
  local found, mode
  local acting = M.acting:get()
  local owner = acting ~= "" and by_key[acting] or nil
  if owner and owner.actions then
    -- The action panel: the chosen row's actions, filtered by the field.
    found, mode = {}, "apps"
    for i, a in ipairs(owner.actions) do
      if q == "" or a.name:lower():find(q:lower(), 1, true) then
        found[#found + 1] = { kind = "menu", id = acting .. ":action:" .. i, name = a.name,
          description = owner.name, material = a.material, run = a.run }
      end
    end
  elseif menus.source:get() ~= "" then
    found, mode = menus.search(q, max_shown())
  else
    found, mode = apps.search(q, max_shown())
  end
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
  local last_section
  local shown = 0
  for i = 1, #found do
    local row = found[i]
    local hero = row.id == "answer" or row.kind == "calc"
    if not hero and shown >= max_shown() then break end
    local section = not hero and section_of(row, q) or nil
    if section and section ~= last_section then
      out[#out + 1] = { key = "header:" .. section .. ":" .. #out, kind = "header", name = section }
    end
    last_section = section or last_section
    local key = row.kind .. ":" .. row.id
    -- A model row is plain data; the row itself (an action's function
    -- among it) stays in Lua, by key.
    by_key[key] = row
    local entry = {
      key = key .. (row.kind == "calc" and (":" .. row.name) or ""),
      kind = hero and "hero" or row.kind, id = row.id, name = row.name,
      description = row.description, icon = row.icon, material = row.material,
      swatch = row.swatch, glyph = row.glyph, question = row.question,
    }
    out[#out + 1] = entry
    by_key[entry.key] = row
    if not hero then shown = shown + 1 end
  end
  M.results:replace(out, "key")
  M.count:set(#out)
  M.wall_count:set(0)
  M.mode:set(mode)
  local first = 1
  while out[first] and out[first].kind == "header" do first = first + 1 end
  M.selected:set(out[first] and first or 1)
end)

local function entry_height(entry)
  if not entry then return ROW end
  if entry.kind == "header" then return HEADER end
  if entry.kind == "hero" then return HERO end
  return ROW
end

--- Where entry `index` starts in the list, and how tall it is.
local function entry_span(index)
  local y = 0
  for i = 1, index - 1 do y = y + entry_height(M.results:get(i)) + ROW_GAP end
  return y, entry_height(M.results:get(index))
end

local function list_height()
  local n = M.count:get()
  if n <= 0 then return EMPTY end
  local y, h = entry_span(n)
  return y + h
end

local function wide() return M.mode:get() == "wallpapers" end

local function width() return wide() and WIDE or WIDTH end
M.width = width

local function height()
  local body = wide() and CAROUSEL or list_height()
  return SEARCH + PAD + body + PAD + FOOTER
end

-- ------------------------------------------------------------------- rows --

local function row_icon(row)
  if row.glyph then
    return kit.centred(32, 32, kit.text { text = row.glyph, font_size = 26 })
  end
  if row.swatch then
    local ok, color = pcall(morf.color, row.swatch)
    return kit.centred(32, 32, ui.Rect {
      width = 28, height = 28, radius = 14, color = ok and color or row.swatch,
      border_width = 2, border_color = function() return C.outlineVariant end,
    })
  end
  if row.kind == "action" or row.kind == "variant" or row.material then
    return kit.centred(32, 32, kit.icon(row.material or row.icon, 34, function() return C.onSurfaceVariant end))
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

local function header(entry)
  return ui.Item {
    id = "launcher-header-" .. entry.key,
    width = WIDTH - 2 * PAD, height = HEADER,
    enter = { opacity = 0, duration = theme.duration.small },
    kit.text {
      x = 14, anchors = { bottom = true, bottom_margin = 6 },
      text = entry.name, font_size = theme.size.small, font_weight = 600,
      color = function() return C.onSurfaceVariant end,
    },
  }
end

--- An answer, as Raycast shows one: the question in a box, an arrow, the
--- answer in large type in another.
local function hero(entry)
  local row = row_of(entry) or entry
  local box_w = (WIDTH - 2 * PAD - 56) / 2
  local function box(x, big, label, id)
    return ui.Rect {
      x = x, y = 6, width = box_w, height = HERO - 12, radius = 16,
      color = function() return C.surfaceContainerHigh end,
      ui.Column {
        anchors = { center_in = true }, gap = 6, align = "center",
        kit.text {
          id = id, text = big, width = box_w - 24, horizontal_alignment = "center", elide = "middle",
          font_size = id and 30 or 19, font_weight = id and 700 or 500,
          color = function() return id and C.onSurface or C.onSurfaceVariant end,
        },
        kit.text {
          text = label, font_size = theme.size.small,
          color = function() return C.onSurfaceVariant end,
        },
      },
    }
  end
  local question = row.question or row.description or ""
  return ui.MouseArea {
    id = "launcher-row-" .. entry.key,
    width = WIDTH - 2 * PAD, height = HERO, cursor = "pointer",
    enter = { opacity = 0, scale = 0.97, duration = theme.duration.small, easing = theme.ease.standard_decel },
    on_clicked = function() M.activate(row) end,
    box(0, question, row.material == "currency_exchange" and "Amount" or "Question"),
    kit.icon("arrow_forward", 26, function() return C.onSurfaceVariant end, {
      x = box_w + 15, y = (HERO - 26) / 2,
    }),
    box(box_w + 56, row.name, "Return copies it", "launcher-answer"),
  }
end

local function delegate(entry)
  if entry.kind == "header" then return header(entry) end
  if entry.kind == "hero" then return hero(entry) end
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
    x = PAD, y = SEARCH + PAD, width = WIDE - 2 * PAD, height = CAROUSEL, clip = true,
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
  M.acting:set("")
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
  local at = M.selected:get()
  local step = delta > 0 and 1 or -1
  for _ = 1, math.abs(delta) do
    local next_at = at + step
    while next_at >= 1 and next_at <= n and not wide() and M.results:get(next_at).kind == "header" do
      next_at = next_at + step
    end
    if next_at < 1 or next_at > n then break end
    at = next_at
  end
  M.selected:set(at)
end

local function chosen()
  if wide() then return M.walls[M.selected:get()] end
  return row_of(M.results:get(M.selected:get()))
end

field = ui.TextInput {
  id = "launcher-search",
  tab_navigation = false,
  height = SEARCH,
  anchors = { left = true, right = true, left_margin = 56, right_margin = 52 },
  vertical_alignment = "center",
  font_family = theme.font, font_size = 21,
  color = function() return C.onSurface end,
  placeholder = "Search apps, files, the web, windows…",
  placeholder_color = function() return C.onSurfaceVariant end,
  caret_color = function() return C.onSurface end,
  selection_color = function() return C.primary:alpha(0.4) end,
  on_text_changed = function(text) M.query:set(text) end,
  on_accepted = function() M.activate(chosen()) end,
  -- A menu's page steps back to the menu first.
  on_escape = function()
    if M.acting:get() ~= "" then
      M.acting:set("")
    elseif require("menus").back() then
      field.text = ""
      M.query:set("")
    else
      M.drawer.set(false)
    end
  end,
  on_key_pressed = function(_, _, modifiers, _, key)
    if key == "Up" then move(-1) return true end
    if key == "Down" then move(1) return true end
    -- Tab or Ctrl+K: the chosen row's actions, and back.
    if key == "Tab" or (key == "k" and tostring(modifiers):find("ctrl")) then
      if M.acting:get() ~= "" then
        M.acting:set("")
      else
        local entry = M.results:get(M.selected:get())
        local r = entry and by_key[entry.key]
        if r and r.actions and #r.actions > 0 then
          M.acting:set(entry.key)
          field.text = ""
          M.query:set("")
        end
      end
      return true
    end
    if wide() and key == "Left" then move(-1) return true end
    if wide() and key == "Right" then move(1) return true end
  end,
}

local clear
clear = ui.MouseArea {
  id = "launcher-clear",
  width = 36, height = 36, cursor = "pointer",
  anchors = { right = true, right_margin = 12, top = true, top_margin = (SEARCH - 36) / 2 },
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
  x = 0, width = WIDTH - 2 * PAD,
  y = function() return (entry_span(math.max(1, M.selected:get()))) end,
  height = function() local _, h = entry_span(math.max(1, M.selected:get())) return h end,
  behavior = { y = kit.spring(380, 26), height = kit.spring(380, 26) },
  stretch = { stiffness = 300, damping = 15, scale = 0.1, max = 0.22 },
  visible = function() return M.count:get() > 0 end,
}
local selection = ui.Sdf {
  id = "launcher-selection",
  anchors = { fill = true }, z = -1,
  ui.SdfShape {
    shape = "box", radius = 12, track = highlight,
    fill_color = function() return C.onSurface:alpha(0.15) end,
  },
}

-- The bar at the foot: what is being searched, and what Return and Tab do.
local MODE_NAMES = {
  calculator = { "Calculator", "calculate" }, run = { "Run", "terminal" }, files = { "Files", "folder_open" },
  web = { "Web", "travel_explore" }, windows = { "Windows", "desktop_windows" }, system = { "System", "settings_power" },
  emoji = { "Emoji", "mood" }, clipboard = { "Clipboard", "content_paste" }, colour = { "Colour", "palette" },
}
local function mode_name()
  if M.acting:get() ~= "" then return "Actions", "bolt" end
  local menus = require("menus")
  if menus.source:get() == "apps" then return "Apps", "apps" end
  if menus.source:get() == "web" then return "Web", "language" end
  local q = M.query:get()
  local what = require("providers").PREFIXES[q:sub(1, 1)]
  if what then return MODE_NAMES[what][1], MODE_NAMES[what][2] end
  if q:sub(1, 1) == config.get("launcher.action_prefix") then return "Commands", "bolt" end
  return "Search", "search"
end
local function key_hint(key, label)
  return ui.Row {
    gap = 6, align = "center",
    kit.text { text = label, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
    ui.Rect {
      height = 22, width = math.max(26, #key * 9 + 12), radius = 6,
      color = function() return C.surfaceContainerHighest end,
      kit.text { anchors = { center_in = true }, text = key, font_size = theme.size.small, font_weight = 600,
        color = function() return C.onSurface end },
    },
  }
end
-- Tab's hint, there only while the chosen row has other actions.
local tab_hint = key_hint("Tab", "Actions")
tab_hint.visible = function()
  local sel = row_of(M.results:get(M.selected:get()))
  return sel ~= nil and sel.actions ~= nil and M.acting:get() == ""
end
local footer = ui.Item {
  id = "launcher-footer",
  anchors = { left = true, right = true, bottom = true }, height = FOOTER,
  ui.Rect { anchors = { left = true, right = true, top = true }, height = 1, color = function() return C.outlineVariant:alpha(0.5) end },
  ui.Row {
    x = 16, anchors = { vertical_center = true }, gap = 8, align = "center",
    kit.icon(function() return select(2, mode_name()) end, 18, function() return C.primary end),
    kit.text { text = function() return (mode_name()) end, font_size = theme.size.small, font_weight = 600,
      color = function() return C.onSurface end },
    kit.text {
      text = function()
        if M.query:get() ~= "" or M.acting:get() ~= "" then return "" end
        return "   = calc   / files   ? web   @ windows   ! system   : emoji"
      end,
      font_size = theme.size.small, color = function() return C.onSurfaceVariant end,
    },
  },
  ui.Row {
    anchors = { right = true, right_margin = 12, vertical_center = true }, gap = 14, align = "center",
    key_hint("↵", "Open"),
    tab_hint,
    key_hint("esc", function() return M.acting:get() ~= "" and "Back" or "Close" end),
  },
}

local content = ui.Item {
  anchors = { fill = true },
  -- The search, on top, in large type.
  ui.Item {
    id = "launcher-field",
    anchors = { left = true, right = true, top = true }, height = SEARCH,
    kit.icon("search", 24, function() return C.onSurfaceVariant end, { x = 20, y = (SEARCH - 24) / 2 }),
    field,
    clear,
  },
  ui.Rect { anchors = { left = true, right = true }, y = SEARCH, height = 1, color = function() return C.outlineVariant:alpha(0.5) end },
  -- The results, down from the search.
  ui.Item {
    y = SEARCH + PAD, width = WIDTH - 2 * PAD,
    anchors = { horizontal_center = true },
    height = function() return list_height() end,
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
  footer,
}

-- Hung from a fixed point near the top of the screen: results change the
-- panel's height downwards only, and the search never moves.
local function top_margin()
  local s = morf.screens[1]
  return math.floor(((s and s.height) or 1080) * 0.16)
end

M.drawer = drawer.new {
  name = "launcher",
  edge = "center",
  width = width,
  height = height,
  content = content,
  props = {
    anchors = { top = true, horizontal_center = true, top_margin = top_margin() },
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
    -- Shut, it is its own list again next time.
    require("menus").open("")
    M.acting:set("")
  end
end)

return M
