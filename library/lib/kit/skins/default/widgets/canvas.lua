-- The default kit's looks for each Canvas widget, over the archetype's
-- skin (canvas_dock.lua), in the Adwaita manner: a node graph of cards
-- with a tinted header and port dots, a whiteboard of paper with sticky
-- notes, a diagram on a dotted grid, a map of soft layers with a scale
-- bar and its attribution, an image on a checkerboard under on-screen
-- zoom buttons, a chart with a crosshair that reads the value, a timeline
-- under a ruler with a playhead, a pixel board. Only what differs is
-- given; the archetype's skin draws the rest.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local canvas = require("lib.kit.canvas")
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local tone = M.canvas_tone

  -- A round on-screen button (the image viewer's and the map's): a
  -- translucent dark plate with a light icon, as Loupe's are.
  local function loupe_button(icon, name, action, props)
    local area
    props = props or {}
    props.width, props.height = props.width or 36, props.height or 36
    props.cursor, props.accessible_role, props.accessible_name = "pointer", "button", name
    props.on_clicked = action
    area = ui.MouseArea(props)
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = props.height / 2,
      color = function() return area.pressed and "#000000cc" or (area.hovered and "#000000b3" or "#00000099") end,
      behavior = { color = quick() } }, area)
    ui.reparent(M.icon(icon, 20, "#ffffff", { anchors = { center_in = true } }), area)
    return area
  end

  -- (A theme restyling these looks may give its own.)
  local osd_button = M.canvas_osd or loupe_button

  -- ------------------------------------------------------------ node graph --

  local HEADER, ROW = 30, 24

  --- Nodes as cards: a header tinted by the node's tone with its title,
  --- a row for each port with its dot on the edge and its name beside it,
  --- wires coloured from the port they leave.
  function S.node_graph(t, spec)
    local function ports_of(id)
      local out = {}
      for _, port in ipairs(canvas.list(spec.ports)) do if port.item == id then out[#out + 1] = port end end
      return out
    end
    return {
      item = function(item, s)
        local function zoom() return math.max(0.0001, s.zoom()) end
        local function hue() local p = P() return tone(s.item().tone) or p.accent end
        local card = ui.Rect { anchors = { fill = true }, radius = R.large, clip = false,
          color = function() return P().card end,
          border_width = function() return 1 / zoom() end,
          border_color = function()
            local p = P()
            if s.selected() then return p.accent end
            return s.hovered() and p.ink:alpha(0.3) or p.border
          end, behavior = { border_color = quick() } }
        -- (A shadow under the card: a soft darker plate a touch lower.)
        ui.reparent(ui.Rect { anchors = { fill = true, top_margin = 2, bottom_margin = -3 }, z = -1, radius = R.large,
          color = function() local p = P() return p.shade:alpha(p.dark and 0.5 or 0.35) end }, card)
        ui.reparent(ui.Rect { anchors = { left = true, right = true, top = true, margins = 1 }, height = HEADER - 1,
          radius = R.large - 1, color = function() local p = P() return M.canvas_fill(s.item().tone or "accent", p.dark and 0.6 or 0.78, p.card) end }, card)
        ui.reparent(ui.Rect { anchors = { left = true, right = true, top = true, top_margin = HEADER - 1 }, height = 1,
          color = function() return P().border end }, card)
        ui.reparent(ui.Rect { x = 12, y = HEADER / 2 - 4, width = 8, height = 8, radius = math.min(4, R.small), color = hue }, card)
        ui.reparent(M.text { x = 28, y = 0, height = HEADER, text = function() return s.item().title or s.item().label or "" end,
          font_size = theme.size.small, font_weight = 700, vertical_alignment = "center", elide = "right",
          width = function() return math.max(10, (s.item().w or 100) - 36) end,
          color = function() return M.canvas_on_fill(s.item().tone or "accent") end }, card)
        -- Ports: a dot on the edge and the name beside it.
        local holder = ui.Item {}
        local built
        morf.effect("kit.default.node." .. tostring(card), function()
          local list = ports_of(item.id)
          if built then ui.destroy(built, true) end
          built = ui.Item {}
          for _, port in ipairs(list) do
            local is_in = port.kind == "in"
            local id = port.id
            local function lx() return port.x - s.item().x end
            local function ly() return port.y - s.item().y end
            local function hot() return t.hovered_port == id or t.connect_from == id or t.connect_to == id end
            ui.reparent(M.text { x = function() return is_in and 14 or 0 end, y = function() return ly() - ROW / 2 end,
              width = function() return (s.item().w or 100) - 14 end, height = ROW, text = port.label or "",
              horizontal_alignment = is_in and "left" or "right", font_size = theme.size.small,
              vertical_alignment = "center", elide = "right", color = function() return P().ink_dim end }, built)
            local function r() return hot() and 7 or 5 end
            ui.reparent(ui.Rect { x = function() return lx() - r() end, y = function() return ly() - r() end,
              width = function() return r() * 2 end, height = function() return r() * 2 end,
              radius = function() return M.canvas_port_round and M.canvas_port_round(hot(), r()) or r() end,
              color = function() local p = P() return hot() and p.accent or (is_in and p.card or hue()) end,
              border_width = 2, border_color = function() local p = P() return hot() and p.accent or hue() end,
              behavior = { x = M.spring(520, 30), y = M.spring(520, 30), width = M.spring(520, 30), height = M.spring(520, 30),
                radius = M.spring(520, 30) } },
              built)
          end
          ui.reparent(built, holder)
        end, { owner = card })
        ui.reparent(holder, card)
        return card
      end,
      wires = function(points, s)
        local function from_item()
          local port = spec.port and spec.port(s.wire().from)
          return port and port.item
        end
        local function hue()
          local p = P()
          local id = from_item()
          for _, n in ipairs(canvas.list(spec.items)) do if n.id == id then return tone(n.tone) or p.ink_dim end end
          return p.ink_dim
        end
        local control = require("lib.kit.control")
        local function lit()
          local w = s.wire()
          local b = spec.port and (spec.port(w.to) or {}).item
          return control.has(t.selection, from_item()) or control.has(t.selection, b)
        end
        return M.canvas_wire(points, s, function()
          local p = P()
          return lit() and hue() or hue():mix(p.view, 0.35)
        end, function() return lit() and 3 or 2 end)
      end,
      selection = function(s)
        return ui.Rect { anchors = { fill = true, margins = -4 }, color = "transparent", radius = R.large + 4,
          border_width = 2, border_color = function() return P().accent end,
          scale = 1, opacity = 1, enter = { scale = 1.05, opacity = 0 },
          behavior = { scale = M.spring(420, 22), opacity = quick() } }
      end,
    }
  end

  -- ------------------------------------------------------------ whiteboard --

  --- Paper with a faint dot grid; sticky notes in their tone with a soft
  --- shadow; strokes in their tone, as drawn; a frame as a dashed outline.
  function S.whiteboard(t, spec)
    return {
      background = ui.Rect { anchors = { fill = true },
        color = function() local p = P() return p.dark and p.view or p.on_accent:mix(p.warning, 0.03) end },
      grid = canvas.grid(t, { kind = "dots", base = 24, lo = 18, hi = 48, stroke_width = 2,
        stroke_color = function() local p = P() return p.ink:alpha(p.strong and 0.4 or (p.dark and 0.08 or 0.14)) end }),
      item = function(item, s)
        if item.kind == "note" then
          local function zoom() return math.max(0.0001, s.zoom()) end
          local note = ui.Rect { anchors = { fill = true }, radius = function() return 4 / math.max(1, zoom()) end,
            color = function() local p = P() return M.canvas_fill(s.item().tone or "warning", p.dark and 0.45 or 0.68, p.on_accent) end }
          ui.reparent(ui.Rect { anchors = { fill = true, top_margin = 3, bottom_margin = -4, left_margin = 2, right_margin = -2 },
            z = -1, radius = math.min(4, R.small), color = function() local p = P() return p.shade:alpha(p.dark and 0.6 or 0.45) end }, note)
          ui.reparent(M.text { anchors = { fill = true, margins = 12 }, text = function() return s.item().label or "" end,
            font_size = theme.size.normal, font_weight = 600, wrap = true, vertical_alignment = "top",
            color = function()
              local p = P()
              if p.on_fills then return M.canvas_on_fill(s.item().tone or "warning") end
              return p.dark and p.shade or p.ink
            end }, note)
          return note
        elseif item.kind == "frame" then
          local frame = ui.Item { anchors = { fill = true } }
          ui.reparent(ui.Path { anchors = { fill = true },
            d = function() local it = s.item() return ("M0 0 H%g V%g H0 Z"):format(it.w or 0, it.h or 0) end,
            fill_color = "transparent", dash = { 8, 6 },
            stroke_width = function() return 1.5 / math.max(0.0001, s.zoom()) end,
            stroke_color = function() local p = P() return s.selected() and p.accent or p.ink:alpha(0.45) end }, frame)
          ui.reparent(M.text { x = 0, y = -26, height = 22, width = function() return s.item().w or 100 end,
            text = function() return s.item().label or "" end, font_size = theme.size.small, font_weight = 700,
            color = function() return P().ink_dim end }, frame)
          return frame
        end
        return M.canvas_item(item, s)
      end,
    }
  end

  -- -------------------------------------------------------------- diagram --

  --- Shapes on a dotted grid: process boxes as white cards, decisions and
  --- terminals in their tone, connectors as dim lines with arrowheads.
  function S.diagram(t, spec)
    return {
      grid = canvas.grid(t, { kind = "dots", stroke_width = 2, lo = 14, hi = 40,
        stroke_color = function() local p = P() return p.ink:alpha(p.strong and 0.45 or (p.dark and 0.1 or 0.2)) end }),
      item = function(item, s)
        return M.canvas_item(item, s, { radius = R.small, border = 1.5, label_size = theme.size.normal })
      end,
    }
  end

  -- ------------------------------------------------------------------ map --

  -- Each layer's colours, from the palette: land is the window's ground
  -- warmed a touch, water the info tone washed out, parks the success's.
  local LAYER = {
    water = function(p) return p.info:mix(p.view, p.dark and 0.55 or 0.62) end,
    park = function(p) return p.success:mix(p.view, p.dark and 0.62 or 0.72) end,
    building = function(p) return p.ink:mix(p.window, p.dark and 0.86 or 0.9) end,
  }

  --- A map: land, water and parks as flat soft fills, buildings a shade
  --- under them, roads as light ribbons with a casing, the route in the
  --- accent, places as pins with their names; a scale bar, zoom buttons
  --- and the data's attribution in the corners.
  function S.map_view(t, spec, _, send)
    local function zoom() return math.max(1e-9, t.zoom or 1) end
    local function scale() local m, px = canvas.scale_bar(zoom(), 120) return m, px end
    return {
      background = ui.Rect { anchors = { fill = true },
        color = function() local p = P() return p.window:mix(p.warning, p.dark and 0.02 or 0.05) end },
      item = function(item, s)
        local layer = item.layer
        if layer == "road" then
          local function z() return math.max(0.0001, s.zoom()) end
          local function d() return canvas.polyline(s.item().points or {}, false) end
          local w, h = 1, 1
          for i = 1, #(item.points or {}) - 1, 2 do w, h = math.max(w, item.points[i]), math.max(h, item.points[i + 1]) end
          return ui.Item {
            ui.Path { width = w + 1, height = h + 1, d = d, fill_color = "transparent", stroke_join = "round",
              stroke_cap = "round", stroke_width = function() return ((s.item().width or 4) + 2) / z() end,
              stroke_color = function() local p = P() return p.ink:alpha(p.dark and 0.3 or 0.16) end },
            ui.Path { width = w + 1, height = h + 1, d = d, fill_color = "transparent", stroke_join = "round",
              stroke_cap = "round", stroke_width = function() return (s.item().width or 4) / z() end,
              stroke_color = function() local p = P() return p.dark and p.card or p.view end } }
        elseif layer == "route" then
          return M.canvas_item(item, s, { stroke = function(_, st)
            local p = P()
            return st.selected() and p.accent_ink or p.accent
          end })
        elseif layer == "place" then
          local pin = M.canvas_item(item, s)
          if item.label then
            local holder = ui.Item { width = 1, height = 1 }
            ui.reparent(pin, holder)
            ui.reparent(M.text { x = function() return s.item().x + 12 / math.max(0.0001, s.zoom()) end,
              y = function() return s.item().y - 10 / math.max(0.0001, s.zoom()) end,
              scale = function() return 1 / math.max(0.0001, s.zoom()) end, transform_origin_x = 0, transform_origin_y = 0,
              text = item.label, font_size = theme.size.small, font_weight = 700, height = 20,
              color = function() return P().ink end }, holder)
            return holder
          end
          return pin
        elseif LAYER[layer] then
          return M.canvas_item(item, s, { fill = function() return LAYER[layer](P()) end,
            stroke = function() local p = P() return LAYER[layer](p):mix(p.ink, 0.12) end, border = layer == "building" and 1 or 0 })
        end
        return M.canvas_item(item, s)
      end,
      overlay = ui.Item { anchors = { fill = true },
        -- Zoom in and out, about the middle.
        ui.Column { anchors = { right = true, top = true, margins = 12 }, gap = 6,
          osd_button("add", "Zoom in", function() send("zoom_by", 1.5, (t.width or 0) / 2, (t.height or 0) / 2) end),
          osd_button("remove", "Zoom out", function() send("zoom_by", 1 / 1.5, (t.width or 0) / 2, (t.height or 0) / 2) end) },
        -- The scale bar.
        ui.Item { anchors = { left = true, bottom = true, margins = 12 }, width = 150, height = 30,
          ui.Rect { x = 0, y = 22, height = 4, radius = math.min(2, R.small), width = function() local _, px = scale() return px end,
            color = function() return P().ink end, behavior = { width = quick() } },
          M.text { x = 0, y = 0, height = 20, text = function()
              local m = scale()
              return m >= 1000 and ("%g km"):format(m / 1000) or ("%d m"):format(m)
            end, font_size = theme.size.small, font_weight = 700, color = function() return P().ink end } },
        -- Where the data comes from.
        spec.attribution and ui.Rect { anchors = { right = true, bottom = true, margins = 8 }, height = 24, width = 136,
          radius = math.min(12, R.large), color = function() return P().view:alpha(0.85) end,
          M.text { anchors = { fill = true }, text = spec.attribution, font_size = theme.size.small,
            horizontal_alignment = "center", vertical_alignment = "center", color = function() return P().ink_dim end } }
          or nil },
    }
  end

  -- --------------------------------------------------------- image viewer --

  --- An image on the window's ground over a checkerboard where it is
  --- clear, annotations as accent outlines with their name on a tag, and
  --- on-screen buttons: zoom out, the zoom, zoom in, fit.
  function S.image_viewer(t, spec, _, send)
    local size = spec.image_size or { 0, 0 }
    local CHECK = 10
    -- One checkerboard, drawn once, slid by the pan as the grid is.
    local function board_box()
      local x, y = (0 - t.view_x) * t.zoom_x, (0 - t.view_y) * t.zoom_y
      -- (A pixel inside the picture's edge, so none of it peeks out.)
      return x + 1, y + 1, size[1] * t.zoom_x - 2, size[2] * t.zoom_y - 2
    end
    local checker_d = function()
      local w, h = (t.width or 0) + CHECK * 4, (t.height or 0) + CHECK * 4
      local parts = {}
      for row = 0, math.ceil(h / CHECK) do
        for col = row % 2, math.ceil(w / CHECK), 2 do
          parts[#parts + 1] = ("M%d %dh%dv%dh-%dZ"):format(col * CHECK, row * CHECK, CHECK, CHECK, CHECK)
        end
      end
      return table.concat(parts, " ")
    end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() local p = P() return p.window end },
      grid = ui.Item {
        x = function() local x = board_box() return x end, y = function() local _, y = board_box() return y end,
        width = function() local _, _, w = board_box() return math.max(0, w) end,
        height = function() local _, _, _, h = board_box() return math.max(0, h) end, clip = true,
        ui.Rect { anchors = { fill = true }, color = function() return P().view end },
        -- (Squares kept to the image's corner; the drawing covers the
        -- screen and moves only by whole pairs of squares.)
        ui.Path { x = function() local x = board_box() return (-x) - ((-x) % (CHECK * 2)) end,
          y = function() local _, y = board_box() return (-y) - ((-y) % (CHECK * 2)) end,
          width = function() return (t.width or 0) + CHECK * 4 end, height = function() return (t.height or 0) + CHECK * 4 end,
          d = checker_d, fill_color = function() local p = P() return p.ink:alpha(0.08) end } },
      item = function(item, s)
        local box = ui.Rect { anchors = { fill = true }, color = "transparent",
          radius = function() return (item.shape == "ellipse") and math.min(s.item().w or 0, s.item().h or 0) / 2 or 0 end,
          border_width = function() return 2 / math.max(0.0001, s.zoom()) end,
          border_color = function() local p = P() return (s.selected() or s.hovered()) and p.accent or (p.paper or p.on_accent):alpha(0.9) end }
        if item.label then
          ui.reparent(ui.Rect { x = 0, y = function() return -30 / math.max(0.0001, s.zoom()) end,
            width = 72, height = 24, radius = math.min(12, R.large), scale = function() return 1 / math.max(0.0001, s.zoom()) end,
            transform_origin_x = 0, transform_origin_y = 0, color = function() return P().accent end,
            M.text { anchors = { fill = true }, text = item.label, font_size = theme.size.small, font_weight = 700,
              horizontal_alignment = "center", vertical_alignment = "center", color = function() return P().on_accent end } }, box)
        end
        return box
      end,
      selection = function() return ui.Item {} end,
      overlay = ui.Row { anchors = { horizontal_center = true, bottom = true, bottom_margin = 14 }, gap = 6, align = "center",
        osd_button("zoom_out", "Zoom out", function() send("zoom_by", 1 / 1.25, (t.width or 0) / 2, (t.height or 0) / 2) end),
        (M.canvas_osd_label or function(text)
          return ui.Rect { width = 72, height = 36, radius = 18, color = "#00000099",
            M.text { anchors = { fill = true }, text = text, font_size = theme.size.small, font_weight = 700,
              color = "#ffffff", horizontal_alignment = "center", vertical_alignment = "center" } }
        end)(function() return ("%d%%"):format(math.floor((t.zoom or 1) * 100 + 0.5)) end),
        osd_button("zoom_in", "Zoom in", function() send("zoom_by", 1.25, (t.width or 0) / 2, (t.height or 0) / 2) end),
        osd_button("fit_screen", "Fit to window", function() send("fit") end) },
    }
  end

  -- ------------------------------------------------------- chart inspector --

  local value_at = canvas.value_at

  --- A chart: value lines across, time columns that pan, the series
  --- drawn on screen (made again only as the view moves) with a wash under
  --- it, a brushed span the full height, and a crosshair that finds the
  --- series and reads it out.
  function S.chart_inspector(t, spec)
    local series = spec.series or {}
    local lo, hi = spec.value_from or 0, spec.value_to or 100
    local function H() return t.height or 0 end
    local function sy(v) return H() - (v - lo) / math.max(1e-9, hi - lo) * H() end
    local function sx(x) return (x - t.view_x) * t.zoom_x end
    local function line_d(closed)
      local s = get(series)
      local parts = {}
      for i = 1, #s - 1, 2 do parts[#parts + 1] = ("%s%.1f %.1f"):format(i == 1 and "M" or "L", sx(s[i]), sy(s[i + 1])) end
      if #parts == 0 then return "M0 0" end
      if closed then parts[#parts + 1] = ("L%.1f %.1f L%.1f %.1f Z"):format(sx(s[#s - 1]), H(), sx(s[1]), H()) end
      return table.concat(parts, " ")
    end
    local function px() return sx(t.pointer_x or 0) end
    local function value() return value_at(get(series), t.pointer_x or 0) end
    local function label()
      local v = value()
      if not v then return "" end
      if spec.readout then return spec.readout(t.pointer_x, v) end
      return ("%.1f"):format(v)
    end
    return {
      grid = ui.Item { anchors = { fill = true },
        canvas.grid(t, { kind = "columns", base = 60, lo = 60, hi = 200, stroke_width = 1,
          stroke_color = function() local p = P() return p.ink:alpha(p.strong and 0.3 or 0.06) end }),
        ui.Path { anchors = { fill = true }, fill_color = "transparent", stroke_width = 1,
          d = function()
            local parts = {}
            for i = 1, 3 do parts[#parts + 1] = ("M0 %.1f H%.1f"):format(H() * i / 4, t.width or 0) end
            return table.concat(parts, " ")
          end, stroke_color = function() local p = P() return p.ink:alpha(p.strong and 0.3 or 0.08) end },
        M.text { x = 8, y = function() return H() / 4 - 20 end, height = 18, font_size = theme.size.small,
          text = ("%g%s"):format(lo + (hi - lo) * 0.75, spec.unit and (" " .. spec.unit) or ""), color = function() return P().ink_dim end },
        M.text { x = 8, y = function() return H() / 2 - 20 end, height = 18, font_size = theme.size.small,
          text = ("%g"):format(lo + (hi - lo) * 0.5), color = function() return P().ink_dim end },
        M.text { x = 8, y = function() return H() * 3 / 4 - 20 end, height = 18, font_size = theme.size.small,
          text = ("%g"):format(lo + (hi - lo) * 0.25), color = function() return P().ink_dim end } },
      content = ui.Item { anchors = { fill = true },
        ui.Path { anchors = { fill = true }, d = function() return line_d(true) end,
          fill_color = function() local p = P() return p.accent:mix(p.view, p.area or (p.dark and 0.86 or 0.9)) end },
        ui.Path { anchors = { fill = true }, d = function() return line_d(false) end, fill_color = "transparent",
          stroke_width = 2, stroke_join = "round", stroke_color = function() return P().accent end } },
      band = ui.Rect { y = 0, height = H,
        x = function() return (select(1, canvas.band_box(t))) end,
        width = function() return (select(3, canvas.band_box(t))) end,
        visible = function() return t.band ~= nil and t.band ~= "" and t.gesture ~= "none" end,
        color = function() local p = P() return p.accent:alpha(p.dark and 0.2 or 0.12) end },
      crosshair = ui.Item { anchors = { fill = true }, visible = function() return t.pointer_inside and value() ~= nil end,
        ui.Rect { width = 1, height = H, x = px, color = function() return P().ink:alpha(0.45) end },
        ui.Rect { width = 12, height = 12, radius = math.min(6, R.large), border_width = 2,
          x = function() return px() - 6 end, y = function() return sy(value() or 0) - 6 end,
          color = function() return P().view end, border_color = function() return P().accent end },
        ui.Rect { height = 26, radius = math.min(13, R.large), y = 8, width = function() return 26 + utf8.len(label()) * 8 end,
          x = function()
            local w = 26 + utf8.len(label()) * 8
            local x = px() + 10
            if x + w > (t.width or 0) - 6 then x = px() - 10 - w end
            return x
          end,
          color = function() return P().raised end, border_width = 1, border_color = function() return P().border end,
          M.text { anchors = { fill = true }, text = label, font_size = theme.size.small, font_weight = 700,
            horizontal_alignment = "center", vertical_alignment = "center", color = function() return P().ink end } } },
    }
  end

  -- -------------------------------------------------------------- timeline --

  local span, stamp = canvas.time_span, canvas.stamp

  --- A time ruler `height` tall across a canvas whose x is seconds: its
  --- ticks one drawing that slides with the pan (made again only as the
  --- zoom changes the span), its times a handful of labels moved along.
  --- Returns it and `lines(color)`, which makes the matching column
  --- lines for the body.
  function M.canvas_ruler(t, height, color, props)
    props = props or {}
    local function W() return t.width or 0 end
    local function major() local sec = span(t.zoom_x or 1) return sec * (t.zoom_x or 1) end
    local function offset() local v = (t.view_x or 0) * (t.zoom_x or 1) return -(v % major()) end
    local holder = ui.Item { anchors = { left = true, right = true, top = true }, height = height, clip = true }
    ui.reparent(ui.Path { x = function() return offset() end, y = height - 10,
      width = function() return W() + major() * 2 end, height = 10, fill_color = "transparent", stroke_width = 1,
      d = function()
        local sec, steps = span(t.zoom_x or 1)
        local m = sec * (t.zoom_x or 1)
        local parts = {}
        local n = math.ceil((W() + m * 2) / m) * steps
        for k = 0, n do parts[#parts + 1] = ("M%.1f %d V10"):format(k * m / steps, k % steps == 0 and 0 or 6) end
        return table.concat(parts, " ")
      end, stroke_color = color }, holder)
    for i = 0, 24 do
      ui.reparent(M.text { y = props.label_y or 2, height = 16, font_size = theme.size.small, color = color,
        font_family = props.font, x = function() return offset() + i * major() + 4 end,
        visible = function() return offset() + i * major() < W() end,
        text = function()
          local sec = span(t.zoom_x or 1)
          local first = math.floor((t.view_x or 0) / sec + 1e-9)
          return stamp((first + i) * sec)
        end }, holder)
    end
    local function lines(line_color)
      return ui.Path { x = function() return offset() end, y = 0,
        width = function() return W() + major() * 2 end, height = function() return t.height or 0 end,
        fill_color = "transparent", stroke_width = 1, stroke_color = line_color,
        d = function()
          local m, parts = major(), {}
          for k = 0, math.ceil((W() + m * 2) / m) do parts[#parts + 1] = ("M%.1f 0 V%.1f"):format(k * m, t.height or 0) end
          return table.concat(parts, " ")
        end }
    end
    return holder, lines
  end

  --- A timeline: a ruler along the top whose ticks pan as one drawing and
  --- whose times are a handful of labels moved along, track lanes in
  --- alternate shades with their names, clips as tinted bars with a
  --- stronger edge, the playhead in the accent with a head on the ruler.
  function S.timeline_track(t, spec)
    local RULER = spec.ruler_height or 30
    local TRACK = spec.track_height or 44
    local tracks = spec.tracks or {}
    local function W() return t.width or 0 end
    local ruler, ruler_lines = M.canvas_ruler(t, RULER, function() return P().ink_dim end)
    local lanes = ui.Item { anchors = { fill = true } }
    for i = 1, #tracks do
      ui.reparent(ui.Rect { y = RULER + (i - 1) * TRACK, height = TRACK, anchors = { left = true, right = true },
        color = function() local p = P() return i % 2 == 0 and p.ink:mix(p.view, p.dark and 0.975 or 0.975) or p.view end }, lanes)
    end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().view end },
      grid = ui.Item { anchors = { fill = true }, lanes,
        ruler_lines(function() local p = P() return p.ink:alpha(p.strong and 0.3 or 0.06) end) },
      content = ui.Item { anchors = { left = true, right = true, top = true }, height = RULER,
        ui.Rect { anchors = { fill = true }, color = function() return P().sidebar end },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1, color = function() return P().border end },
        ruler },
      item = function(item, s)
        local function hue() return tone(s.item().tone) or P().accent end
        local clip = ui.Rect { anchors = { fill = true }, radius = R.small, clip = true,
          color = function() local p = P() return M.canvas_fill(s.item().tone or "accent", p.dark and 0.6 or 0.74) end,
          border_width = 1, border_color = function() local p = P() return s.selected() and p.accent or hue():mix(p.view, 0.3) end }
        ui.reparent(ui.Rect { anchors = { left = true, top = true, bottom = true }, width = 4, color = hue }, clip)
        ui.reparent(M.text { anchors = { fill = true, left_margin = 12, right_margin = 6 }, text = item.label or "",
          font_size = theme.size.small, font_weight = 600, vertical_alignment = "center", elide = "right",
          color = function() return M.canvas_on_fill(s.item().tone or "accent") end }, clip)
        return clip
      end,
      selection = function()
        return ui.Rect { anchors = { fill = true, margins = -3 }, color = "transparent", radius = R.small + 3,
          border_width = 2, border_color = function() return P().accent end,
          scale = 1, opacity = 1, enter = { scale = 1.04, opacity = 0 },
          behavior = { scale = M.spring(420, 22), opacity = quick() } }
      end,
      crosshair = ui.Item { anchors = { fill = true }, visible = function() return t.pointer_inside end,
        ui.Rect { width = 2, y = RULER, height = function() return (t.height or 0) - RULER end,
          x = function() return (t.pointer_x - t.view_x) * t.zoom_x - 1 end, color = function() return P().accent end },
        ui.Path { width = 14, height = 12, y = RULER - 12, view_box = { 0, 0, 14, 12 }, d = "M0 0 H14 V5 L7 12 L0 5 Z",
          x = function() return (t.pointer_x - t.view_x) * t.zoom_x - 7 end, fill_color = function() return P().accent end } },
    }
  end

  -- --------------------------------------------------------- drawing board --

  --- A pixel board: the board's page with a line between every cell and a
  --- stronger one every eighth, the ground beyond it in the sidebar's
  --- shade, square pixels in their tone, a square outline round the
  --- chosen.
  function S.drawing_board(t, spec)
    local board = spec.board
    local function box()
      if not board then return 0, 0, t.width or 0, t.height or 0 end
      return (board[1] - t.view_x) * t.zoom_x, (board[2] - t.view_y) * t.zoom_y,
        (board[3] - board[1]) * t.zoom_x, (board[4] - board[2]) * t.zoom_y
    end
    local function ink(a) return function() local p = P() return p.ink:alpha(p.strong and a * 3 or (p.dark and a * 0.55 or a)) end end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().sidebar end },
      grid = ui.Item { x = function() return (box()) end, y = function() local _, y = box() return y end,
        width = function() local _, _, w = box() return w end, height = function() local _, _, _, h = box() return h end,
        clip = true,
        ui.Rect { anchors = { fill = true }, color = function() return P().view end },
        ui.Item { x = function() return -(select(1, box())) end, y = function() return -(select(2, box())) end,
          width = function() return t.width or 0 end, height = function() return t.height or 0 end,
          canvas.grid(t, { lo = 4, hi = 400, stroke_width = 1, stroke_color = ink(0.07) }),
          canvas.grid(t, { every = 8, lo = 4, hi = 400, stroke_width = 1, stroke_color = ink(0.16) }) } },
      item = function(item, s)
        return ui.Rect { anchors = { fill = true },
          color = function() local it = s.item() return (it.fill and get(it.fill)) or tone(it.tone) or P().ink end }
      end,
      selection = function()
        return ui.Rect { anchors = { fill = true, margins = -2 }, color = "transparent", border_width = 2,
          border_color = function() return P().accent end, enter = { opacity = 0 }, opacity = 1,
          behavior = { opacity = quick() } }
      end,
    }
  end
end
