local stroke = require("themes.tsugumori.strokes")
-- Framed, mechanical controls. All fills and ink come from the supplied
-- wallpaper palette; the reference's fixed red is not part of this theme.
local morf = require("morf")
local ui = require("morf.ui")
return function(theme)
  local M = require("themes.material.components")(theme)
  local C = theme.color
  local feedback = require("themes.tsugumori.interaction")(theme)
  local heading_serial = 0
  local heading_viewports = {}
  -- Capture the surrounding scroll surfaces while constructing a page, even
  -- when those surfaces are created after their children. Nested page-local
  -- viewports add to this chain instead of replacing the outer clipping area.
  function M.with_viewport(viewport, build)
    local previous = heading_viewports
    heading_viewports = {table.unpack(previous)}
    heading_viewports[#heading_viewports + 1] = viewport
    local result = table.pack(pcall(build))
    heading_viewports = previous
    if not result[1] then error(result[2], 0) end
    return table.unpack(result, 2, result.n)
  end
  function M.heading(props)
    heading_serial = heading_serial + 1
    props.id = props.id or "tsugumori-heading-" .. heading_serial
    props.font_size = theme.typography[props.level or "title"] or theme.typography.title
    props.font_weight = 500
    props.color = props.ink or function() return C.primary end
    if not props.active and props.scope then props.active = require("presentation").active(props.scope) end
    props.viewports = {table.unpack(heading_viewports)}
    if props.viewport then props.viewports[#props.viewports + 1] = props.viewport end
    return require("themes.tsugumori.heading")(theme, M, props)
  end
  function M.subtitle(props)
    props.font_size, props.font_weight = theme.typography.subtitle, 400
    props.color = props.color or function() return C.onSurfaceVariant end
    return M.text(props)
  end
  function M.menu_label(props)
    props.font_size, props.font_weight = props.font_size or theme.typography.menu, 500
    return M.text(props)
  end
  function M.section_label(props)
    props.font_size, props.font_weight = theme.typography.label, 500
    props.color = props.color or function() return C.onSurfaceVariant end
    return M.text(props)
  end
  function M.action(props)
    props.scale, props.stretch = nil, nil
    if props.behavior then props.behavior.scale = nil end
    for _,key in ipairs {"enter","exit"} do
      if type(props[key])=="table" then props[key].scale=nil end
    end
    return feedback(ui.MouseArea(props), props.id)
  end
  function M.tabs(spec) return require("themes.tsugumori.tabs")(theme, M, spec) end
  function M.tabbed(spec) return require("themes.tsugumori.tabbed")(theme, M, spec) end
  function M.surface(props)
    if props.border_width then props.border_color=function() return stroke(C,"idle") end end
    for _, key in ipairs { "radius", "top_left_radius", "top_right_radius", "bottom_left_radius", "bottom_right_radius" } do
      if props[key] ~= nil then props[key] = 0 end
      if props.behavior then props.behavior[key] = nil end
    end
    return ui.Rect(props)
  end
  local outlines = {
    circle = "M30 0 L70 0 L100 30 L100 70 L70 100 L30 100 L0 70 L0 30 Z",
    diamond = "M50 0 L100 50 L50 100 L0 50 Z",
    frame = "M12 0 L100 0 L100 88 L88 100 L0 100 L0 12 Z",
  }
  local function outline(name)
    return outlines[name] or outlines.frame
  end
  function M.shape_path(name) return outline(name) end
  function M.shape(props)
    local shape = props.shape
    props.shape, props.duration, props.easing = nil, nil, nil
    props.d = function() return outline(type(shape) == "function" and shape() or shape) end
    props.view_box = { 0, 0, 100, 100 }
    props.fill_color, props.color = props.color, nil
    return ui.Path(props)
  end
  function M.svg(name)
    return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path d="' .. outline(name) .. '"/></svg>'
  end
  function M.sdf_shape(props)
    props.shape, props.duration, props.easing = "box", nil, nil
    props.radius, props.loop = 0, nil
    return ui.SdfShape(props)
  end
  function M.loading(size, color, props)
    props=props or {}
    local active=props.active or function() return true end
    props.active=nil
    props.width,props.height=size,size
    for i=1,4 do
      props[#props+1]=ui.Rect {x=(i-1)*size/4,width=size/4-3,height=3,y=size/2,
        color=color,opacity=0.25,
        loop=function()
          if not active() then return nil end
          return {opacity={from=0.15,to=1,duration=440,delay=(i-1)*90,alternate=true}}
        end }
    end
    return ui.Item(props)
  end
  local card = M.card
  function M.card(props)
    props.radius = 0
    props[#props + 1] = ui.Rect {
      anchors = { fill = true }, color = "transparent", border_width = 1,
      border_color = function() return stroke(C,"idle") end,
    }
    for _, corner in ipairs { { left = true, top = true }, { right = true, bottom = true } } do
      props[#props + 1] = ui.Item {
        anchors = corner, width = 10, height = 10,
        ui.Rect { anchors = corner, width = 10, height = 1, color = function() return stroke(C,"corner") end },
        ui.Rect { anchors = corner, width = 1, height = 10, color = function() return stroke(C,"corner") end },
      }
    end
    return card(props)
  end
  function M.spring()
    return { duration = theme.duration.small, easing = theme.ease.standard }
  end
  M.STRETCH = nil
  local indicators = setmetatable({}, { __mode = "k" })
  function M.elastic(node, axis, _, _, start, finish)
    if indicators[node] then indicators[node]:stop() end
    indicators[node] = morf.animation.play { { parallel = {
      { node = node, property = axis, to = start, duration = 320, easing = "out_cubic" },
      { node = node, property = axis == "x" and "width" or "height", to = finish-start, duration = 320, easing = "out_cubic" },
    } } }
    return indicators[node]
  end
  function M.hover(area, color)
    ui.reparent(ui.Rect {
      anchors = { fill = true }, z = -1, radius = 0, border_width = 1,
      color = function() return color(area.hovered) end,
      border_color = function() return area.hovered and stroke(C,"focus") or stroke(C,"idle") end,
      behavior = { color = { duration = 140 }, border_color = { duration = 140 } },
    }, area)
    feedback(area)
    return area
  end
  function M.pill(spec)
    local ink = spec.ink or function() return C.onPrimaryContainer end
    local fill = spec.color or function() return C.primaryContainer end
    local function caption()
      local label = type(spec.label)=="function" and spec.label() or spec.label
      return tostring(label or ""):upper()
    end
    local function width() return type(spec.width)=="function" and spec.width() or spec.width end
    -- Measure the selected face instead of assuming a monospace advance.
    local measure = M.menu_label {text=caption,font_size=theme.typography.menu,height=18,opacity=0}
    local function natural_width()
      return math.max(1,measure.layout_width or utf8.len(caption())*theme.typography.menu*.62)
    end
    local function available() return math.max(0,width()-(spec.icon and 36 or 16)) end
    local function label_size() return math.max(9,math.min(theme.typography.menu,theme.typography.menu*available()/natural_width())) end
    local function text_width() return math.min(available(),natural_width()*label_size()/theme.typography.menu+2) end
    local strip = ui.Column { y=-36,gap=0,
      M.menu_label {text="/ / / / / /",height=18,color=ink},
      M.menu_label {text="+ | + | + |",height=18,color=ink},
      M.menu_label {text=caption,font_size=label_size,height=18,width=text_width,elide="right",color=ink},
    }
    local area = M.action {
      id = spec.id, width = spec.width, height = spec.height or 32, cursor = "pointer",
      x = spec.x, y = spec.y, anchors = spec.anchors, on_clicked = spec.on_clicked,
      measure,
      ui.Row { anchors = { center_in = true }, gap = 6, align="center",
        spec.icon and M.icon(spec.icon,17,ink) or nil,
        ui.Item {id=(spec.id or "pill").."-label",width=text_width,height=18,clip=true,visible=function() return text_width()>0 end,strip},
      },
    }
    local was,running=false,nil
    morf.effect("tsugumori.label."..(spec.id or tostring(area)),function()
      local now=area.hovered
      if was==now then return end
      was=now
      if running then running:stop() end
      if now then running=morf.animation.play { {node=strip,property="y",from=0,to=-36,duration=340,easing="out_cubic"} }
      else strip.y=-36 end
    end,{owner=area})
    return M.hover(area, function(hovered) return hovered and fill():mix(ink(), 0.12) or fill() end)
  end
  function M.switch(spec)
    local function on() return spec.on() == true end
    return M.action {
      id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors,
      width = 52, height = 32, cursor = "pointer",
      on_clicked = function() if spec.on_toggled then spec.on_toggled(not on()) end end,
      ui.Rect { anchors = { fill = true }, radius = 0, border_width = 1,
        color = function() return C.surfaceContainerHighest end,
        border_color = function() return on() and stroke(C,"focus") or stroke(C,"idle") end },
      ui.Rect { y = 5, x = function() return on() and 28 or 5 end, width = 19, height = 22,
        color = function() return on() and C.primary or C.outline end,
        behavior = { x = { duration = 150, easing = "out_cubic" } } },
    }
  end
  local meters = require("themes.tsugumori.meters")(theme, M)
  M.slider, M.bar, M.media_progress = meters.slider, meters.bar, meters.media_progress
  return M
end
