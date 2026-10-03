-- The default kit's Canvas and Dock skins, in the Adwaita manner: a view
-- ground with a hairline grid, cards for items with the accent round what
-- is chosen, wires as soft curves, a blue band; a dock of view-coloured
-- stacks under flat tabs whose current one is raised, hairline dividers
-- that turn blue under the pointer, and a tinted plate where a tab lands.
-- Each widget's own look (a node graph's nodes, a map's layers) is in
-- widgets/canvas.lua and widgets/dock.lua.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local canvas = require("lib.kit.canvas")

  -- A tone an item names, as a palette colour ("paper" is the light of a
  -- page, "ink" the text's); nil for none.
  local function tone(name)
    if not name then return nil end
    local p = P()
    if name == "paper" then return p.paper or p.on_accent end
    if name == "ink" then return p.ink end
    return p[name] or p.accent
  end
  M.canvas_tone = tone
  --- A tone's soft fill (a note, a clip, a node's header) and the ink on
  --- it: the palette's `fills`/`on_fills` when it has them (a theme's
  --- containers), else the tone washed into `ground` (the view).
  function M.canvas_fill(name, amount, ground)
    local p = P()
    if p.fills and p.fills[name or "none"] then return p.fills[name] end
    local c = tone(name)
    return c and c:mix(ground or p.view, amount or (p.dark and 0.72 or 0.84)) or p.card
  end
  function M.canvas_on_fill(name)
    local p = P()
    return (p.on_fills and p.on_fills[name or "none"]) or p.ink
  end

  local extent, centre, arrow = canvas.extent, canvas.centre, canvas.arrow_path

  --- An item drawn from its fields: a card for a box (`label`, `tone` or
  --- `fill`/`stroke`), the shape itself for a line (`width` px, `arrow`), a
  --- polygon (`label` at its middle) or a point (`r` px). `look` changes
  --- it: `fill(item)`, `stroke(item, s)`, `radius`, `ink(item)`, `border`
  --- (px), `label_size`.
  function M.canvas_item(item, s, look)
    look = look or {}
    local shape = item.shape or "rect"
    local function zoom() return math.max(0.0001, s.zoom()) end
    local function fill()
      local it = s.item()
      if look.fill then return look.fill(it) end
      if it.fill then return get(it.fill) end
      return M.canvas_fill(it.tone)
    end
    local function stroke()
      local p = P()
      local it = s.item()
      if look.stroke then return look.stroke(it, s) end
      if s.selected() then return p.accent end
      if s.hovered() then return p.ink:alpha(0.4) end
      if it.stroke then return get(it.stroke) end
      local c = tone(it.tone)
      return c and c:mix(p.view, 0.35) or p.border
    end
    local function ink() return look.ink and look.ink(s.item()) or M.canvas_on_fill(s.item().tone) end
    if shape == "line" or shape == "polygon" then
      local holder = ui.Item {}
      local function line_color()
        local it = s.item()
        if look.stroke then return look.stroke(it, s) end
        if s.selected() then return P().accent end
        return (it.stroke and get(it.stroke)) or tone(it.tone) or P().ink_dim
      end
      ui.reparent(ui.Path {
        width = function() local w = extent(s.item().points or {}) return w end,
        height = function() local _, h = extent(s.item().points or {}) return h end,
        d = function() return canvas.polyline(s.item().points or {}, shape == "polygon") end,
        fill_color = shape == "polygon" and fill or "transparent",
        stroke_color = shape == "line" and line_color or stroke,
        stroke_width = function() return (shape == "line" and (s.item().width or 2) or (look.border or 1)) / zoom() end,
        stroke_join = "round", stroke_cap = "round" }, holder)
      if shape == "line" and item.arrow then
        ui.reparent(ui.Path {
          width = function() local w = extent(s.item().points or {}) return w + 20 / zoom() end,
          height = function() local _, h = extent(s.item().points or {}) return h + 20 / zoom() end,
          d = function() return arrow(s.item().points or {}, 10 / zoom()) end,
          fill_color = line_color }, holder)
      end
      if shape == "polygon" and item.label then
        ui.reparent(M.text {
          x = function() local cx, _, w = centre(s.item().points or {}) return cx - w / 2 end,
          y = function() local _, cy, _, h = centre(s.item().points or {}) return cy - h / 2 end,
          width = function() local _, _, w = centre(s.item().points or {}) return w end,
          height = function() local _, _, _, h = centre(s.item().points or {}) return h end,
          text = function() return s.item().label or "" end, color = ink,
          font_size = look.label_size or theme.size.normal, horizontal_alignment = "center",
          vertical_alignment = "center", elide = "right" }, holder)
      end
      return holder
    elseif shape == "point" then
      local function r() return (s.item().r or 6) * (s.hovered() and 1.25 or 1) end
      return ui.Rect { x = function() return (s.item().x or 0) - r() / zoom() end,
        y = function() return (s.item().y or 0) - r() / zoom() end,
        width = function() return 2 * r() / zoom() end, height = function() return 2 * r() / zoom() end,
        radius = function() return r() / zoom() end,
        color = function() local it = s.item() return (it.fill and get(it.fill)) or tone(it.tone) or P().accent end,
        border_width = function() return 2 / zoom() end, border_color = function() return P().view end }
    end
    local round = shape == "ellipse" or shape == "circle"
    local holder = ui.Rect { anchors = { fill = true }, color = fill,
      radius = function()
        if round then return math.min(s.item().w or 0, s.item().h or 0) / 2 end
        return (look.radius or R.medium) / math.max(1, zoom())
      end,
      border_width = function() return (look.border or 1) / zoom() end, border_color = stroke,
      behavior = { border_color = quick() } }
    if item.label then
      ui.reparent(M.text { anchors = { fill = true, margins = 6 }, text = function() return s.item().label or "" end,
        color = ink, font_size = look.label_size or theme.size.normal, elide = "right",
        horizontal_alignment = "center", vertical_alignment = "center" }, holder)
    end
    return holder
  end

  --- A wire between two world points as a curve, `width` px (2), its
  --- colour `color`; its box reaches to the further point so nothing is
  --- cut.
  function M.canvas_wire(points, s, color, width)
    return ui.Path {
      width = function() local x0, _, x1 = points() return x0 and math.max(x0, x1) + 80 or 1 end,
      height = function() local _, y0, _, y1 = points() return y0 and math.max(y0, y1) + 80 or 1 end,
      fill_color = "transparent", stroke_cap = "round",
      d = function()
        local x0, y0, x1, y1 = points()
        if not x0 then return "M0 0" end
        return canvas.curve(x0, y0, x1, y1)
      end,
      stroke_color = color, stroke_width = function() return (get(width) or 2) / math.max(0.0001, s.zoom()) end }
  end

  --- A canvas: a view ground, a hairline grid that pans as a drawing (its
  --- lines made again only as the zoom changes the step), card items, and
  --- the accent for what is chosen, drawn and banded.
  function S.Canvas(t, spec)
    local W, H = function() return t.width or 0 end, function() return t.height or 0 end
    local show_grid = spec.show_grid
    if show_grid == nil then show_grid = spec.widget ~= "image_viewer" and spec.widget ~= "map_view" end
    local crosshair = spec.crosshair
    if crosshair == nil then crosshair = spec.widget == "chart_inspector" or spec.widget == "timeline_track" end
    local readout = spec.zoom_readout
    if readout == nil then
      readout = spec.widget == "zoomable_canvas" or spec.widget == "node_graph" or spec.widget == "drawing_board"
    end
    local function grid_ink(a)
      return function() local p = P() return p.ink:alpha(p.strong and a * 3.5 or (p.dark and a * 0.5 or a)) end
    end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().view end },
      grid = show_grid and ui.Item { anchors = { fill = true },
        canvas.grid(t, { stroke_width = 1, stroke_color = grid_ink(0.05) }),
        canvas.grid(t, { every = 4, stroke_width = 1, stroke_color = grid_ink(0.06) }) } or nil,
      item = function(item, s) return M.canvas_item(item, s) end,
      wires = function(points, s)
        return M.canvas_wire(points, s, function()
          return (s.wire().color and get(s.wire().color)) or P().ink_dim
        end)
      end,
      -- The outline round what is chosen springs in from a touch wider.
      selection = function(s)
        local shape = s.item().shape or "rect"
        local round = shape == "ellipse" or shape == "circle"
        local m = shape == "point" and -12 or -4
        return ui.Rect { anchors = { fill = true, margins = m }, color = "transparent",
          radius = function()
            local it = s.item()
            if shape == "point" then return math.min(12, R.large) end
            if round then return math.min(it.w or 0, it.h or 0) * s.zoom() / 2 + 4 end
            return R.medium + 4
          end,
          border_width = 2, border_color = function() return P().accent end,
          scale = 1, opacity = 1, enter = { scale = 1.06, opacity = 0 },
          behavior = { scale = M.spring(420, 22), opacity = quick() } }
      end,
      draft = ui.Item { anchors = { fill = true },
        ui.Path { anchors = { fill = true }, d = function() return canvas.draft_path(t) end,
          fill_color = function() local p = P() return t.gesture == "draw" and p.accent:alpha(0.08) or p.accent:alpha(0) end,
          stroke_width = 2, stroke_join = "round", stroke_cap = "round", stroke_color = function() return P().accent end,
          visible = function() return t.gesture == "draw" or t.gesture == "draft" end },
        ui.Path { anchors = { fill = true }, d = function() return canvas.pull_path(t, spec) end,
          fill_color = "transparent", stroke_width = 2, stroke_cap = "round", dash = { 6, 4 },
          stroke_color = function() local p = P() return t.connect_to ~= "" and p.accent or p.ink_dim end,
          visible = function() return t.gesture == "connect" end } },
      band = ui.Rect { x = function() return (select(1, canvas.band_box(t))) end,
        y = function() return (select(2, canvas.band_box(t))) end,
        width = function() return (select(3, canvas.band_box(t))) end,
        height = function() return (select(4, canvas.band_box(t))) end,
        visible = function() return t.band ~= nil and t.band ~= "" and t.gesture ~= "none" end,
        color = function() local p = P() return p.accent:alpha(p.dark and 0.18 or 0.12) end,
        border_width = 1, border_color = function() return P().accent end, radius = math.min(2, R.small) },
      crosshair = crosshair and ui.Item { anchors = { fill = true }, visible = function() return t.pointer_inside end,
        ui.Rect { width = 1, height = H, color = function() return P().ink:alpha(0.4) end,
          x = function() return (t.pointer_x - t.view_x) * t.zoom_x end },
        ui.Rect { height = 1, width = W, color = function() return P().ink:alpha(0.4) end,
          y = function() return (t.pointer_y - t.view_y) * t.zoom_y end,
          visible = function() return spec.axes ~= "x" end } } or nil,
      overlay = readout and ui.Rect { anchors = { right = true, bottom = true, margins = 10 }, width = 64, height = 28,
        radius = math.min(14, R.large), color = function() return P().raised end, border_width = 1,
        border_color = function() return P().border end,
        M.text { anchors = { fill = true }, text = function() return ("%d%%"):format(math.floor((t.zoom or 1) * 100 + 0.5)) end,
          color = function() return P().ink_dim end, font_size = theme.size.small,
          horizontal_alignment = "center", vertical_alignment = "center" } } or nil,
    }
  end

  --- A dock's tab: flat, the current one raised to the view's ground with
  --- a line under it (the accent while its stack has focus) that grows out
  --- from the middle as it is chosen. `look`: `top` (margin), `radius`,
  --- `current()` (the chosen tab's ground), `underline` (false: none;
  --- "always": also in a stack without focus).
  function M.dock_tab(s, look)
    look = look or {}
    local close_area
    local tab = ui.Rect { anchors = { fill = true, left_margin = 2, right_margin = 2, top_margin = look.top or 4 },
      radius = look.radius or R.small,
      color = function()
        local p = P()
        if s.current() then return look.current and look.current() or p.view end
        if s.hovered() then return p.ink:alpha(p.wash.hover) end
        return p.ink:alpha(0)
      end,
      border_width = function() local p = P() return (p.strong and s.current()) and 1 or 0 end,
      border_color = function() return P().border end, behavior = { color = quick() } }
    local row = ui.Row { anchors = { fill = true, left_margin = 10, right_margin = 6 }, gap = 6, align = "center" }
    if s.icon then
      ui.reparent(M.icon(s.icon, 16, function() local p = P() return s.current() and p.ink or p.ink_dim end), row)
    end
    ui.reparent(M.text { text = s.title, font_size = theme.size.small,
      -- (A tab is made anew as its stack's current changes: read once.)
      font_weight = s.current() and 700 or 400,
      color = function() local p = P() return s.current() and p.ink or p.ink_dim end, elide = "right",
      width = function() return math.max(20, (tab.layout_width or 100) - (s.icon and 64 or 42)) end,
      -- (A narrow tab with an icon shows the icon alone.)
      visible = function() return not s.icon or (tab.layout_width or 0) >= 104 end }, row)
    ui.reparent(row, tab)
    if s.closable then
      close_area = ui.MouseArea { anchors = { right = true, vertical_center = true, right_margin = 4 },
        width = 22, height = 22, cursor = "pointer", accessible_role = "button", accessible_name = "Close " .. s.title,
        visible = function() return (s.current() or s.hovered()) and (tab.layout_width or 0) >= 76 end,
        on_clicked = function() s.close() end,
        ui.Rect { anchors = { fill = true }, radius = math.min(11, R.large),
          color = function() local p = P() return close_area and close_area.hovered and p.ink:alpha(p.wash.hover) or p.ink:alpha(0) end },
        M.icon("close", 14, function() return P().ink_dim end, { anchors = { center_in = true } }) }
      ui.reparent(close_area, tab)
    end
    if look.underline ~= false then
      local function full() return math.max(0, (tab.layout_width or 0) - 16) end
      ui.reparent(ui.Rect { anchors = { bottom = true, horizontal_center = true }, height = 2, radius = 1,
        width = full, enter = { width = 0 }, behavior = { width = M.spring(380, 30) },
        color = function() local p = P() return s.focused() and p.accent or p.ink:alpha(0.3) end,
        visible = function() return s.current() and (look.underline == "always" or s.focused()) end }, tab)
    end
    return tab
  end

  --- A plate where a dragged tab would land, springing from zone to zone:
  --- `spec` (the dock's), `props` for the plate (a Rect's), `inset` (4).
  function M.dock_drop(spec, props, inset)
    inset = inset or 4
    props.x = function() local x = spec.drop_box() return x + inset end
    props.y = function() local _, y = spec.drop_box() return y + inset end
    props.width = function() local _, _, w = spec.drop_box() return math.max(0, w - inset * 2) end
    props.height = function() local _, _, _, h = spec.drop_box() return math.max(0, h - inset * 2) end
    local motion = props.motion or M.spring(520, 40)
    props.motion = nil
    props.behavior = props.behavior or {}
    for _, k in ipairs { "x", "y", "width", "height" } do props.behavior[k] = motion end
    return ui.Item { anchors = { fill = true }, ui.Rect(props) }
  end

  --- A dock: stacks on the view's ground under a strip of flat tabs,
  --- hairline dividers that turn blue and thicken under the pointer,
  --- floating panels as raised cards, a tinted plate where a dragged tab
  --- would land.
  function S.Dock(t, spec)
    local TAB = spec.tab_height or 34
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().window end },
      tab = function(s) return M.dock_tab(s) end,
      stack = function(s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().sidebar end },
          ui.Rect { anchors = { left = true, right = true, bottom = true, top = true, top_margin = TAB },
            color = function() return P().view end },
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end } }
      end,
      divider = function(s)
        local vertical = s.orientation == "vertical"
        local function on() return s.hovered() or s.dragging() end
        return ui.Rect { anchors = vertical and { left = true, right = true, vertical_center = true }
            or { top = true, bottom = true, horizontal_center = true },
          width = function() return (on() and 3 or 1) end,
          height = function() return (on() and 3 or 1) end,
          color = function() local p = P() return on() and p.accent or p.border end,
          behavior = { color = quick() } }
      end,
      floating = function(s)
        return ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().raised end,
          border_width = 1, border_color = function() return P().border end, clip = true,
          ui.Rect { anchors = { left = true, right = true }, height = TAB, color = function() return P().header end },
          M.text { x = 12, height = TAB, text = s.title, font_size = theme.size.small, font_weight = 700,
            color = function() return P().ink end, vertical_alignment = "center" } }
      end,
      drop_indicator = M.dock_drop(spec, { radius = R.medium,
        color = function() return P().accent:alpha(0.18) end,
        border_width = 2, border_color = function() return P().accent end }),
    }
  end
end
