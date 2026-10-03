-- The default kit's looks for each Disclosure widget, in the Adwaita
-- manner: an expander row's card, an accordion's ruled rows, a sidebar's
-- dim section heading, a details triangle, a tree's folders, a "show more"
-- link, a collapsible card, a tinted fold-out. The archetype opens and
-- closes; the glue grows the control to its content on a spring. Every
-- look turns its indicator as it opens, and while the content is growing
-- or shrinking it fades with it: for that moment the control wears a mask
-- whose content part fades (nothing is masked at rest).
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local turn = M.spring(320, 36)
  local seq = 0
  local function key(name) seq = seq + 1 return "kit.default.disclosure." .. name .. "." .. seq end

  --- The content fading as it is revealed or hidden: a mask on `node`
  --- (the control) while it moves -- the header whole, the content part
  --- fading in after the header (or out) -- taken away once it settles.
  --- Returns a node to put in a slot, which owns the watch.
  local function reveal(t, node, H)
    local owner = ui.Item {}
    local was = t.expanded
    morf.effect(key("reveal"), function()
      local now = t.expanded
      if now == was then return end
      was = now
      if theme.reduced or not node then return end
      local part = ui.Rect { anchors = { fill = true, top_margin = H }, color = "#ffffff", opacity = now and 0 or 1 }
      local mask = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { left = true, right = true, top = true }, height = H, color = "#ffffff" }, part }
      node.mask = mask
      morf.animation.play { { node = part, property = "opacity", to = now and 1 or 0,
        duration = now and 320 or 140, easing = now and theme.ease.decelerate or "linear" },
        on_finished = function() if node.mask == mask then node.mask = nil end end }
    end, { owner = owner })
    return owner
  end

  local function ink() return P().ink end
  local function dim() return P().ink_dim end
  local function wash(t)
    return function()
      local p = P()
      if t.down then return p.ink:alpha(p.wash.active) end
      return t.hovered and p.ink:alpha(p.wash.hover) or p.ink:alpha(0)
    end
  end
  local function ring(t, radius, H)
    return ui.Rect { width = function() return t.width end, height = H, radius = radius, color = "transparent", z = 5,
      border_width = function() return t.visual_focus and 2 or 0 end,
      border_color = function() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end }
  end
  local function chevron(t, H, size, props)
    props = props or {}
    local item = ui.Item { x = props.x or function() return (t.width or 0) - size - 12 end, y = (H - size) / 2,
      width = size, height = size,
      rotation = function() return t.expanded and (props.open or 180) or (props.closed or 0) end,
      behavior = { rotation = turn },
      M.icon(props.icon or "expand_more", size, props.color or dim) }
    return item
  end
  local function title(spec, x, H, props)
    props = props or {}
    props.x, props.text = x, spec.title or ""
    props.y = props.y or (H - (props.font_size or theme.size.normal) * 1.4) / 2
    props.font_size = props.font_size or theme.size.normal
    props.color = props.color or ink
    if props.width then props.elide = "right" end
    return M.text(props)
  end
  local function room(t, used) return function() return math.max(0, (t.width or 0) - used) end end

  --- An expander: a flat row, the title and a chevron that turns over.
  function S.expander(t, spec, node)
    local H = spec.header_height or 44
    return {
      background = ui.Rect { width = function() return t.width end, height = H, radius = R.large, color = wash(t),
        behavior = { color = quick() } },
      header = ui.Item { width = function() return t.width end, height = H,
        title(spec, 12, H, { width = room(t, 56) }), ring(t, R.large, H) },
      indicator = ui.Item { chevron(t, H, 20), reveal(t, node, H) },
    }
  end

  --- An expander row: a boxed-list card holding the title, a dim
  --- subtitle and the chevron, which grows round the content, a hairline
  --- between them once open.
  function S.expander_row(t, spec, node)
    local H = spec.header_height or 52
    local sub = spec.subtitle
    return {
      background = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().card end,
          border_width = function() local p = P() return (p.strong and 2) or (p.dark and 0) or 1 end,
          border_color = function() return P().border end },
        ui.Rect { width = function() return t.width end, height = H, radius = R.large, color = wash(t),
          behavior = { color = quick() } } },
      header = ui.Item { width = function() return t.width end, height = H,
        title(spec, 14, H, { y = sub and (H / 2 - theme.size.normal * 1.3) or nil, width = room(t, 60) }),
        sub and M.text { x = 14, y = H / 2 + 1, text = sub, font_size = theme.size.small, color = dim,
          width = room(t, 60), elide = "right" } or nil,
        ring(t, R.large, H) },
      indicator = ui.Item { chevron(t, H, 20), reveal(t, node, H) },
      content = ui.Rect { y = H, width = function() return t.width end, height = 1,
        color = function() return P().border end, opacity = function() return t.expanded and 1 or 0 end,
        behavior = { opacity = quick() } },
    }
  end

  --- An accordion's row: ruled below, an accent bar that grows at its
  --- start and the title in the accent while open, and a plus that turns
  --- into a cross.
  function S.accordion(t, spec, node)
    local H = spec.header_height or 40
    local function bar() return t.expanded and H - 16 or 0 end
    return {
      background = ui.Item { width = function() return t.width end, height = H,
        ui.Rect { anchors = { fill = true }, color = wash(t), behavior = { color = quick() } },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1,
          color = function() local p = P() return p.strong and p.border or p.border:alpha(0.8) end },
        ui.Rect { x = 0, width = 3, radius = 1.5, height = bar, y = function() return (H - bar()) / 2 end,
          color = function() return P().accent end, behavior = { height = turn, y = turn } } },
      header = ui.Item { width = function() return t.width end, height = H,
        title(spec, 14, H, { width = room(t, 56),
          color = function() local p = P() return t.expanded and p.accent_ink or p.ink end }),
        ring(t, 0, H) },
      indicator = ui.Item { chevron(t, H, 20, { icon = "add", open = 135 }), reveal(t, node, H) },
    }
  end

  --- A collapsible heading: the sidebar's dim small bold heading with a
  --- small chevron right after it that turns down as it opens.
  function S.collapsible_header(t, spec, node)
    local H = spec.header_height or 32
    local text = title(spec, 8, H, { font_size = theme.size.small, font_weight = 700, color = dim })
    return {
      background = ui.Rect { width = function() return t.width end, height = H, radius = R.small, color = wash(t),
        behavior = { color = quick() } },
      header = ui.Item { width = function() return t.width end, height = H, text, ring(t, R.small, H) },
      indicator = ui.Item {
        chevron(t, H, 16, { icon = "expand_more", closed = -90, open = 0,
          x = function() return 14 + (text.layout_width or 0) end }),
        reveal(t, node, H) },
    }
  end

  --- A collapsible section: a large bold title over a hairline, an accent
  --- underline that draws out under the title while open, and the chevron
  --- in a round flat button.
  function S.collapsible_section(t, spec, node)
    local H = spec.header_height or 44
    local text = title(spec, 4, H, { font_size = theme.size.larger, font_weight = 700 })
    return {
      background = ui.Item { width = function() return t.width end, height = H,
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1,
          color = function() local p = P() return p.strong and p.border or p.border:alpha(0.7) end },
        ui.Rect { x = 4, anchors = { bottom = true }, height = 2, radius = 1,
          width = function() return t.expanded and (text.layout_width or 0) or 0 end,
          color = function() return P().accent end, behavior = { width = turn } } },
      header = ui.Item { width = function() return t.width end, height = H, text, ring(t, R.small, H) },
      indicator = ui.Item {
        ui.Rect { x = function() return (t.width or 0) - 36 end, y = (H - 32) / 2, width = 32, height = 32, radius = 16,
          color = wash(t), behavior = { color = quick() } },
        chevron(t, H, 20, { x = function() return (t.width or 0) - 30 end }),
        reveal(t, node, H) },
    }
  end

  --- Details: a small triangle before the title that turns down, and a
  --- rule down the side of what it shows.
  function S.details(t, spec, node)
    local H = spec.header_height or 36
    return {
      background = ui.Rect { width = function() return t.width end, height = H, radius = R.small, color = wash(t),
        behavior = { color = quick() } },
      header = ui.Item { width = function() return t.width end, height = H,
        title(spec, 30, H, { width = room(t, 40) }), ring(t, R.small, H) },
      indicator = ui.Item { chevron(t, H, 20, { x = 4, icon = "arrow_right", open = 90, color = ink }),
        reveal(t, node, H) },
      content = ui.Rect { anchors = { left = true, top = true, bottom = true, left_margin = 13, top_margin = H + 2,
        bottom_margin = 4 }, width = 2, radius = 1, color = function() return P().border end },
    }
  end

  --- A tree's node: a chevron that turns, a folder that opens, the name,
  --- and a guide down the side of its children.
  function S.tree_node(t, spec, node)
    local H = spec.header_height or 32
    return {
      background = ui.Rect { width = function() return t.width end, height = H, radius = R.small, color = wash(t),
        behavior = { color = quick() } },
      header = ui.Item { width = function() return t.width end, height = H,
        M.icon(function() return t.expanded and "folder_open" or "folder" end, 18,
          function() local p = P() return t.expanded and p.accent_ink or p.ink_dim end,
          { x = 28, anchors = { vertical_center = true }, fill = true }),
        title(spec, 54, H, { width = room(t, 60) }), ring(t, R.small, H) },
      indicator = ui.Item { chevron(t, H, 18, { x = 4, icon = "chevron_right", open = 90 }), reveal(t, node, H) },
      content = ui.Rect { anchors = { left = true, top = true, bottom = true, left_margin = 12, top_margin = H,
        bottom_margin = 2 }, width = 1, color = function() local p = P() return p.strong and p.border or p.border:alpha(0.8) end },
    }
  end

  --- "Show more": an accent link centred in a pill, its words turning
  --- into "Show less" and its chevron over as it opens.
  function S.show_more(t, spec, node)
    local H = spec.header_height or 36
    local more, less = spec.title or "Show more", spec.title_expanded or "Show less"
    local text = M.text { text = function() return t.expanded and less or more end, font_size = theme.size.normal,
      font_weight = 700, color = function() return P().accent_ink end }
    return {
      background = ui.Rect { x = function() return ((t.width or 0) - (text.layout_width or 0)) / 2 - 18 end, y = 2,
        width = function() return (text.layout_width or 0) + 52 end, height = H - 4, radius = (H - 4) / 2,
        color = wash(t), behavior = { color = quick() } },
      header = ui.Item { width = function() return t.width end, height = H,
        ui.Item { x = function() return ((t.width or 0) - (text.layout_width or 0)) / 2 - 10 end, y = (H - theme.size.normal * 1.4) / 2,
          text },
        ring(t, (H - 4) / 2, H) },
      indicator = ui.Item {
        chevron(t, H, 18, { color = function() return P().accent_ink end,
          x = function() return ((t.width or 0) + (text.layout_width or 0)) / 2 - 4 end }),
        reveal(t, node, H) },
    }
  end

  --- A collapsible card: the card itself grows round its content; a bold
  --- title, and the chevron in a round button tinted while hovered.
  function S.collapsible_card(t, spec, node)
    local H = spec.header_height or 48
    return {
      background = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().card end,
          border_width = function() local p = P() return (p.strong and 2) or 1 end,
          border_color = function() local p = P() return p.strong and p.border or p.shade:alpha(p.dark and 0.9 or 0.6) end } },
      header = ui.Item { width = function() return t.width end, height = H,
        title(spec, 16, H, { font_weight = 700, width = room(t, 64) }), ring(t, R.large, H) },
      indicator = ui.Item {
        ui.Rect { x = function() return (t.width or 0) - 44 end, y = (H - 32) / 2, width = 32, height = 32, radius = 16,
          color = function()
            local p = P()
            if t.down then return p.ink:alpha(p.wash.active) end
            return p.ink:alpha(t.hovered and p.wash.raised_hover or p.wash.hover)
          end, behavior = { color = quick() } },
        chevron(t, H, 20, { x = function() return (t.width or 0) - 38 end, color = ink }),
        reveal(t, node, H) },
    }
  end

  --- A fold-out: a band in the accent's tint with a double chevron, the
  --- panel it folds out tinted more faintly with an accent edge.
  function S.fold_out(t, spec, node)
    local H = spec.header_height or 40
    return {
      background = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, radius = R.medium,
          color = function() local p = P() return p.accent:alpha(p.dark and 0.07 or 0.06) end },
        ui.Rect { width = function() return t.width end, height = H, radius = R.medium,
          color = function()
            local p = P()
            local a = p.dark and 0.13 or 0.12
            if t.down then a = a + 0.08 elseif t.hovered then a = a + 0.04 end
            return p.accent:alpha(a)
          end, behavior = { color = quick() } },
        ui.Rect { anchors = { left = true, top = true, bottom = true, top_margin = H + 6, bottom_margin = 6 }, width = 3,
          radius = 1.5, color = function() return P().accent end,
          opacity = function() return t.expanded and 1 or 0 end, behavior = { opacity = quick() } } },
      header = ui.Item { width = function() return t.width end, height = H,
        M.icon("tune", 18, function() return P().accent_ink end, { x = 12, anchors = { vertical_center = true } }),
        title(spec, 38, H, { font_weight = 700, width = room(t, 80), color = function() return P().accent_ink end }),
        ring(t, R.medium, H) },
      indicator = ui.Item {
        chevron(t, H, 20, { icon = "keyboard_double_arrow_down", color = function() return P().accent_ink end }),
        reveal(t, node, H) },
    }
  end
end
