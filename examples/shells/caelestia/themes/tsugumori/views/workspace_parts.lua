local stroke = require("themes.tsugumori.strokes")
local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local C = theme.color
local V = {}
function V.read(value) return type(value) == "function" and value() or value end
function V.enter(model, root, nodes)
  local running, previous = {}, nil
  morf.effect("tsugumori.workspace." .. model.key, function()
    local active = model.active()
    if active == previous then return end
    previous = active
    for _, handle in ipairs(running) do handle:stop() end
    running = {}
    if active then
      local entries = {}
      for i, node in ipairs(nodes) do entries[i] = { node = node } end
      running = theme.motion.entries(entries, true, { delay = 280, stagger = 80 })
    else
      for _, node in ipairs(nodes) do node.opacity, node.translate_x, node.translate_y = 1, 0, 0 end
    end
  end, { owner = root })
end
function V.emblem(id, icon)
  return ui.Item { id = id, width = 88, height = 88,
    ui.Rect { x = 10, y = 10, width = 68, height = 68, color = function() return C.surfaceContainerHigh end,
      border_width = 1, border_color = function() return stroke(C,"quiet") end },
    ui.Path { anchors = { fill = true }, view_box = {0, 0, 88, 88},
      d = "M0 24 V0 H24 M64 88 H88 V64", fill_color = "transparent",
      stroke_color = function() return C.primary end, stroke_width = 2 },
    kit.icon(icon, 32, function() return C.primary end, { anchors = { center_in = true } }),
  }
end
function V.status(id, caption, width)
  return ui.Item { id = id, width = width, height = 28,
    ui.Rect { y = 10, width = 5, height = 5, color = function() return C.outline end },
    kit.section_label { x = 15, y = 4, width = function() return V.read(width) - 15 end,
      text = caption, elide = "right" },
  }
end
return V
