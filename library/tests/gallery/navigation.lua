-- Gallery samples for the Navigation widgets: each holding pages the way
-- an application does.
local ui = require("morf.ui")

local M = { span = { tab_view = { 2, 1 } } }

local function page(w, text)
  return function() return w.box { variant = "card", width = 260, height = 160,
    w.label { text = text, variant = "title_4", anchors = { center_in = true } } } end
end

function M.navigation_view(_, w)
  local node, nav = w.navigation_view { width = 260, height = 200, chrome = true, current = "settings",
    titles = { settings = "Settings", sound = "Sound" },
    pages = { settings = page(w, "Settings"), sound = page(w, "Sound") } }
  nav.push("sound")
  return node
end
function M.view_stack(_, w)
  return (w.view_stack { width = 260, height = 180, current = "a", order = { "a", "b" },
    pages = { a = page(w, "Albums"), b = page(w, "Artists") } })
end
function M.stack(_, w)
  return (w.stack { width = 260, height = 180, current = "one", transition_type = "slide_left_right",
    order = { "one", "two" }, pages = { one = page(w, "One"), two = page(w, "Two") } })
end
function M.carousel(_, w)
  return (w.carousel { width = 280, height = 210,
    pages = { page(w, "Welcome"), page(w, "Swipe on"), page(w, "Done") } })
end
function M.tab_view(_, w)
  local pages, view = w.tab_view { width = 560, height = 150,
    tabs = { { title = "Notes", icon = "description", content = page(w, "Notes") },
      { title = "Todo", icon = "check_box", content = page(w, "Todo") } } }
  local bar = require("lib.kit.composites").tab_bar { view = view, width = 560 }
  pages.y = 40
  return ui.Item { width = 560, height = 190, bar, pages }
end

return M
