-- The notes panel: a deck of every note with New first, and an open note
-- as paper.
--
-- Port of NotesPanel.qml. Opening a card fills the island with that note,
-- and the island itself becomes the paper: its black gives way to the
-- note's colour and its padding to the sheet's own (`paper`, `padding`
-- below). The title is in the UI face, the body in handwriting.
--
-- Opens with the keyboard ring on New. Arrows move between cards, Enter
-- opens one, Delete archives it, Escape closes. In a note, Escape goes back
-- to the deck when the note was opened there, and closes the island when it
-- was opened from elsewhere (an edge tab, the module, IPC). A note left
-- blank is dropped.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local notes = require("services.notes")
local deck = require("services.deck")
local kit = require("components.kit")
local pill = require("components.pill")
local sticky = require("components.sticky")

local C = theme.color

local KEY = {
  escape = 0xff1b, enter = 0xff0d, kp_enter = 0xff8d, space = 0x20, tab = 0xff09,
  left = 0xff51, up = 0xff52, right = 0xff53, down = 0xff54, delete = 0xffff,
  iso_left_tab = 0xfe20,
}

local CARD_W, CARD_H, GAP = 160, 120, 12
local PAD = theme.panel_padding
local ROOM_W = notes.panel_width - 2 * PAD
local ROOM_H = notes.panel_height - 2 * PAD
local FOOTER = 28
local VIEW_H = ROOM_H - FOOTER - 12
local COLUMNS = 3

-- Leaving the panel leaves the note (dropping it if blank), so the panel
-- opens on the deck next time.
-- The archive drawer closes with it too.
local showing_archived = morf.signal("impasto.notes.archived_shown", false)
local was_open = false
morf.effect("impasto.notes.leave", function()
  local open = island.state.open_panel() == "notes"
  if was_open and not open then
    morf.timer(1, function()
      if island.state.open_panel() ~= "notes" then
        notes.leave()
        showing_archived:set(false)
      end
    end, false)
  end
  was_open = open
end)

-- A fresh signal per page built: the pages are rebuilt each time the panel
-- opens, and each starts from its own first state.
local built = 0
local function fresh(name, value)
  built = built + 1
  return morf.signal("impasto.notes." .. name .. "." .. built, value)
end

-- New first, then the deck, then the drawer when it is open. One model for
-- every build of the page, kept current once, here.
local function cards()
  local out = { { key = "__new" } }
  for _, note in ipairs(notes.live()) do out[#out + 1] = { key = note.key } end
  if showing_archived:get() then
    for _, note in ipairs(notes.archived()) do out[#out + 1] = { key = note.key } end
  end
  return out
end
local model = morf.list_model(cards())
morf.effect("impasto.notes.cards", function() model:replace(cards(), "key") end)

local function back()
  if notes.direct() then island.close() else notes.leave() end
end

-- ------------------------------------------------------------------- deck --

local function build_deck()
  -- The keyboard's ring starts on New.
  local cursor = fresh("cursor", 1)
  local scroll = fresh("scroll", 0)
  local walking = fresh("walking", true)

  local function index_of(key)
    for i, card in ipairs(cards()) do if card.key == key then return i end end
    return 0
  end

  local function max_scroll()
    local rows = math.ceil(#cards() / COLUMNS)
    return math.max(0, rows * (CARD_H + GAP) - GAP - VIEW_H)
  end

  -- The ring's row stays in view.
  local function follow()
    local row = math.floor((cursor:get() - 1) / COLUMNS)
    local top, bottom = row * (CARD_H + GAP), row * (CARD_H + GAP) + CARD_H
    local at = scroll:get()
    if top < at then scroll:set(top)
    elseif bottom > at + VIEW_H then scroll:set(bottom - VIEW_H) end
  end

  local function open_card(key)
    if key == "__new" then notes.create("yellow", true) else notes.open(key, true) end
  end

  local function current_key()
    local card = cards()[cursor:get()]
    return card and card.key
  end

  local function walk(delta)
    walking:set(true)
    cursor:set(math.max(1, math.min(#cards(), cursor:get() + delta)))
    follow()
  end

  local function fresh_card()
    local hovered = kit.hover_signal("notes.new")
    return ui.Rect {
      width = CARD_W, height = CARD_H, radius = theme.paper_radius,
      color = function() return hovered:get() and C.islandSurfaceHover or C.islandSurface end,
      border_color = function() return (walking:get() and cursor:get() == 1) and C.accent() or C.islandBorder end,
      border_width = function() return (walking:get() and cursor:get() == 1) and 2 or 1 end,
      behavior = { color = theme.behave("fast"), border_color = theme.behave("fast") },
      ui.Column {
        anchors = { center_in = true }, gap = 6, align = "center",
        kit.glyph { text = "󰐕", size = 22, color = C.accent },
        kit.text { text = "New note", size = theme.size.small, weight = 600 },
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() notes.create("yellow", true) end,
      },
    }
  end

  local function note_card(key)
    local note = function() return notes.entry(key) end
    local current = function() return walking:get() and cursor:get() == index_of(key) end
    return ui.Item {
      width = CARD_W, height = CARD_H,
      opacity = function() local n = note() return (n and n.archived) and 0.5 or 1 end,
      sticky.build { note = note, width = CARD_W, height = CARD_H, body_size = 14 },
      ui.Rect {
        anchors = { fill = true }, radius = theme.paper_radius, color = "#00000000",
        border_width = 2,
        border_color = function() return current() and C.accent() or "#00000000" end,
        behavior = { border_color = theme.behave("fast") },
      },
      -- Where the note is shown: archived, on an edge, on the desktop.
      kit.glyph {
        anchors = { right = true, bottom = true, right_margin = 10, bottom_margin = 8 },
        size = 11, color = C.paperInkMuted,
        text = function()
          local n = note()
          if not n then return "" end
          if n.archived then return "󰁫" end
          local place = deck.placement_of(key)
          if place == "grid" then return "󰕰" end
          return place ~= "" and "󰞘" or ""
        end,
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_clicked = function() notes.open(key, true) end,
      },
    }
  end

  local grid = ui.Repeater {
    as = "grid", columns = COLUMNS, gap = GAP,
    translate_y = function() return -scroll:get() end,
    model = model,
    delegate = function(row)
      if row.key == "__new" then return fresh_card() end
      return note_card(row.key)
    end,
  }

  local keys
  keys = ui.MouseArea {
    anchors = { fill = true }, z = -1,
    focus = true,
    on_wheel = function(_, _, _, pixel_y, _, steps_y)
      local delta = (steps_y and steps_y ~= 0) and steps_y * 60 or (pixel_y or 0)
      scroll:set(math.max(0, math.min(max_scroll(), scroll:get() + delta)))
    end,
    on_key_pressed = function(keysym)
      if keysym == KEY.escape then island.close()
      elseif keysym == KEY.left then walk(-1)
      elseif keysym == KEY.right or keysym == KEY.tab then walk(1)
      elseif keysym == KEY.up then walk(-COLUMNS)
      elseif keysym == KEY.down then walk(COLUMNS)
      elseif keysym == KEY.enter or keysym == KEY.kp_enter or keysym == KEY.space then
        local key = current_key()
        if key then open_card(key) end
      elseif keysym == KEY.delete then
        local key = current_key()
        local n = key and notes.entry(key)
        if n and not n.archived then notes.archive(key, true) end
        cursor:set(math.max(1, math.min(#cards(), cursor:get())))
      end
    end,
  }

  return ui.Item {
    width = ROOM_W, height = ROOM_H,
    keys,
    ui.ClipRect {
      width = ROOM_W, height = VIEW_H, color = "#00000000",
      grid,
    },
    ui.Item {
      y = ROOM_H - FOOTER, width = ROOM_W, height = FOOTER,
      kit.text {
        anchors = { left = true, vertical_center = true },
        width = ROOM_W - 150, elide = "right",
        mono = true, size = theme.size.small, color = C.textMuted,
        text = function()
          local n, a = notes.count(), #notes.archived()
          if n == 0 and a == 0 then return "Nothing written down yet" end
          local text = n .. (n == 1 and " note" or " notes")
          if a > 0 then text = text .. " · " .. a .. " archived" end
          return text
        end,
      },
      pill.button {
        anchors = { right = true, vertical_center = true },
        visible = function() return #notes.archived() > 0 end,
        icon = "󰁫",
        text = function() return showing_archived:get() and "Hide archived" or "Show archived" end,
        active = function() return showing_archived:get() end,
        on_click = function() showing_archived:set(not showing_archived:get()) end,
      },
    },
  }
end

-- ------------------------------------------------------------------ sheet --

local function build_sheet()
  local W, H = notes.panel_width, notes.panel_height
  -- Built for one note: opening another rebuilds the sheet.
  local opened_key = notes.opened()
  local key = function() return opened_key end
  local first = notes.entry(opened_key) or { title = "", text = "" }
  local note = function() return notes.entry(opened_key) end
  local archived = function() local n = note() return n and n.archived or false end
  local paper = function() local n = note() return notes.paper_of(n and n.tint or "yellow") end
  local HEAD = 30

  local heading, writing

  local function to_title()
    if archived() then return end
    heading.focus = true
    heading.cursor_position = #heading.text
  end

  heading = ui.TextInput {
    x = PAD, y = PAD, width = W - 2 * PAD - 70, height = HEAD,
    font_family = function() return theme.font() end,
    font_size = theme.size.large, font_weight = 600,
    color = C.paperInk, caret_color = C.paperInk,
    placeholder = "Title", placeholder_color = C.paperInkMuted,
    selection_color = C.paperInk, selected_text_color = paper,
    -- Set once, not bound: text goes to the service on every key, and a
    -- binding coming back would move the caret. A new note starts on its
    -- title; one with a title, at the end of its body.
    text = first.title, cursor_position = #first.title,
    focus = first.title == "",
    read_only = archived,
    on_text_changed = function(text) notes.update(key(), { title = text }) end,
    -- Enter, Tab or Down moves on to the body.
    on_accepted = function() writing.focus = true end,
    on_escape = back,
    on_key_pressed = function(keysym)
      if keysym == KEY.tab or keysym == KEY.down then writing.focus = true end
    end,
  }

  writing = ui.TextInput {
    x = PAD, y = PAD + HEAD + 8 + 1 + 8,
    width = W - 2 * PAD, height = H - 2 * PAD - HEAD - 17 - 8 - 34,
    multiline = true, wrap = true,
    font_family = function() return theme.font_hand() end,
    font_source = function() return theme.font_hand_source() end,
    font_size = 21, line_height = 1.25,
    color = C.paperInk, caret_color = C.paperInk,
    placeholder = "Write…  a line that starts [ ] is a box",
    placeholder_color = C.paperInkMuted,
    selection_color = C.paperInk, selected_text_color = paper,
    vertical_alignment = "top",
    text = first.text, cursor_position = #first.text,
    focus = first.title ~= "",
    read_only = archived,
    on_text_changed = function(text) notes.update(key(), { text = text }) end,
    on_escape = back,
    -- Back to the title: Shift+Tab, or Up on the first line.
    on_key_pressed = function(keysym, _, modifiers)
      if keysym == KEY.iso_left_tab or (keysym == KEY.tab and tostring(modifiers or ""):find("shift")) then
        to_title()
      elseif keysym == KEY.up and not writing.text:sub(1, writing.cursor_position):find("\n") then
        to_title()
      end
    end,
  }

  local function swatch(tint)
    local current = function() local n = note() return n and n.tint == tint end
    return ui.Item {
      width = 22, height = 22,
      ui.Rect {
        anchors = { center_in = true },
        width = function() return current() and 22 or 14 end,
        height = function() return current() and 22 or 14 end,
        radius = 11, color = "#00000000",
        border_color = C.paperInk,
        border_width = function() return current() and 2 or 0 end,
        behavior = { width = theme.behave("fast"), height = theme.behave("fast") },
      },
      ui.Rect {
        anchors = { center_in = true }, width = 12, height = 12, radius = 6,
        color = function() return notes.tint_color(tint) end,
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_clicked = function() notes.set_tint(key(), tint) end,
      },
    }
  end
  local swatches = {}
  for _, tint in ipairs(notes.tints) do swatches[#swatches + 1] = swatch(tint) end

  return ui.Item {
    width = W, height = H,
    -- The whole band above the line is the title's: a press there writes
    -- at its end.
    ui.MouseArea {
      width = W, height = PAD + HEAD + 8, cursor = "text", z = -1,
      on_pressed = to_title,
    },
    heading,
    kit.text {
      anchors = { right = true, top = true, right_margin = PAD, top_margin = PAD },
      height = HEAD, vertical_alignment = "center",
      mono = true, size = theme.size.label, color = C.paperInkMuted,
      text = function() local n = note() return n and notes.age_of(n.edited) or "" end,
    },
    ui.Rect { x = PAD, y = PAD + HEAD + 8, width = W - 2 * PAD, height = 1, color = C.paperLine },
    writing,
    -- The colours and the actions. Back is the arrow, or Escape.
    ui.Row {
      x = PAD, y = H - PAD - 28, height = 28, gap = 8, align = "center",
      kit.icon_button {
        glyph = "󰁍", glyph_size = 14, diameter = 28,
        color = "#00000000", hover_color = C.paperLine, glyph_color = C.paperInk,
        on_click = back,
      },
      ui.Item { width = 6, height = 1 },
      table.unpack(swatches),
    },
    ui.Row {
      anchors = { right = true, bottom = true, right_margin = PAD, bottom_margin = PAD },
      gap = 8, align = "center",
      -- Not in the original, whose notes reach the edges by dragging on the
      -- desktop: here a note goes to the left edge, and comes back, from
      -- the sheet.
      pill.button {
        icon = "󰞘",
        text = function() return deck.placement_of(key()) ~= "" and "Off the edge" or "On the edge" end,
        visible = function() return not archived() end,
        on_click = function()
          local k = key()
          if deck.placement_of(k) ~= "" then deck.remove_note(k) else deck.place_note(k, "left") end
        end,
      },
      pill.button {
        icon = function() return archived() and "󰁭" or "󰁫" end,
        text = function() return archived() and "Restore" or "Archive" end,
        on_click = function()
          local k = key()
          notes.archive(k, not archived())
          back()
        end,
      },
      pill.button {
        icon = "󰆴", text = "Delete",
        on_click = function()
          local was_direct = notes.direct()
          notes.remove(key())
          if was_direct then island.close() end
        end,
      },
    },
  }
end

-- ------------------------------------------------------------------ panel --

island.register("notes", {
  size = function() return notes.panel_width, notes.panel_height end,
  declared = true,
  -- While a note is open the island is the note: paper to the edge, with
  -- no rim, and the sheet keeps its own margins.
  paper = function()
    if notes.opened() == "" then return nil end
    local n = notes.entry(notes.opened())
    return notes.paper_of(n and n.tint or "yellow")
  end,
  padding = function() return notes.opened() ~= "" and 0 or theme.panel_padding end,
  -- One page at a time, each built when it shows: the deck, or the sheet
  -- for the open note.
  build = function()
    return ui.Item {
      anchors = { fill = true },
      ui.Loader {
        width = ROOM_W, height = ROOM_H,
        active = function() return notes.opened() == "" end,
        source = build_deck,
      },
      ui.Loader {
        width = notes.panel_width, height = notes.panel_height,
        active = function() return notes.opened() ~= "" end,
        source = build_sheet,
      },
    }
  end,
})

-- Test and keybind verbs: `morf ipc call notes.open <key>` opens a note
-- straight onto its paper; `notes.new` a blank one.
morf.ipc["notes.open"] = function(key)
  local entry = notes.entry(key or "")
  if not entry then
    local first = notes.newest()
    key = first and first.key or ""
  end
  if key == "" then return "no note" end
  notes.open(key)
  island.open("notes")
  return key
end
morf.ipc["notes.new"] = function()
  local key = notes.create("yellow", false)
  island.open("notes")
  return key
end
