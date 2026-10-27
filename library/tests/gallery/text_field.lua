-- Gallery samples for the TextField widgets.
local ui = require("morf.ui")

local M = {}

function M.entry(_, w) return (w.entry { width = 240, height = 40, label = "Name", text = "Ada", well = true }) end
function M.password_entry(_, w)
  return (w.password_entry { width = 240, height = 40, label = "Password", text = "hunter2", icon = "lock", well = true })
end
function M.search_entry(_, w)
  return (w.search_entry { width = 240, height = 36, placeholder = "Search", icon = "search", inset = { 40, 0, 34, 0 } })
end
function M.text_view(_, w)
  return (w.text_view { width = 260, height = 140, label = "Notes", text = "Buy oat milk.\nCall the plumber.",
    inset = { 12, 26, 12, 8 } })
end
function M.editable_label(_, w)
  return (w.editable_label { width = 240, height = 36, text = "Holiday photos", icon = "edit", inset = { 10, 0, 34, 0 } })
end

return M
