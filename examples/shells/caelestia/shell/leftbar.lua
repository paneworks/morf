-- Shared planner navigation and Taskwarrior polling lifetime. The selected
-- theme owns the drawer dimensions, tab strip, pages and transitions.
local morf = require("morf")
local ui = require("morf.ui")
local model = require("side_panel_model").new {
  id = "leftbar", edge = "left", tabs = {
    { key = "tasks", name = "Tasks", icon = "checklist", build = require("tasks_page").build },
    { key = "calendar", name = "Calendar", icon = "calendar_month", build = require("calendar_page").build },
  },
}
local view = require("themes").view("side_panel").build(model)
local M = { TABS = model.tabs, panel = model, WIDTH = view.width }
M.height = view.height
M.drawer = require("drawer").new {
  name = "leftbar", edge = view.edge, width = view.width, height = view.height,
  content = view.content, props = view.props,
}
function M.catcher()
  return ui.MouseArea {
    id = "leftbar-catcher", anchors = { fill = true },
    visible = function()
      -- Register containment even when the edge-hover controller is disabled.
      local inside = M.drawer.panel.contains_pointer
      return M.drawer.open:get()
    end,
    on_clicked = function()
      if not M.drawer.panel.contains_pointer then M.drawer.set(false) end
    end,
  }
end
morf.effect("caelestia.leftbar.shown", function()
  local open = M.drawer.open:get()
  model.opened:set(open)
  -- Start polling outside the effect's lifetime.
  morf.timer(1, function() require("planner").client.watch(open) end, false)
end)
return M
