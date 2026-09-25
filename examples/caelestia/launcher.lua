-- The launcher: a drawer at the bottom of the frame with a search field and
-- the best matches above it. Typing the action prefix (">") lists the
-- shell's own actions instead. Up and Down move the highlight, Return runs
-- it, Escape shuts the drawer.
--
-- Measured off the reference: 630 wide; rows 57 tall, 8 apart, a 32 px
-- icon; a 48 px search field; the drawer as tall as the rows it shows (up
-- to seven), easing to a new height as the results change.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local apps = require("apps")
local drawer = require("drawer")

local C = theme.color
local M = {}

local WIDTH = 630
local PAD = 15
local ROW, ROW_GAP = 57, 8
local SEARCH = 48

M.query = morf.signal("caelestia.launcher.query", "")
M.selected = morf.signal("caelestia.launcher.selected", 1)
M.results = morf.list_model({})
M.count = morf.signal("caelestia.launcher.count", 0)

local function max_shown() return config.get("launcher.max_shown") end

-- The results follow the query.
morf.effect("caelestia.launcher.search", function()
  local q = M.query:get()
  local found = apps.search(q, max_shown())
  local out = {}
  for i = 1, math.min(#found, max_shown()) do
    local row = found[i]
    out[i] = { key = row.kind .. ":" .. row.id, row = row }
  end
  M.results:replace(out, "key")
  M.count:set(#out)
  M.selected:set(1)
end)

local function list_height(n)
  if n <= 0 then return ROW end
  return n * ROW + (n - 1) * ROW_GAP
end

local function height()
  return PAD + list_height(M.count:get()) + 16 + SEARCH + 6
end

-- ------------------------------------------------------------------- rows --

local function row_icon(row)
  if row.kind == "action" then
    return kit.centred(32, 32, kit.icon(row.icon, 30, function() return C.onSurfaceVariant end))
  end
  local hit = apps.icon(row.icon)
  if hit and hit.name then
    return ui.Icon { width = 32, height = 32, name = hit.name, source_width = 64, source_height = 64 }
  elseif hit and hit.path then
    return ui.Image { width = 32, height = 32, source = hit.path, fill_mode = "preserve_aspect_fit" }
  end
  return kit.centred(32, 32, kit.icon("apps", 28, function() return C.onSurfaceVariant end))
end

local function delegate(entry)
  local row = entry.row
  local area
  area = ui.MouseArea {
    id = "launcher-row-" .. entry.key,
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
      color = function()
        local sel = M.results:get(M.selected:get())
        local on = sel and sel.key == entry.key
        return on and C.onSurface:alpha(0.15) or C.onSurface:alpha(0)
      end,
      behavior = { color = { duration = theme.duration.small } },
    },
    ui.Row {
      x = row.kind == "action" and 12 or 12, y = (ROW - 32) / 2, gap = row.kind == "action" and 17 or 13,
      align = "center",
      row_icon(row),
      ui.Column {
        gap = 1,
        kit.text { text = row.name, font_size = theme.size.larger, color = function() return C.onSurface end },
        kit.text {
          text = row.description, font_size = theme.size.normal,
          color = function() return C.onSurfaceVariant end,
          width = WIDTH - 2 * PAD - 80, elide = "right",
        },
      },
    },
  }
  return area
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
  local n = M.results:len()
  if n == 0 then return end
  M.selected:set(math.max(1, math.min(n, M.selected:get() + delta)))
end

field = ui.TextInput {
  id = "launcher-search",
  x = 48, width = WIDTH - 2 * PAD - 48 - 44, height = SEARCH,
  vertical_alignment = "center",
  font_family = theme.font, font_size = theme.size.larger,
  color = function() return C.onSurface end,
  placeholder = 'Type ">" for commands',
  placeholder_color = function() return C.onSurfaceVariant end,
  caret_color = function() return C.onSurface end,
  selection_color = function() return C.primary:alpha(0.4) end,
  on_text_changed = function(text) M.query:set(text) end,
  on_accepted = function() M.activate((M.results:get(M.selected:get()) or {}).row) end,
  on_escape = function() M.drawer.set(false) end,
  on_key_pressed = function(keysym)
    if keysym == "Up" then move(-1) return true end
    if keysym == "Down" then move(1) return true end
    if keysym == "Tab" then move(1) return true end
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

local content = ui.Item {
  anchors = { fill = true },
  -- The results, bottom-up from the search field.
  ui.Item {
    x = PAD, y = PAD, width = WIDTH - 2 * PAD,
    height = function() return list_height(M.count:get()) end,
    clip = true,
    ui.Repeater {
      as = "column", gap = ROW_GAP,
      model = M.results,
      delegate = delegate,
    },
    kit.text {
      id = "launcher-empty",
      anchors = { center_in = true },
      text = "No results",
      font_size = theme.size.larger,
      color = function() return C.onSurfaceVariant end,
      visible = function() return M.count:get() == 0 end,
    },
  },
  kit.card {
    id = "launcher-field",
    x = PAD, width = WIDTH - 2 * PAD, height = SEARCH,
    anchors = { bottom = true, bottom_margin = 6 },
    radius = SEARCH / 2,
    kit.icon("search", 20, function() return C.onSurfaceVariant end, { x = 16, y = (SEARCH - 20) / 2 }),
    field,
    clear,
  },
}

M.drawer = drawer.new {
  name = "launcher",
  edge = "bottom",
  width = WIDTH,
  height = height,
  content = content,
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
