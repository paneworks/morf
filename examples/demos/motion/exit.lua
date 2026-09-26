-- Exit animations: `exit = { opacity = 0, scale = 0.9, y = 10, duration, easing }`
-- on a node is how it leaves. Whatever lets go of it -- a Loader turning
-- inactive, a list row removed, `ui.destroy` -- starts that instead of
-- removing it: the node stays drawn where it was while its properties move,
-- out of the layout (what it was pushing closes up at once) and taking no
-- input, and is removed when the animation ends. Put back before then, it
-- animates home instead.
--
-- On the left, a stack of notifications in a Repeater: each enters from the
-- right and leaves fading, shrinking and dropping. On the right, a panel in
-- a Loader that leaves the same way when it is closed.
--
--     morf render examples/demos/motion/exit.lua -o exit.png
--     morf test examples/demos/tests/exit_spec.lua
--
-- IPC: `dismiss N` removes the Nth notification, `restore` puts the last one
-- dismissed back, `toggle` opens or closes the panel, `count` says how many
-- notifications the model holds.

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.namespace = "exit"
morf.surface.anchors = {}
morf.surface.width = 640
morf.surface.height = 360

local ACCENTS = { "#7aa2f7", "#9ece6a", "#e0af68", "#f7768e" }
local notes = morf.list_model({
  { id = 1, title = "Battery at 20%", body = "Plug in soon" },
  { id = 2, title = "Download finished", body = "morf-0.2.tar.zst" },
  { id = 3, title = "Meeting in 5 minutes", body = "Design review" },
  { id = 4, title = "Wi-Fi connected", body = "home-5g" },
})
local dismissed = {}
local panel_open = morf.signal("exit.panel", true)

local function notification(note)
  return ui.Rect {
    id = ("note-%d"):format(note.id),
    width = 300, height = 64, radius = 14,
    color = "#24283b",
    border_width = 1, border_color = "#3b4261",
    -- Where the first frame starts, and how it gets from there.
    enter = { opacity = 0, translate_x = 40, duration = 260, easing = "out_cubic" },
    -- Where it goes when the model lets go of it.
    exit = { opacity = 0, scale = 0.9, y = 10, duration = 240, easing = "in_cubic" },
    ui.Rect {
      x = 14, y = 14, width = 36, height = 36, radius = 18,
      color = ACCENTS[(note.id - 1) % #ACCENTS + 1],
    },
    ui.Text { x = 62, y = 13, text = note.title, font_size = 15, color = "#c0caf5" },
    ui.Text { x = 62, y = 35, text = note.body, font_size = 12, color = "#737aa2" },
  }
end

morf.ipc.dismiss = function(n)
  n = tonumber(n) or 1
  local note = notes:get(n)
  if not note then return false end
  dismissed[#dismissed + 1] = { index = n, note = note }
  notes:remove(n)
  return true
end

morf.ipc.restore = function()
  local last = table.remove(dismissed)
  if not last then return false end
  notes:insert(math.min(last.index, notes:len() + 1), last.note)
  return true
end

morf.ipc.toggle = function()
  panel_open:set(not panel_open:get())
  return panel_open:get()
end

morf.ipc.count = function() return notes:len() end

ui.Rect {
  id = "desk",
  anchors = { fill = true },
  color = "#1a1b26",
  ui.Repeater {
    id = "notes",
    as = "column", x = 24, y = 24, gap = 10,
    model = notes,
    delegate = notification,
  },
  ui.Loader {
    id = "panel-loader",
    x = 352, y = 24,
    active = function() return panel_open:get() end,
    source = function()
      return ui.Rect {
        id = "panel",
        width = 264, height = 190, radius = 20,
        color = "#414868",
        enter = { opacity = 0, scale = 0.9, duration = 220, easing = "out_cubic" },
        exit = { opacity = 0, scale = 0.9, y = 10, duration = 220, easing = "in_cubic" },
        ui.Text { x = 20, y = 18, text = "Quick settings", font_size = 18, color = "#c0caf5" },
        ui.Rect { x = 20, y = 60, width = 104, height = 48, radius = 14, color = "#7aa2f7" },
        ui.Rect { x = 140, y = 60, width = 104, height = 48, radius = 14, color = "#565f89" },
        ui.Rect { x = 20, y = 124, width = 224, height = 44, radius = 14, color = "#565f89" },
      }
    end,
  },
}
