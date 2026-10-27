-- The default kit's containers: `box` and `frame`, and the state helpers
-- every control's skin is built with -- `draw.quick` and `draw.spring` (its
-- motion), `draw.wash` (the ink at a state layer's alpha), `draw.ring` (the
-- keyboard's focus ring), `draw.get` (a value or a binding's).
--
-- A box is GtkBox: children packed along `orientation` ("horizontal",
-- "vertical"; left out, placed by their own `x`, `y` and anchors) with
-- `spacing` between, and its variants libadwaita's classes -- `card` (the
-- card ground, 12 px corners, a hairline in light), `view` (the view's
-- ground), `osd` (dark and translucent), `toolbar` (6 px around and
-- between), `linked` (no room between), `boxed_list` (a card whose
-- children are rows with hairlines between).
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local press = require("lib.kit.archetypes.press")

  function draw.get(v) if type(v) == "function" then return v() end return v end
  function draw.quick() return { duration = theme.duration.small, easing = theme.ease.standard } end

  --- A spring for a `behavior`; with motion reduced, an instant step.
  --- Critically damped, as libadwaita's are: no overshoot.
  function draw.spring(stiffness, damping)
    if theme.reduced then return { duration = 0 } end
    local k = stiffness or 320
    return ui.spring { stiffness = k, damping = math.max(damping or 26, 2 * math.sqrt(k) * 0.95) }
  end

  --- The ink at the alpha the palette gives a state layer.
  function draw.wash(kind) return function() local p = P() return p.window_fg:alpha(p.wash[kind]) end end

  --- The keyboard's ring: 2 px of the focus tone just inside the edge,
  --- built the first time the control is reached (most never are).
  function draw.ring(t, radius)
    return function()
      return ui.Rect { anchors = { fill = true }, z = 50, color = "transparent", radius = radius,
        border_width = 2, border_color = function() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end,
        visible = function() return t.visual_focus end }
    end
  end

  local GROUND = {
    card = function(props)
      props.radius = props.radius or R.large
      props.color = props.color or function() return P().card_bg end
      props.border_width = function() local p = P() return (p.strong and 2) or (p.dark and 0) or 1 end
      props.border_color = function() return P().border end
    end,
    view = function(props) props.color = props.color or function() return P().view_bg end end,
    osd = function(props)
      props.radius = props.radius or R.large
      props.color = props.color or function() local p = P() return p.strong and p.window_fg or p.shade:alpha(0.82) end
    end,
  }
  GROUND.boxed_list = GROUND.card

  --- GtkBox.
  function S.box(spec)
    local props, children = {}, {}
    for k, v in pairs(spec) do
      if type(k) == "number" then children[k] = v
      elseif k ~= "variant" and k ~= "widget" and k ~= "orientation" and k ~= "spacing" then props[k] = v end
    end
    local grounded
    for class, ground in pairs(GROUND) do
      if press.has(spec, class) then ground(props) grounded = true end
    end
    local toolbar, linked, boxed = press.has(spec, "toolbar"), press.has(spec, "linked"), press.has(spec, "boxed_list")
    local orientation = spec.orientation or ((toolbar and "horizontal") or (boxed and "vertical") or nil)
    if not orientation then
      for i, child in ipairs(children) do props[i] = child end
      return (grounded and ui.Rect or ui.Item)(props)
    end
    local flex = { direction = orientation == "vertical" and "column" or "row",
      gap = (linked or boxed) and 0 or (spec.spacing or (toolbar and 6 or 0)),
      padding = toolbar and 6 or nil, align = spec.align or (orientation == "vertical" and "stretch" or "center"),
      justify = spec.justify }
    for i, child in ipairs(children) do
      if boxed and i > 1 then
        flex[#flex + 1] = ui.Rect { height = 1, color = function() return P().border end }
      end
      flex[#flex + 1] = child
    end
    if not grounded then
      for k, v in pairs(props) do flex[k] = v end
      return ui.Flex(flex)
    end
    local inner = ui.Flex(flex)
    if props.width ~= nil or props.anchors ~= nil then inner.anchors = { fill = true }
    else props.width = function() return inner.layout_width or 0 end end
    if props.height == nil and props.anchors == nil then props.height = function() return inner.layout_height or 0 end
    elseif props.height ~= nil then inner.anchors = { fill = true } end
    if boxed and props.clip == nil then props.clip = true end
    props[1] = inner
    return ui.Rect(props)
  end

  --- GtkFrame: a hairline round its child, with `label` above it.
  function S.frame(spec)
    local props, children = {}, {}
    for k, v in pairs(spec) do
      if type(k) == "number" then children[k] = v
      elseif k ~= "variant" and k ~= "widget" and k ~= "label" then props[k] = v end
    end
    local top = spec.label and 22 or 0
    local node = ui.Item(props)
    if spec.label then
      ui.reparent(draw.text { x = 2, y = 0, height = 20, text = spec.label, font_size = theme.size.small,
        font_weight = 700, color = function() return P().dim_fg end }, node)
    end
    local well = ui.Rect { anchors = { fill = true, top_margin = top }, radius = R.large, color = "transparent",
      border_width = function() return P().strong and 2 or 1 end, border_color = function() return P().border end }
    for _, child in ipairs(children) do ui.reparent(child, well) end
    ui.reparent(well, node)
    return node
  end
end
