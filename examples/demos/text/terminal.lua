-- A terminal in the shell: btop in a floating panel.
--
-- `ui.Terminal` is a real terminal — a pseudo-terminal with a program on it
-- and a VT emulator drawing its screen — so anything that runs in a terminal
-- window runs here: btop, htop, nvim, a shell. The panel around it is
-- ordinary morf UI; the title bar follows the program's own title and the
-- grid's size as bindings, and closes the panel when the program exits.
--
-- Click the panel to give it the keyboard. The wheel scrolls the history of
-- a shell, and goes to a program that asked for the mouse (btop does: click
-- its boxes). From outside:
--
--     morf examples/demos/text/terminal.lua
--     morf ipc call type q        -- keys, as if typed: quits btop
--     morf ipc call paste "text"  -- a paste, bracketed if the program wants

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.width = 1000
morf.surface.height = 640
-- No anchors: the compositor centres it, a panel floating over the desktop.
morf.surface.anchors = {}
morf.surface.keyboard_focus = "on_demand"

local paper = "#0f1218"
local ink = "#d7dae0"
local muted = "#6c7486"
local accent = "#7aa2f7"

-- btop where there is one, top where there is not.
local program = { "sh", "-c", "command -v btop >/dev/null && exec btop || exec top" }

local term
term = ui.Terminal {
  command = program,
  font_family = "monospace",
  font_size = 13,
  padding = 8,
  focus = true,
  colors = {
    foreground = ink,
    background = paper,
    cursor = accent,
    palette = {
      "#1d202f", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
      "#414868", "#ff899d", "#9fe044", "#faba4a", "#8db0ff", "#c7a9ff", "#a4daff", "#c0caf5",
    },
  },
  layout = { grow = 1 },
  on_exit = function(code)
    morf.log.info(("the terminal's program exited with %s"):format(tostring(code)))
    morf.quit()
  end,
  on_bell = function() morf.log.info("bell") end,
}

morf.ipc.type = function(text) return term:write(text) end
morf.ipc.paste = function(text) return term:paste(text) end
morf.ipc.text = function() return term:text() end

ui.Rect {
  anchors = { fill = true },
  radius = 14,
  color = paper,
  border_width = 1,
  border_color = "#262b36",
  ui.Flex {
    anchors = { fill = true },
    direction = "column",
    padding = 6,
    gap = 2,
    -- The title bar: what the program calls itself, and the grid's size.
    ui.Flex {
      direction = "row",
      align = "center",
      gap = 8,
      layout = { height = 28 },
      ui.Item { width = 4, height = 1 },
      ui.Rect {
        width = 10, height = 10, radius = 5,
        color = function() return term.running and "#9ece6a" or "#f7768e" end,
      },
      ui.Text {
        text = function() return term.title ~= "" and term.title or "btop" end,
        color = ink, font_size = 13, font_weight = 600,
        layout = { grow = 1 },
      },
      ui.Text {
        text = function() return ("%d × %d"):format(term.columns, term.rows) end,
        color = muted, font_size = 12, font_family = "monospace",
      },
      ui.Item { width = 6, height = 1 },
    },
    term,
  },
}
