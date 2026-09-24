-- The launcher: a search field and a list, where the first character says
-- what the search is for.
--
-- Port of LauncherPanel.qml. Text searches applications; `=` works a sum
-- out, `>` lists the shell's own panels, modes and settings, `@` the open
-- windows, `!` starts a countdown and `'` searches the clipboard history.
-- Up and Down (or the pointer) move one selection, Enter runs it, Tab steps
-- down the list or enters the mode a row names, Escape closes, and
-- Shift+Delete forgets a clipboard entry (plain Delete edits the text, and
-- a forgotten entry cannot come back). The island's height follows the
-- results when `launcherFits` is on.
--
-- `morf ipc call launcher` toggles it; `launcher_query <text>` opens it on a
-- query, `launcher_key up|down|tab|enter|escape` presses a key and
-- `launcher_results` lists what is shown, for testing without a keyboard.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local island = require("bar.island")
local kit = require("components.kit")
local launcher = require("services.launcher")
local clipboard = require("services.clipboard")
local workspaces = require("services.workspaces")
local app_icon = require("components.app_icon")

local C = theme.color
local L = launcher

local PAD = theme.panel_padding
local INNER = L.panel_width - 2 * PAD

local KEY = {
  UP = 0xff52, DOWN = 0xff54, PAGE_UP = 0xff55, PAGE_DOWN = 0xff56,
  HOME = 0xff50, END = 0xff57, LEFT_TAB = 0xfe20, DELETE = 0xffff, KP_DELETE = 0xff9f,
}

-- ---------------------------------------------------------------- state --

-- The field of the launcher that is open, nil once it has closed.
local field_node
local selected = morf.signal("impasto.launcher.selected", 1)
local first = morf.signal("impasto.launcher.first", 1)

-- How many rows the list has room for: the panel's height less the field,
-- the rule and the padding.
local function list_height()
  return L.panel_height() - 2 * PAD - L.chrome_height
end
local function visible_rows()
  return math.max(1, (list_height() + L.row_spacing) // (L.row_height + L.row_spacing))
end

local function entry_at(index) return L.results()[index] end

-- Wraps around at both ends, and scrolls to keep the selection in view.
local function select(index)
  local count = L.count:get()
  if count == 0 then
    selected:set(1)
    first:set(1)
    return
  end
  index = (index - 1) % count + 1
  selected:set(index)
  local rows = visible_rows()
  if index < first:get() then first:set(index) end
  if index > first:get() + rows - 1 then first:set(index - rows + 1) end
end

local function move(delta) select(selected:get() + delta) end

-- Back to the top whenever the results change: something is always
-- selected, so Enter always runs a result.
morf.effect("impasto.launcher.reset", function()
  L.results_revision:get()
  selected:set(1)
  first:set(1)
end)

-- Opening refreshes the application index and the window list (both are
-- otherwise read once and would miss what has appeared since); closing
-- clears the query once the island has finished shrinking, not before, or
-- the island would resize for an empty query on its way out.
local close_generation = 0
morf.effect("impasto.launcher.lifecycle", function()
  local open = island.state.open_panel() == "launcher"
  L.open:set(open)
  close_generation = close_generation + 1
  local mine = close_generation
  if open then
    morf.timer(1, function()
      L.refresh()
      workspaces.reload()
    end, false)
  else
    morf.timer(theme.duration_morph() + 60, function()
      if mine == close_generation and island.state.open_panel() ~= "launcher" then
        L.query:set("")
      end
    end, false)
  end
end)

-- Two kinds are not run by the service alone: a panel hands the island
-- over, a mode or a setting keeps the launcher open. Everything else runs
-- and the launcher closes.
local function run(entry)
  if not entry then return end
  local outcome = L.activate(entry)
  if outcome == "close" then island.close() end
end

local function run_selected() run(entry_at(selected:get())) end

-- Tab enters the mode a row names; otherwise it steps down.
local function tab()
  local entry = entry_at(selected:get())
  if entry and entry.kind == "mode" then run(entry) return end
  move(1)
end

-- Only clipboard entries can be forgotten: they are recorded without being
-- chosen, so a mistake needs a way out. The panel stays open.
local function forget_selected()
  if L.mode_for(L.query:get()).id ~= "clipboard" then return end
  local entry = entry_at(selected:get())
  if entry then clipboard.forget(entry.id) end
end

-- ---------------------------------------------------------------- badges --

-- The icon an entry is drawn with: its application's (an app, or the
-- class of a window), or "" for the kinds that take a glyph.
local function icon_name(entry)
  if not entry then return "" end
  if entry.kind == "app" then return entry.icon or "" end
  if entry.kind == "window" then return entry.app_icon or "" end
  return ""
end

local function kind_colour(kind)
  if kind == "calculation" then return C.green() end
  if kind == "action" or kind == "setting" then return C.yellow() end
  if kind == "timer" or kind == "window" then return C.blue() end
  return C.accent()
end

-- ------------------------------------------------------------------ rows --

-- One delegate per visible slot; its slot never changes, the entry it shows
-- is `first + slot - 1`. Scrolling and new results patch the same rows.
local function row(slot_row)
  local slot = slot_row.slot
  local index = function() return first:get() + slot - 1 end
  local entry = function() return entry_at(index()) end
  local field = function(name)
    return function()
      local e = entry()
      return e and tostring(e[name] or "") or ""
    end
  end
  local is_selected = function() return selected:get() == index() end
  local picture = function() local e = entry() return e and e.picture or "" end
  local icon = function() return icon_name(entry()) end
  local kind = function() local e = entry() return e and e.kind or "" end
  local sigil = function() local e = entry() return e and e.sigil or "" end
  local favourite = function()
    local e = entry()
    return e ~= nil and e.kind == "app" and L.favourites()[e.id] ~= nil
  end
  local text_width = INNER - 48 - 44

  return ui.Rect {
    width = INNER, height = L.row_height,
    radius = theme.radius_small,
    color = function() return is_selected() and C.islandSurfaceHover or "#00000000" end,
    behavior = { color = theme.behave("fast") },

    -- Applications get their themed icon, copied images a thumbnail, the
    -- rest a glyph in a tinted disc.
    ui.Item {
      x = 10, y = (L.row_height - 26) / 2, width = 26, height = 26,
      app_icon.node {
        anchors = { fill = true }, size = 26,
        name = icon,
        fallback = function()
          local e = entry()
          return e and e.kind == "window" and "󰖯" or "󰀻"
        end,
        visible = function()
          local k = kind()
          return picture() == "" and (k == "app" or k == "window")
        end,
      },
      ui.ClipRect {
        anchors = { fill = true }, radius = 26 * theme.picture_corner,
        color = C.islandSurfaceHover,
        visible = function() return picture() ~= "" end,
        ui.Image {
          anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
          source_width = 52, source_height = 52, source = picture,
        },
      },
      ui.Rect {
        anchors = { fill = true }, radius = 13, color = C.islandSurfaceHover,
        visible = function()
          local k = kind()
          return picture() == "" and k ~= "app" and k ~= "window"
        end,
        kit.glyph {
          anchors = { center_in = true }, size = 14,
          glyph = function() local e = entry() return e and e.icon or "" end,
          color = function() return kind_colour(kind()) end,
        },
      },
    },

    ui.Column {
      x = 48, y = 0, height = L.row_height, justify = "center", gap = 1,
      kit.text {
        text = field("name"), width = text_width, elide = "right",
        size = theme.size.regular,
        mono = false,
        font_family = function() return kind() == "calculation" and theme.font_mono() or theme.font() end,
        font_weight = function() return kind() == "calculation" and 600 or 400 end,
      },
      kit.text {
        text = field("subtitle"), width = text_width, elide = "right",
        size = theme.size.label, color = C.textMuted,
      },
    },

    -- Marks apps kept on the dock, which is why they rank near the top.
    kit.glyph {
      anchors = { right = true, right_margin = 12, vertical_center = true },
      glyph = "󱂩", size = 12, color = C.textMuted, visible = favourite,
    },

    -- A mode row shows its sigil as a keycap, which also teaches it.
    ui.Rect {
      anchors = { right = true, right_margin = 12, vertical_center = true },
      width = 22, height = 20, radius = theme.radius_small - 2,
      color = C.island, border_width = 1, border_color = C.islandBorder,
      visible = function() return sigil() ~= "" end,
      kit.text {
        anchors = { center_in = true }, text = sigil, mono = true,
        size = theme.size.small, weight = 600, color = C.accent,
      },
    },

    -- On movement, not on entry: a row appearing under a resting pointer
    -- would otherwise take the selection on open.
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_position_changed = function()
        if entry() and not is_selected() then selected:set(index()) end
      end,
      on_clicked = function() run(entry()) end,
    },
  }
end

-- The rows the Repeater shows: one per slot in view.
local slots = morf.list_model({})
morf.effect("impasto.launcher.slots", function()
  local count = L.count:get()
  local shown = math.max(0, math.min(visible_rows(), count - first:get() + 1))
  local rows = {}
  for slot = 1, shown do rows[slot] = { slot = slot } end
  slots:replace(rows, "slot")
end)

-- ----------------------------------------------------------------- panel --

-- What the field holds, as the field last said: compared with the query
-- without reading the node's `text`, which would make this effect follow
-- every keystroke.
local typed = ""

-- The query can be set from outside (a mode row, a sigil key, IPC); the
-- field follows it, with the caret at the end.
morf.effect("impasto.launcher.field", function()
  local query = L.query:get()
  local input = field_node
  if input and L.open:get() and typed ~= query then
    typed = query
    -- The field may be one that has closed and gone, until the next one is
    -- built; that one starts from the query.
    local ok = pcall(function()
      input.text = query
      input.cursor_position = #query
    end)
    if not ok then field_node = nil end
  end
end)

local function build()
  typed = L.query:get()
  local input
  input = ui.TextInput {
    anchors = { left = true, right = true, vertical_center = true, left_margin = 29 },
    height = L.field_height,
    vertical_alignment = "center",
    text = L.query:get(),
    placeholder = "Search…",
    placeholder_color = C.textMuted,
    font_family = function() return theme.font() end,
    font_size = 16,
    color = C.text,
    caret_color = C.accent,
    selection_color = C.accent,
    selected_text_color = C.accentText,
    focus = true,
    on_text_changed = function(text)
      typed = text
      L.query:set(text)
    end,
    on_accepted = function() run_selected() end,
    on_escape = function() island.close() end,
    on_key_pressed = function(keysym, _, modifiers)
      modifiers = modifiers or ""
      if keysym == KEY.UP then move(-1)
      elseif keysym == KEY.DOWN then move(1)
      elseif keysym == KEY.LEFT_TAB then move(-1)
      elseif keysym == KEY.PAGE_UP then move(-visible_rows())
      elseif keysym == KEY.PAGE_DOWN then move(visible_rows())
      elseif (keysym == KEY.DELETE or keysym == KEY.KP_DELETE) and modifiers:find("shift") then
        forget_selected()
      end
    end,
    -- Tab is the engine's: it moves the keyboard to the next thing that
    -- takes keys. The catcher below is that next thing, so losing the
    -- keyboard to it is a Tab; either way the field takes it back.
    on_focus_changed = function(focused)
      if focused or not L.open:get() or field_node ~= input then return end
      tab()
      input.focus = true
    end,
  }
  field_node = input

  local catcher = ui.MouseArea {
    x = 0, y = 0, width = 1, height = 1,
    on_key_pressed = function() input.focus = true end,
  }

  -- In an Item of the panel's own size: the Inset around a panel is laid
  -- out at the capsule's size, which starts small while the island morphs.
  return ui.Item {
    width = INNER,
    height = function() return L.panel_height() - 2 * PAD end,
    ui.Column {
      gap = L.gap,
      -- The field: text on the island's own black with a rule under it, the
      -- current mode's glyph instead of a magnifier.
      ui.Item {
        width = INNER, height = L.field_height,
        kit.glyph {
          anchors = { left = true, vertical_center = true },
          glyph = function() return L.mode_for(L.query:get()).icon end,
          size = 17, color = C.accent, width = 20,
        },
        input,
        catcher,
      },
      ui.Rect { width = INNER, height = 1, color = C.islandBorder },
      ui.Item {
        width = INNER,
        height = function() return math.max(L.row_height, list_height()) end,
        -- In a sigil mode, usually only the sigil has been typed so far.
        kit.text {
          anchors = { left = true, top = true, left_margin = 4 },
          visible = function() return L.count:get() == 0 and L.query:get() ~= "" end,
          text = L.empty_text, size = theme.size.small, color = C.textMuted,
          width = INNER - 8, elide = "right",
        },
        kit.text {
          anchors = { left = true, top = true, left_margin = 4 },
          visible = function() return L.count:get() == 0 and L.query:get() == "" end,
          text = "Nothing to launch: no desktop entries were found",
          size = theme.size.small, color = C.textMuted,
        },
        ui.Repeater {
          as = "column", gap = L.row_spacing,
          model = slots,
          delegate = row,
        },
        ui.MouseArea {
          anchors = { fill = true }, z = -1,
          on_wheel = function(_, _, _, _, _, steps_y)
            if not steps_y or steps_y == 0 then return end
            local most = math.max(1, L.count:get() - visible_rows() + 1)
            first:set(math.max(1, math.min(most, first:get() + steps_y)))
          end,
        },
      },
    }
  }
end

island.register("launcher", {
  size = function() return L.panel_width, L.panel_height() end,
  build = build,
})

-- --------------------------------------------------------------------- IPC --

morf.ipc.launcher_query = function(text)
  L.open_with(text or "")
  return tostring(L.count:get())
end

morf.ipc.launcher_key = function(name)
  if name == "forget" then forget_selected()
  elseif name == "up" then move(-1)
  elseif name == "down" then move(1)
  elseif name == "tab" then tab()
  elseif name == "enter" then run_selected()
  elseif name == "escape" then island.close()
  else return "keys: up down tab enter escape forget" end
  return tostring(selected:get())
end

morf.ipc.launcher_results = function()
  local lines = {}
  for position, entry in ipairs(L.results()) do
    lines[#lines + 1] = (position == selected:get() and "> " or "  ")
      .. entry.kind .. "  " .. entry.name .. "  |  " .. (entry.subtitle or "")
  end
  return table.concat(lines, "\n")
end
