-- Gallery samples for the Disclosure archetype's widgets (samples/init.lua):
-- each the widget as a configuration would make it -- a title, the
-- content it shows while open -- drawn by the theme's skin. Most start
-- open, so the gallery shows both the header and what it reveals.
local ui = require("morf.ui")

local S = {}
local W = 280

-- Content: lines of text in the kit's own type, `indent` px in.
local function body(kit, lines, indent)
  indent = indent or 14
  local column = { x = indent, y = 8, gap = 6 }
  for i, line in ipairs(lines) do
    column[#column + 1] = kit.text { text = line, width = W - indent - 12, elide = "right",
      color = i == 1 and kit.ink("hi") or kit.ink("lo") }
  end
  return ui.Item { width = W, height = #lines * 26 + 16, ui.Column(column) }
end

function S.expander(kit, widgets)
  return widgets.expander { id = "sample-expander", title = "Advanced", width = W, header_height = 44,
    expanded = true, content = body(kit, { "Hardware acceleration", "Experimental features", "Developer tools" }) }
end

function S.expander_row(kit, widgets)
  return widgets.expander_row { id = "sample-expander-row", title = "Network", subtitle = "Wired, connected",
    width = W, header_height = 52, expanded = true,
    content = body(kit, { "IPv4  192.168.1.24", "Gateway  192.168.1.1", "DNS  automatic" }, 16) }
end

function S.accordion(kit, widgets)
  local first = widgets.accordion { id = "sample-accordion-1", title = "Shipping", width = W, header_height = 40,
    group = "sample-accordion", expanded = true, content = body(kit, { "Two to four days", "Tracked parcel" }) }
  local second = widgets.accordion { id = "sample-accordion-2", title = "Returns", width = W, header_height = 40,
    group = "sample-accordion", content = body(kit, { "Thirty days", "Free of charge" }) }
  local third = widgets.accordion { id = "sample-accordion-3", title = "Warranty", width = W, header_height = 40,
    group = "sample-accordion", content = body(kit, { "Two years" }) }
  return ui.Column { width = W, gap = 0, first, second, third }
end

function S.collapsible_header(kit, widgets)
  return widgets.collapsible_header { id = "sample-collapsible-header", title = "Pinned", width = W,
    header_height = 32, expanded = true, content = body(kit, { "Notes", "Music", "Calendar" }, 10) }
end

function S.collapsible_section(kit, widgets)
  return widgets.collapsible_section { id = "sample-collapsible-section", title = "Appearance", width = W,
    header_height = 44, expanded = true, content = body(kit, { "Style  Dark", "Accent  Blue", "Fonts  Inter" }) }
end

function S.details(kit, widgets)
  return widgets.details { id = "sample-details", title = "System details", width = W, header_height = 36,
    expanded = true, content = body(kit, { "Kernel 7.2.7", "Memory 32 GiB", "Graphics Intel Xe" }, 26) }
end

function S.tree_node(kit, widgets)
  local child = widgets.tree_node { id = "sample-tree-child", title = "assets", width = W - 24, header_height = 32,
    depth = 1, content = body(kit, { "logo.svg" }, 26) }
  local leafs = ui.Column { width = W, gap = 0,
    ui.Item { width = W, height = 32, kit.text { x = 52, y = 7, text = "main.lua", color = kit.ink("lo") } },
    ui.Item { width = W, height = 32, x = 24, child },
  }
  return widgets.tree_node { id = "sample-tree-node", title = "src", width = W, header_height = 32,
    expanded = true, content = ui.Item { width = W, height = 64, leafs } }
end

function S.show_more(kit, widgets)
  return ui.Column { width = W, gap = 4,
    kit.text { text = "Release notes", color = kit.ink("hi"), width = W },
    kit.text { text = "Faster start-up and smoother scrolling.", color = kit.ink("lo"), width = W, elide = "right" },
    (widgets.show_more { id = "sample-show-more", title = "Show more", width = W, header_height = 36,
      content = body(kit, { "New widget gallery", "Sharper icons", "Fewer wake-ups" }, 0) }) }
end

function S.collapsible_card(kit, widgets)
  return widgets.collapsible_card { id = "sample-collapsible-card", title = "Storage", width = W, header_height = 48,
    expanded = true, content = body(kit, { "182 GB of 512 GB used", "Photos 64 GB", "Apps 41 GB" }) }
end

function S.fold_out(kit, widgets)
  return widgets.fold_out { id = "sample-fold-out", title = "Filters", width = W, header_height = 40,
    expanded = true, content = body(kit, { "Only unread", "From contacts", "With attachments" }) }
end

return S
