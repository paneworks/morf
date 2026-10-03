-- The default kit's Canvas and Dock skins, in the Adwaita manner: a view
-- ground with a hairline grid, cards for items with the accent round what
-- is chosen, wires as soft curves, a blue band; a dock of view-coloured
-- stacks under flat tabs whose current one is raised, hairline dividers
-- that turn blue under the pointer, and a tinted plate where a tab lands.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local canvas = require("lib.kit.canvas")

  -- Points of a flat list, as path data: a polyline, closed when asked.
  local function polyline(points, closed)
    if #points < 4 then return "M0 0" end
    local parts = { ("M%.1f %.1f"):format(points[1], points[2]) }
    for i = 3, #points - 1, 2 do parts[#parts + 1] = ("L%.1f %.1f"):format(points[i], points[i + 1]) end
    if closed then parts[#parts + 1] = "Z" end
    return table.concat(parts, " ")
  end
  local function extent(points)
    local w, h = 1, 1
    for i = 1, #points - 1, 2 do w, h = math.max(w, points[i]), math.max(h, points[i + 1]) end
    return w + 1, h + 1
  end
  -- A wire between two points: a curve leaving and arriving level.
  local function curve(x0, y0, x1, y1)
    local d = math.max(30, math.abs(x1 - x0) / 2)
    return ("M%.1f %.1f C%.1f %.1f %.1f %.1f %.1f %.1f"):format(x0, y0, x0 + d, y0, x1 - d, y1, x1, y1)
  end
  -- The step a grid's lines stand at on screen: the world's, doubled
  -- until they are far enough apart to read.
  local function screen_step(t)
    local g = (t.grid and t.grid > 0) and t.grid or 50
    local step = g * (t.zoom_x or 1)
    while step < 12 do step = step * 2 end
    while step > 96 do step = step / 2 end
    return step
  end

  --- A canvas: a view ground, a hairline grid that pans as a drawing (its
  --- lines made again only as the zoom changes the step), card items, and
  --- the accent for what is chosen, drawn and banded.
  function S.Canvas(t, spec)
    local W, H = function() return t.width or 0 end, function() return t.height or 0 end
    local grid_lines = function()
      local step = screen_step(t)
      local w, h = get(W) + step * 2, get(H) + step * 2
      local parts = {}
      for x = 0, w, step do parts[#parts + 1] = ("M%.1f 0 V%.1f"):format(x, h) end
      for y = 0, h, step do parts[#parts + 1] = ("M0 %.1f H%.1f"):format(y, w) end
      return table.concat(parts, " ")
    end
    local function offset(axis)
      local step = screen_step(t)
      local v = axis == "x" and (t.view_x or 0) * (t.zoom_x or 1) or (t.view_y or 0) * (t.zoom_y or 1)
      return -(v % step) - step
    end
    local function to_screen(x, y) return (x - t.view_x) * t.zoom_x, (y - t.view_y) * t.zoom_y end
    local show_grid = spec.show_grid
    if show_grid == nil then show_grid = spec.widget ~= "image_viewer" and spec.widget ~= "map_view" end
    local draft_d = function()
      local pts = canvas.numbers(t.draft)
      local g = t.gesture
      if g == "draw" and #pts >= 4 then
        local x0, y0 = to_screen(pts[1], pts[2])
        local x1, y1 = to_screen(pts[3], pts[4])
        if t.tool == "rect" then
          return ("M%.1f %.1f H%.1f V%.1f H%.1f Z"):format(x0, y0, x1, y1, x0)
        elseif t.tool == "ellipse" then
          local cx, cy, rx, ry = (x0 + x1) / 2, (y0 + y1) / 2, math.abs(x1 - x0) / 2, math.abs(y1 - y0) / 2
          if rx < 0.5 or ry < 0.5 then return "M0 0" end
          return ("M%.1f %.1f A%.1f %.1f 0 1 1 %.1f %.1f A%.1f %.1f 0 1 1 %.1f %.1f Z"):format(cx - rx, cy, rx, ry,
            cx + rx, cy, rx, ry, cx - rx, cy)
        end
        return ("M%.1f %.1f L%.1f %.1f"):format(x0, y0, x1, y1)
      end
      if #pts >= 2 then
        local screen = {}
        for i = 1, #pts - 1, 2 do
          local x, y = to_screen(pts[i], pts[i + 1])
          screen[#screen + 1], screen[#screen + 2] = x, y
        end
        -- A clicked-out shape reaches on to the pointer.
        if g == "draft" and t.tool ~= "freehand" and t.pointer_inside then
          local x, y = to_screen(t.pointer_x, t.pointer_y)
          screen[#screen + 1], screen[#screen + 2] = x, y
        end
        if #screen >= 4 then return polyline(screen, false) end
      end
      return "M0 0"
    end
    local wire_d = function()
      if t.gesture ~= "connect" or not spec.port_point then return "M0 0" end
      local fx, fy = spec.port_point(t.connect_from)
      if not fx then return "M0 0" end
      local x0, y0 = to_screen(fx, fy)
      local x1, y1 = to_screen(t.connect_x, t.connect_y)
      return curve(x0, y0, x1, y1)
    end
    local function band(i)
      local b = canvas.numbers(t.band)
      if #b < 4 then return 0 end
      local x0, y0 = to_screen(b[1], b[2])
      local x1, y1 = to_screen(b[3], b[4])
      return ({ x0, y0, x1 - x0, y1 - y0 })[i]
    end
    local crosshair = spec.crosshair
    if crosshair == nil then crosshair = spec.widget == "chart_inspector" or spec.widget == "timeline_track" end
    local readout = spec.zoom_readout
    if readout == nil then
      readout = spec.widget == "image_viewer" or spec.widget == "map_view" or spec.widget == "zoomable_canvas"
        or spec.widget == "node_graph"
    end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().view end },
      grid = show_grid and ui.Path { x = function() return offset("x") end, y = function() return offset("y") end,
        width = function() return get(W) + screen_step(t) * 2 end,
        height = function() return get(H) + screen_step(t) * 2 end,
        d = grid_lines, fill_color = "transparent", stroke_width = 1,
        stroke_color = function() local p = P() return p.ink:alpha(p.strong and 0.25 or (p.dark and 0.06 or 0.07)) end } or nil,
      -- An item: a card for a box, the shape itself for a line, a polygon
      -- or a point, from its own fields.
      item = function(item, s)
        local shape = item.shape or "rect"
        local function fill() local p = P() return s.item().fill or p.card end
        local function stroke()
          local p = P()
          if s.selected() then return p.accent end
          if s.hovered() then return p.ink:alpha(0.35) end
          return s.item().stroke or p.border
        end
        if shape == "line" or shape == "polygon" then
          local pts = item.points or {}
          local w, h = extent(pts)
          return ui.Path { width = w, height = h, d = function() return polyline(s.item().points or {}, shape == "polygon") end,
            fill_color = shape == "polygon" and function() return fill():alpha(0.5) end or "transparent",
            stroke_color = stroke, stroke_width = function() return (s.item().width or 2) / math.max(0.01, s.zoom()) end,
            stroke_join = "round", stroke_cap = "round" }
        elseif shape == "point" then
          local r = item.r or 6
          return ui.Rect { x = function() return (s.item().x or 0) - r / s.zoom() end,
            y = function() return (s.item().y or 0) - r / s.zoom() end,
            width = function() return 2 * r / s.zoom() end, height = function() return 2 * r / s.zoom() end,
            radius = function() return r / s.zoom() end,
            color = function() return s.item().fill or P().accent end,
            border_width = function() return 2 / s.zoom() end, border_color = function() return P().view end }
        end
        local round = shape == "ellipse" or shape == "circle"
        local holder = ui.Rect { anchors = { fill = true }, color = fill,
          radius = function() return round and math.min(s.item().w or 0, s.item().h or 0) / 2 or R.medium end,
          border_width = function() return 1 / math.max(0.01, s.zoom()) end, border_color = stroke,
          behavior = { border_color = quick() } }
        if item.label then
          ui.reparent(M.text { anchors = { fill = true, margins = 6 }, text = function() return s.item().label or "" end,
            color = function() return P().ink end, font_size = theme.size.normal, elide = "right",
            horizontal_alignment = "center", vertical_alignment = "center" }, holder)
        end
        return holder
      end,
      wires = function(points, s)
        return ui.Path { width = 1, height = 1, fill_color = "transparent", stroke_cap = "round",
          d = function()
            local x0, y0, x1, y1 = points()
            if not x0 then return "M0 0" end
            return curve(x0, y0, x1, y1)
          end,
          stroke_color = function() local p = P() return (s.wire().color and get(s.wire().color)) or p.ink_dim end,
          stroke_width = function() return 2 / math.max(0.01, s.zoom()) end }
      end,
      selection = function()
        return ui.Rect { anchors = { fill = true, margins = -3 }, color = "transparent", radius = R.medium + 3,
          border_width = 2, border_color = function() return P().accent end }
      end,
      draft = ui.Item { anchors = { fill = true },
        ui.Path { anchors = { fill = true }, d = draft_d, fill_color = "transparent", stroke_width = 2,
          stroke_join = "round", stroke_cap = "round", stroke_color = function() return P().accent end,
          visible = function() return t.gesture == "draw" or t.gesture == "draft" end },
        ui.Path { anchors = { fill = true }, d = wire_d, fill_color = "transparent", stroke_width = 2,
          stroke_cap = "round", dash = { 6, 4 },
          stroke_color = function() local p = P() return t.connect_to ~= "" and p.accent or p.ink_dim end,
          visible = function() return t.gesture == "connect" end } },
      band = ui.Rect { x = function() return band(1) end, y = function() return band(2) end,
        width = function() return band(3) end, height = function() return band(4) end,
        visible = function() return t.band ~= nil and t.gesture ~= "none" end,
        color = function() local p = P() return p.accent:alpha(p.dark and 0.18 or 0.12) end,
        border_width = 1, border_color = function() return P().accent end, radius = 2 },
      crosshair = crosshair and ui.Item { anchors = { fill = true }, visible = function() return t.pointer_inside end,
        ui.Rect { width = 1, height = H, color = function() return P().ink:alpha(0.4) end,
          x = function() return (t.pointer_x - t.view_x) * t.zoom_x end },
        ui.Rect { height = 1, width = W, color = function() return P().ink:alpha(0.4) end,
          y = function() return (t.pointer_y - t.view_y) * t.zoom_y end,
          visible = function() return spec.widget ~= "chart_inspector" and spec.widget ~= "timeline_track" end } } or nil,
      overlay = readout and ui.Rect { anchors = { right = true, bottom = true, margins = 10 }, width = 64, height = 26,
        radius = 13, color = function() return P().raised end, border_width = 1,
        border_color = function() return P().border end,
        M.text { anchors = { fill = true }, text = function() return ("%d%%"):format(math.floor((t.zoom or 1) * 100 + 0.5)) end,
          color = function() return P().ink_dim end, font_size = theme.size.small,
          horizontal_alignment = "center", vertical_alignment = "center" } } or nil,
    }
  end

  --- A dock: stacks on the view's ground under a strip of flat tabs whose
  --- current one is raised and the focused stack's underlined in blue,
  --- hairline dividers, floating panels as raised cards.
  function S.Dock(t, spec)
    local TAB = spec.tab_height or 34
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().window end },
      tab = function(s)
        local close_area
        local tab = ui.Rect { anchors = { fill = true, left_margin = 2, right_margin = 2, top_margin = 4 },
          radius = R.small,
          color = function()
            local p = P()
            if s.current() then return p.view end
            if s.hovered() then return p.ink:alpha(p.wash.hover) end
            return p.ink:alpha(0)
          end,
          border_width = function() local p = P() return (p.strong and s.current()) and 1 or 0 end,
          border_color = function() return P().border end, behavior = { color = quick() } }
        local row = ui.Row { anchors = { fill = true, left_margin = 10, right_margin = 6 }, gap = 6, align = "center" }
        if s.icon then ui.reparent(M.icon(s.icon, 16, function() return P().ink_dim end), row) end
        ui.reparent(M.text { text = s.title, font_size = theme.size.small,
          font_weight = function() return s.current() and 700 or 400 end,
          color = function() local p = P() return s.current() and p.ink or p.ink_dim end, elide = "right",
          width = function() return math.max(20, (tab.layout_width or 100) - 46) end }, row)
        ui.reparent(row, tab)
        if s.closable then
          close_area = ui.MouseArea { anchors = { right = true, vertical_center = true, right_margin = 4 },
            width = 22, height = 22, cursor = "pointer", accessible_role = "button", accessible_name = "Close " .. s.title,
            visible = function() return s.current() or s.hovered() end,
            on_clicked = function() s.close() end,
            ui.Rect { anchors = { fill = true }, radius = 11,
              color = function() local p = P() return close_area and close_area.hovered and p.ink:alpha(p.wash.hover) or p.ink:alpha(0) end },
            M.icon("close", 14, function() return P().ink_dim end, { anchors = { center_in = true } }) }
          ui.reparent(close_area, tab)
        end
        -- The focused stack's current tab: a blue line under it.
        ui.reparent(ui.Rect { anchors = { left = true, right = true, bottom = true, left_margin = 8, right_margin = 8 },
          height = 2, radius = 1, color = function() return P().accent end,
          visible = function() return s.current() and s.focused() end }, tab)
        return tab
      end,
      stack = function(s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().sidebar end },
          ui.Rect { y = TAB, anchors = { left = true, right = true, bottom = true, top = true, top_margin = TAB },
            color = function() return P().view end },
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end } }
      end,
      divider = function(s)
        local vertical = s.orientation == "vertical"
        return ui.Rect { anchors = { center_in = true },
          width = function() return vertical and 9999 or ((s.hovered() or s.dragging()) and 2 or 1) end,
          height = function() return vertical and ((s.hovered() or s.dragging()) and 2 or 1) or 9999 end,
          color = function() local p = P() return (s.hovered() or s.dragging()) and p.accent or p.border end,
          behavior = { color = quick() } }
      end,
      floating = function(s)
        return ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().raised end,
          border_width = 1, border_color = function() return P().border end, clip = true,
          ui.Rect { anchors = { left = true, right = true }, height = TAB, color = function() return P().header end },
          M.text { x = 12, height = TAB, text = s.title, font_size = theme.size.small, font_weight = 700,
            color = function() return P().ink end, vertical_alignment = "center" } }
      end,
      drop_indicator = ui.Rect { radius = R.medium, color = function() local p = P() return p.accent:alpha(0.18) end,
        border_width = 2, border_color = function() return P().accent end },
    }
  end
end
