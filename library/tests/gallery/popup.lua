-- Gallery samples for the Popup widgets: each the button that opens it, as
-- an application has one.
local ui = require("morf.ui")

local M = {}

local function opener(w, label, open)
  local node
  node = w.button { label = label, width = 160, on_clicked = function() open(node) end }
  return node
end

local function words(w, text) return w.label { text = text, width = 220, wrap = true } end

function M.popover(_, w)
  local pop = w.popover { content = ui.Item { width = 240, height = 80, z = 1, words(w, "A popover holds what a button offers.") },
    padding = 12 }
  return opener(w, "Show details", function(at) pop.toggle(at) end)
end
function M.popover_menu(_, w)
  local menu = w.popover_menu { width = 220, padding = 4, items = {
    { label = "New window", icon = "open_in_new" }, { label = "Word wrap", checked = function() return true end },
    { label = "Quit", icon = "logout" } } }
  return opener(w, "Main menu", function(at) menu.toggle(at) end)
end
function M.popover_menu_bar(_, w)
  local menu = w.popover_menu_bar { width = 200, padding = 4, items = { { label = "Open…" }, { label = "Save" } } }
  return opener(w, "File", function(at) menu.toggle(at) end)
end
function M.tooltip(_, w)
  local node = w.button { icon = "help", accessible_name = "Help", variant = "flat circular" }
  require("lib.kit.archetypes.popup").tooltip(node, "What this button does")
  return node
end
function M.dialog(_, w)
  local dialog = w.dialog { title = "Rename", body = "A new name for the folder.", width = 340,
    buttons = { { label = "Cancel" }, { label = "Rename", suggested = true } } }
  return opener(w, "Rename…", function() dialog.open() end)
end
function M.alert_dialog(_, w)
  local alert = w.alert_dialog { title = "Discard changes?", body = "What was not saved is lost.", width = 340,
    buttons = { { label = "Cancel" }, { label = "Discard", destructive = true } } }
  return opener(w, "Close document", function() alert.open() end)
end
function M.bottom_sheet(_, w)
  local sheet = w.bottom_sheet { content = ui.Item { width = 360, height = 140, z = 1, words(w, "A sheet from the bottom.") },
    width = 360, height = 160 }
  return opener(w, "Share…", function() sheet.open() end)
end

return M
