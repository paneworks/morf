-- Three fields: a search box, a password, and a note several lines long.
--
-- Each is one `ui.TextInput`. The field owns its caret, its selection, its
-- scroll and its undo history, and publishes all of it as properties, so the
-- rest of the configuration reads `field.text` the way it reads anything
-- else — the counter under the note is a binding on it, nothing more.
--
-- Click a field, or Tab between them. The keys are the ones every text box
-- has: arrows and Home/End, Ctrl for words, Shift to select, Ctrl+A/C/X/V,
-- Ctrl+Z and Ctrl+Shift+Z, a double click for a word. Enter in the search
-- box accepts it; in the note it starts a new line, and Ctrl+Enter saves.
--
--     morf examples/text-input.lua

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.width = 560
morf.surface.height = 420
morf.surface.anchors = { top = true, left = true }
-- A field is no use without the keyboard; take it when clicked.
morf.surface.keyboard_focus = "on_demand"

local ink = "#e6e8ee"
local muted = "#8b90a0"
local accent = "#6d8cff"
local said = morf.signal("text-input.said", "Type, then press Enter.")

-- Characters rather than bytes: every byte that does not continue a letter.
local function characters(text)
  return select(2, text:gsub("[^\128-\191]", ""))
end

-- The frame every field sits in: brighter while it has the keyboard.
local function frame(input, height)
  return ui.Rect {
    width = 512,
    height = height,
    radius = 10,
    color = "#1c1f27",
    border_width = function() return input.focus and 2 or 1 end,
    border_color = function() return input.focus and accent or "#2c313d" end,
    behavior = { border_color = { duration = 160 } },
    ui.Inset { anchors = { fill = true }, margin = 12, input },
  }
end

-- Declared before they are built: each field's own callbacks name it.
local search, password, note
search = ui.TextInput {
  placeholder = "Search applications…",
  placeholder_color = muted,
  color = ink,
  font_size = 17,
  focus = true,
  caret_color = accent,
  selection_color = "#6d8cff55",
  on_accepted = function(text) said:set("searched for “" .. text .. "”") end,
  on_escape = function() search.text = "" end,
}

password = ui.TextInput {
  placeholder = "Password",
  placeholder_color = muted,
  color = ink,
  font_size = 17,
  password = true,
  max_length = 64,
  caret_color = accent,
  on_accepted = function(text)
    said:set(("a password of %d characters"):format(characters(text)))
    password.text = ""
  end,
}

note = ui.TextInput {
  text = "Notes wrap at the edge of the box,\nand scroll once they outgrow it.",
  color = ink,
  font_size = 15,
  line_height = 1.4,
  multiline = true,
  wrap = true,
  caret_color = accent,
  selected_text_color = "#ffffff",
  selection_color = "#6d8cffaa",
  on_accepted = function(text) said:set(("saved %d bytes of notes"):format(#text)) end,
}

ui.Rect {
  width = 560,
  height = 420,
  color = "#12141a",
  ui.Column {
    x = 24,
    y = 24,
    gap = 12,
    ui.Text { text = "Search", color = muted, font_size = 12 },
    frame(search, 46),
    ui.Text { text = "Password", color = muted, font_size = 12 },
    frame(password, 46),
    ui.Text { text = "Note", color = muted, font_size = 12 },
    frame(note, 132),
    ui.Row {
      gap = 16,
      ui.Text {
        text = function()
          return ("%d characters, line %d"):format(
            characters(note.text),
            select(2, note.text:sub(1, note.cursor_position):gsub("\n", "")) + 1
          )
        end,
        color = muted,
        font_size = 12,
      },
      ui.Text { text = function() return said:get() end, color = ink, font_size = 12 },
    },
  },
}
