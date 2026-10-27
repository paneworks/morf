-- Gallery samples for the display widgets: every label class, a box of
-- each kind, and the status and media pieces as an application uses them.
local ui = require("morf.ui")

local M = { span = { label = { 2, 1 }, box = { 2, 1 } } }

function M.label(_, w)
  local col = ui.Flex { direction = "column", gap = 2, width = 560, wrap = true, height = 210 }
  for _, class in ipairs { "title_1", "title_2", "title_3", "title_4", "heading", "body", "caption_heading", "caption",
    "monospace", "numeric", "dim", "accent", "success", "warning", "error" } do
    ui.reparent(w.label { text = class, variant = class }, col)
  end
  return col
end
function M.image(_, w)
  return ui.Row { gap = 12, align = "center", w.image { icon = "folder", pixel_size = 48 },
    w.image { icon = "check_circle", pixel_size = 32, variant = "success" }, w.image { width = 96, height = 64, radius = 8 } }
end
function M.avatar(_, w)
  return ui.Row { gap = 10, w.avatar { text = "Ada Lovelace", size = 48 }, w.avatar { text = "Grace Hopper", size = 32 } }
end
function M.spinner(_, w) return w.spinner { size = 32 } end
function M.progress_bar(_, w)
  return ui.Column { gap = 14, w.progress_bar { width = 240, fraction = 0.45 },
    w.progress_bar { width = 240, fraction = 0.8, text = "Copying 4 of 5" } }
end
function M.level_bar(_, w)
  return ui.Column { gap = 14, w.level_bar { width = 240, value = 0.7 },
    w.level_bar { width = 240, mode = "discrete", min_value = 0, max_value = 5, value = 2, offsets = { low = 0.25 } } }
end
function M.separator(_, w)
  return ui.Item { width = 240, height = 120,
    w.label { text = "Above" }, w.separator { y = 40, width = 240 }, w.label { y = 56, text = "Below" } }
end
function M.status_page(_, w)
  return w.status_page { width = 280, height = 220, icon_name = "search_off", title = "No results",
    description = "Try a different search.", variant = "compact" }
end
function M.shortcut_label(_, w) return w.shortcut_label { accelerator = "Ctrl+Shift+T" } end
function M.window_title(_, w) return w.window_title { title = "Documents", subtitle = "~/Projects" } end
function M.button_content(_, w) return w.button_content { icon_name = "add", label = "New document" } end
function M.box(_, w)
  local function word(text) return w.label { text = text, anchors = { center_in = true } } end
  return ui.Flex { direction = "row", wrap = true, gap = 12, width = 560,
    w.box { variant = "card", width = 120, height = 70, word("card") },
    w.box { variant = "view", width = 120, height = 70, word("view") },
    w.box { variant = "osd", width = 120, height = 70, w.label { text = "osd", color = "#ffffff", anchors = { center_in = true } } },
    w.box { variant = "toolbar", w.button { icon = "undo", variant = "flat", accessible_name = "Undo" },
      w.button { icon = "redo", variant = "flat", accessible_name = "Redo" } },
    w.box { variant = "linked", w.button { label = "Left", width = 70 }, w.button { label = "Right", width = 70 } },
    w.box { variant = "boxed_list", width = 200,
      ui.Item { width = 200, height = 40, word("First row") }, ui.Item { width = 200, height = 40, word("Second row") } } }
end
function M.frame(_, w)
  return w.frame { label = "Network", width = 240, height = 120,
    w.label { text = "Connected", anchors = { center_in = true } } }
end

return M
