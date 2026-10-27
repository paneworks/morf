-- The default kit's Disclosure skin, in the Adwaita manner: a GtkExpander's
-- flat row with its chevron, an expander row's card, a revealer that is
-- only its content. The archetype opens and closes; the glue grows the
-- control to its content on a spring. Every look turns its indicator as
-- it opens, and while the content is growing or shrinking it fades with
-- it: for that moment the control wears a mask whose content part fades
-- (nothing is masked at rest).
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get, quick = draw.get, draw.quick
  local turn = draw.spring(320, 36)
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

  local function ink() return P().window_fg end
  local function dim() return P().dim_fg end
  local function wash(t)
    return function()
      local p = P()
      if t.down then return p.window_fg:alpha(p.wash.active) end
      return t.hovered and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0)
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
      draw.icon(props.icon or "expand_more", size, props.color or dim) }
    return item
  end
  local function title(spec, x, H, props)
    props = props or {}
    props.x, props.text = x, spec.title or ""
    props.y = props.y or (H - (props.font_size or theme.size.normal) * 1.4) / 2
    props.font_size = props.font_size or theme.size.normal
    props.color = props.color or ink
    if props.width then props.elide = "right" end
    return draw.text(props)
  end
  local function room(t, used) return function() return math.max(0, (t.width or 0) - used) end end

  --- GtkExpander: a flat row, the title and a chevron that turns over.
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

  --- AdwExpanderRow's header: a boxed-list card holding the title, a dim
  --- subtitle and the chevron, which grows round the content, a hairline
  --- between them once open.
  function S.expander_row(t, spec, node)
    local H = spec.header_height or 52
    local sub = spec.subtitle
    return {
      background = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().card_bg end,
          border_width = function() local p = P() return (p.strong and 2) or (p.dark and 0) or 1 end,
          border_color = function() return P().border end },
        ui.Rect { width = function() return t.width end, height = H, radius = R.large, color = wash(t),
          behavior = { color = quick() } } },
      header = ui.Item { width = function() return t.width end, height = H,
        title(spec, 14, H, { y = sub and (H / 2 - theme.size.normal * 1.3) or nil, width = room(t, 60) }),
        sub and draw.text { x = 14, y = H / 2 + 1, text = sub, font_size = theme.size.small, color = dim,
          width = room(t, 60), elide = "right" } or nil,
        ring(t, R.large, H) },
      indicator = ui.Item { chevron(t, H, 20), reveal(t, node, H) },
      content = ui.Rect { y = H, width = function() return t.width end, height = 1,
        color = function() return P().border end, opacity = function() return t.expanded and 1 or 0 end,
        behavior = { opacity = quick() } },
    }
  end

  --- GtkRevealer: no header, only its content, fading as it is revealed.
  function S.revealer(t, spec, node)
    return { background = reveal(t, node, 0), header = ui.Item {}, indicator = ui.Item {} }
  end

  --- Any other disclosure: the expander's row.
  S.Disclosure = S.expander
end
