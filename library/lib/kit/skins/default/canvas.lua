-- The default kit's Canvas skin, in the Adwaita manner: a view ground with
-- a hairline grid, cards for items with the accent round what is chosen,
-- wires as soft curves, a blue band. Each widget adds what it is: a node
-- graph of cards with a tinted header and port dots, a whiteboard of paper
-- with sticky notes, a diagram on a dotted grid, a map of soft layers with
-- a scale bar and its attribution, an image on a checkerboard under
-- on-screen zoom buttons, a chart with a crosshair that reads the value, a
-- timeline under a ruler with a playhead, a pixel board. Only what differs
-- is given; the archetype's skin draws the rest.
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get, quick = draw.get, draw.quick
  local canvas = require("lib.kit.archetypes.canvas")

  -- A tone an item names (accent, success, warning, error, info, extra) as
  -- a fill; "paper" the light of a page, "ink" the text's; nil for none.
  local function tone(name)
    if not name then return nil end
    local p = P()
    if name == "paper" then return p.accent_fg end
    if name == "ink" then return p.window_fg end
    return p[name .. "_bg"] or p.accent_bg
  end
  draw.canvas_tone = tone
  --- A tone's soft fill (a note, a clip, a node's header): the tone washed
  --- into `ground` (the view); the ink on it.
  function draw.canvas_fill(name, amount, ground)
    local p = P()
    local c = tone(name)
    return c and c:mix(ground or p.view_bg, amount or (p.dark and 0.72 or 0.84)) or p.card_bg
  end
  function draw.canvas_on_fill() return P().window_fg end

  local extent, centre, arrow = canvas.extent, canvas.centre, canvas.arrow_path


  --- An item drawn from its fields: a card for a box (`label`, `tone` or
  --- `fill`/`stroke`), the shape itself for a line (`width` px, `arrow`), a
  --- polygon (`label` at its middle) or a point (`r` px). `look` changes
  --- it: `fill(item)`, `stroke(item, s)`, `radius`, `ink(item)`, `border`
  --- (px), `label_size`.
  function draw.canvas_item(item, s, look)
    look = look or {}
    local shape = item.shape or "rect"
    local function zoom() return math.max(0.0001, s.zoom()) end
    local function fill()
      local it = s.item()
      if look.fill then return look.fill(it) end
      if it.fill then return get(it.fill) end
      return draw.canvas_fill(it.tone)
    end
    local function stroke()
      local p = P()
      local it = s.item()
      if look.stroke then return look.stroke(it, s) end
      if s.selected() then return p.accent_bg end
      if s.hovered() then return p.window_fg:alpha(0.4) end
      if it.stroke then return get(it.stroke) end
      local c = tone(it.tone)
      return c and c:mix(p.view_bg, 0.35) or p.border
    end
    local function ink() return look.ink and look.ink(s.item()) or draw.canvas_on_fill(s.item().tone) end
    if shape == "line" or shape == "polygon" then
      local holder = ui.Item {}
      local function line_color()
        local it = s.item()
        if look.stroke then return look.stroke(it, s) end
        if s.selected() then return P().accent_bg end
        return (it.stroke and get(it.stroke)) or tone(it.tone) or P().dim_fg
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
        ui.reparent(draw.text {
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
        color = function() local it = s.item() return (it.fill and get(it.fill)) or tone(it.tone) or P().accent_bg end,
        border_width = function() return 2 / zoom() end, border_color = function() return P().view_bg end }
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
      ui.reparent(draw.text { anchors = { fill = true, margins = 6 }, text = function() return s.item().label or "" end,
        color = ink, font_size = look.label_size or theme.size.normal, elide = "right",
        horizontal_alignment = "center", vertical_alignment = "center" }, holder)
    end
    return holder
  end

  --- A wire between two world points as a curve, `width` px (2), its
  --- colour `color`; its box reaches to the further point so nothing is
  --- cut.
  function draw.canvas_wire(points, s, color, width)
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
      return function() local p = P() return p.window_fg:alpha(p.strong and a * 3.5 or (p.dark and a * 0.5 or a)) end
    end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().view_bg end },
      grid = show_grid and ui.Item { anchors = { fill = true },
        canvas.grid(t, { stroke_width = 1, stroke_color = grid_ink(0.05) }),
        canvas.grid(t, { every = 4, stroke_width = 1, stroke_color = grid_ink(0.06) }) } or nil,
      item = function(item, s) return draw.canvas_item(item, s) end,
      wires = function(points, s)
        return draw.canvas_wire(points, s, function()
          return (s.wire().color and get(s.wire().color)) or P().dim_fg
        end)
      end,
      -- A handle of the one chosen box that resizes: a white knob with
      -- the accent's ring, swelling under the pointer.
      grip = function(s)
        return ui.Rect { x = -5, y = -5, width = 10, height = 10, radius = 5,
          color = function() return P().knob end, border_width = 2,
          border_color = function() return P().accent_bg end,
          shadow_color = "#00000033", shadow_blur = 3, shadow_offset_y = 1,
          scale = function() return (s.hovered() or s.held()) and 1.35 or 1 end,
          behavior = { scale = draw.spring(520, 24) } }
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
          border_width = 2, border_color = function() return P().accent_bg end,
          scale = 1, opacity = 1, enter = { scale = 1.06, opacity = 0 },
          behavior = { scale = draw.spring(420, 22), opacity = quick() } }
      end,
      draft = ui.Item { anchors = { fill = true },
        ui.Path { anchors = { fill = true }, d = function() return canvas.draft_path(t) end,
          fill_color = function() local p = P() return t.gesture == "draw" and p.accent_bg:alpha(0.08) or p.accent_bg:alpha(0) end,
          stroke_width = 2, stroke_join = "round", stroke_cap = "round", stroke_color = function() return P().accent_bg end,
          visible = function() return t.gesture == "draw" or t.gesture == "draft" end },
        ui.Path { anchors = { fill = true }, d = function() return canvas.pull_path(t, spec) end,
          fill_color = "transparent", stroke_width = 2, stroke_cap = "round", dash = { 6, 4 },
          stroke_color = function() local p = P() return t.connect_to ~= "" and p.accent_bg or p.dim_fg end,
          visible = function() return t.gesture == "connect" end } },
      band = ui.Rect { x = function() return (select(1, canvas.band_box(t))) end,
        y = function() return (select(2, canvas.band_box(t))) end,
        width = function() return (select(3, canvas.band_box(t))) end,
        height = function() return (select(4, canvas.band_box(t))) end,
        visible = function() return t.band ~= nil and t.band ~= "" and t.gesture ~= "none" end,
        color = function() local p = P() return p.accent_bg:alpha(p.dark and 0.18 or 0.12) end,
        border_width = 1, border_color = function() return P().accent_bg end, radius = math.min(2, R.small) },
      crosshair = crosshair and ui.Item { anchors = { fill = true }, visible = function() return t.pointer_inside end,
        ui.Rect { width = 1, height = H, color = function() return P().window_fg:alpha(0.4) end,
          x = function() return (t.pointer_x - t.view_x) * t.zoom_x end },
        ui.Rect { height = 1, width = W, color = function() return P().window_fg:alpha(0.4) end,
          y = function() return (t.pointer_y - t.view_y) * t.zoom_y end,
          visible = function() return spec.axes ~= "x" end } } or nil,
      overlay = readout and ui.Rect { anchors = { right = true, bottom = true, margins = 10 }, width = 64, height = 28,
        radius = math.min(14, R.large), color = function() return P().popover_bg end, border_width = 1,
        border_color = function() return P().border end,
        draw.text { anchors = { fill = true }, text = function() return ("%d%%"):format(math.floor((t.zoom or 1) * 100 + 0.5)) end,
          color = function() return P().dim_fg end, font_size = theme.size.small,
          horizontal_alignment = "center", vertical_alignment = "center" } } or nil,
    }
  end

  local R = theme.radius
  local canvas = require("lib.kit.archetypes.canvas")
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local tone = draw.canvas_tone

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
    ui.reparent(draw.icon(icon, 20, "#ffffff", { anchors = { center_in = true } }), area)
    return area
  end

  -- (A theme restyling these looks may give its own.)
  local osd_button = draw.canvas_osd or loupe_button

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
        local function hue() local p = P() return tone(s.item().tone) or p.accent_bg end
        local card = ui.Rect { anchors = { fill = true }, radius = R.large, clip = false,
          color = function() return P().card_bg end,
          border_width = function() return 1 / zoom() end,
          border_color = function()
            local p = P()
            if s.selected() then return p.accent_bg end
            return s.hovered() and p.window_fg:alpha(0.3) or p.border
          end, behavior = { border_color = quick() } }
        -- (A shadow under the card: a soft darker plate a touch lower.)
        ui.reparent(ui.Rect { anchors = { fill = true, top_margin = 2, bottom_margin = -3 }, z = -1, radius = R.large,
          color = function() local p = P() return p.shade:alpha(p.dark and 0.5 or 0.35) end }, card)
        ui.reparent(ui.Rect { anchors = { left = true, right = true, top = true, margins = 1 }, height = HEADER - 1,
          radius = R.large - 1, color = function() local p = P() return draw.canvas_fill(s.item().tone or "accent", p.dark and 0.6 or 0.78, p.card_bg) end }, card)
        ui.reparent(ui.Rect { anchors = { left = true, right = true, top = true, top_margin = HEADER - 1 }, height = 1,
          color = function() return P().border end }, card)
        ui.reparent(ui.Rect { x = 12, y = HEADER / 2 - 4, width = 8, height = 8, radius = math.min(4, R.small), color = hue }, card)
        ui.reparent(draw.text { x = 28, y = 0, height = HEADER, text = function() return s.item().title or s.item().label or "" end,
          font_size = theme.size.small, font_weight = 700, vertical_alignment = "center", elide = "right",
          width = function() return math.max(10, (s.item().w or 100) - 36) end,
          color = function() return draw.canvas_on_fill(s.item().tone or "accent") end }, card)
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
            ui.reparent(draw.text { x = function() return is_in and 14 or 0 end, y = function() return ly() - ROW / 2 end,
              width = function() return (s.item().w or 100) - 14 end, height = ROW, text = port.label or "",
              horizontal_alignment = is_in and "left" or "right", font_size = theme.size.small,
              vertical_alignment = "center", elide = "right", color = function() return P().dim_fg end }, built)
            local function r() return hot() and 7 or 5 end
            ui.reparent(ui.Rect { x = function() return lx() - r() end, y = function() return ly() - r() end,
              width = function() return r() * 2 end, height = function() return r() * 2 end,
              radius = function() return draw.canvas_port_round and draw.canvas_port_round(hot(), r()) or r() end,
              color = function() local p = P() return hot() and p.accent_bg or (is_in and p.card_bg or hue()) end,
              border_width = 2, border_color = function() local p = P() return hot() and p.accent_bg or hue() end,
              behavior = { x = draw.spring(520, 30), y = draw.spring(520, 30), width = draw.spring(520, 30), height = draw.spring(520, 30),
                radius = draw.spring(520, 30) } },
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
          for _, n in ipairs(canvas.list(spec.items)) do if n.id == id then return tone(n.tone) or p.dim_fg end end
          return p.dim_fg
        end
        local control = require("lib.kit.control")
        local function lit()
          local w = s.wire()
          local b = spec.port and (spec.port(w.to) or {}).item
          return control.has(t.selection, from_item()) or control.has(t.selection, b)
        end
        return draw.canvas_wire(points, s, function()
          local p = P()
          return lit() and hue() or hue():mix(p.view_bg, 0.35)
        end, function() return lit() and 3 or 2 end)
      end,
      selection = function(s)
        return ui.Rect { anchors = { fill = true, margins = -4 }, color = "transparent", radius = R.large + 4,
          border_width = 2, border_color = function() return P().accent_bg end,
          scale = 1, opacity = 1, enter = { scale = 1.05, opacity = 0 },
          behavior = { scale = draw.spring(420, 22), opacity = quick() } }
      end,
    }
  end

  -- ------------------------------------------------------------ whiteboard --

  --- Paper with a faint dot grid; sticky notes in their tone with a soft
  --- shadow; strokes in their tone, as drawn; a frame as a dashed outline.
  function S.whiteboard(t, spec)
    return {
      background = ui.Rect { anchors = { fill = true },
        color = function() local p = P() return p.dark and p.view_bg or p.accent_fg:mix(p.warning_bg, 0.03) end },
      grid = canvas.grid(t, { kind = "dots", base = 24, lo = 18, hi = 48, stroke_width = 2,
        stroke_color = function() local p = P() return p.window_fg:alpha(p.strong and 0.4 or (p.dark and 0.08 or 0.14)) end }),
      item = function(item, s)
        if item.kind == "note" then
          local function zoom() return math.max(0.0001, s.zoom()) end
          local note = ui.Rect { anchors = { fill = true }, radius = function() return 4 / math.max(1, zoom()) end,
            color = function() local p = P() return draw.canvas_fill(s.item().tone or "warning", p.dark and 0.45 or 0.68, p.accent_fg) end }
          ui.reparent(ui.Rect { anchors = { fill = true, top_margin = 3, bottom_margin = -4, left_margin = 2, right_margin = -2 },
            z = -1, radius = math.min(4, R.small), color = function() local p = P() return p.shade:alpha(p.dark and 0.6 or 0.45) end }, note)
          ui.reparent(draw.text { anchors = { fill = true, margins = 12 }, text = function() return s.item().label or "" end,
            font_size = theme.size.normal, font_weight = 600, wrap = true, vertical_alignment = "top",
            color = function()
              local p = P()
              return p.dark and p.shade or p.window_fg
            end }, note)
          return note
        elseif item.kind == "frame" then
          local frame = ui.Item { anchors = { fill = true } }
          ui.reparent(ui.Path { anchors = { fill = true },
            d = function() local it = s.item() return ("M0 0 H%g V%g H0 Z"):format(it.w or 0, it.h or 0) end,
            fill_color = "transparent", dash = { 8, 6 },
            stroke_width = function() return 1.5 / math.max(0.0001, s.zoom()) end,
            stroke_color = function() local p = P() return s.selected() and p.accent_bg or p.window_fg:alpha(0.45) end }, frame)
          ui.reparent(draw.text { x = 0, y = -26, height = 22, width = function() return s.item().w or 100 end,
            text = function() return s.item().label or "" end, font_size = theme.size.small, font_weight = 700,
            color = function() return P().dim_fg end }, frame)
          return frame
        end
        return draw.canvas_item(item, s)
      end,
    }
  end

  -- -------------------------------------------------------------- diagram --

  --- Shapes on a dotted grid: process boxes as white cards, decisions and
  --- terminals in their tone, connectors as dim lines with arrowheads.
  function S.diagram(t, spec)
    return {
      grid = canvas.grid(t, { kind = "dots", stroke_width = 2, lo = 14, hi = 40,
        stroke_color = function() local p = P() return p.window_fg:alpha(p.strong and 0.45 or (p.dark and 0.1 or 0.2)) end }),
      item = function(item, s)
        return draw.canvas_item(item, s, { radius = R.small, border = 1.5, label_size = theme.size.normal })
      end,
    }
  end

  -- ------------------------------------------------------------------ map --

  -- Each layer's colours, from the palette: land is the window's ground
  -- warmed a touch, water the info tone washed out, parks the success's.
  local LAYER = {
    water = function(p) return p.info_bg:mix(p.view_bg, p.dark and 0.55 or 0.62) end,
    park = function(p) return p.success_bg:mix(p.view_bg, p.dark and 0.62 or 0.72) end,
    building = function(p) return p.window_fg:mix(p.window_bg, p.dark and 0.86 or 0.9) end,
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
        color = function() local p = P() return p.window_bg:mix(p.warning_bg, p.dark and 0.02 or 0.05) end },
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
              stroke_color = function() local p = P() return p.window_fg:alpha(p.dark and 0.3 or 0.16) end },
            ui.Path { width = w + 1, height = h + 1, d = d, fill_color = "transparent", stroke_join = "round",
              stroke_cap = "round", stroke_width = function() return (s.item().width or 4) / z() end,
              stroke_color = function() local p = P() return p.dark and p.card_bg or p.view_bg end } }
        elseif layer == "route" then
          return draw.canvas_item(item, s, { stroke = function(_, st)
            local p = P()
            return st.selected() and p.accent or p.accent_bg
          end })
        elseif layer == "place" then
          local pin = draw.canvas_item(item, s)
          if item.label then
            local holder = ui.Item { width = 1, height = 1 }
            ui.reparent(pin, holder)
            ui.reparent(draw.text { x = function() return s.item().x + 12 / math.max(0.0001, s.zoom()) end,
              y = function() return s.item().y - 10 / math.max(0.0001, s.zoom()) end,
              scale = function() return 1 / math.max(0.0001, s.zoom()) end, transform_origin_x = 0, transform_origin_y = 0,
              text = item.label, font_size = theme.size.small, font_weight = 700, height = 20,
              color = function() return P().window_fg end }, holder)
            return holder
          end
          return pin
        elseif LAYER[layer] then
          return draw.canvas_item(item, s, { fill = function() return LAYER[layer](P()) end,
            stroke = function() local p = P() return LAYER[layer](p):mix(p.window_fg, 0.12) end, border = layer == "building" and 1 or 0 })
        end
        return draw.canvas_item(item, s)
      end,
      overlay = ui.Item { anchors = { fill = true },
        -- Zoom in and out, about the middle.
        ui.Column { anchors = { right = true, top = true, margins = 12 }, gap = 6,
          osd_button("add", "Zoom in", function() send("zoom_by", 1.5, (t.width or 0) / 2, (t.height or 0) / 2) end),
          osd_button("remove", "Zoom out", function() send("zoom_by", 1 / 1.5, (t.width or 0) / 2, (t.height or 0) / 2) end) },
        -- The scale bar.
        ui.Item { anchors = { left = true, bottom = true, margins = 12 }, width = 150, height = 30,
          ui.Rect { x = 0, y = 22, height = 4, radius = math.min(2, R.small), width = function() local _, px = scale() return px end,
            color = function() return P().window_fg end, behavior = { width = quick() } },
          draw.text { x = 0, y = 0, height = 20, text = function()
              local m = scale()
              return m >= 1000 and ("%g km"):format(m / 1000) or ("%d m"):format(m)
            end, font_size = theme.size.small, font_weight = 700, color = function() return P().window_fg end } },
        -- Where the data comes from.
        spec.attribution and ui.Rect { anchors = { right = true, bottom = true, margins = 8 }, height = 24, width = 136,
          radius = math.min(12, R.large), color = function() return P().view_bg:alpha(0.85) end,
          draw.text { anchors = { fill = true }, text = spec.attribution, font_size = theme.size.small,
            horizontal_alignment = "center", vertical_alignment = "center", color = function() return P().dim_fg end } }
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
      background = ui.Rect { anchors = { fill = true }, color = function() local p = P() return p.window_bg end },
      grid = ui.Item {
        x = function() local x = board_box() return x end, y = function() local _, y = board_box() return y end,
        width = function() local _, _, w = board_box() return math.max(0, w) end,
        height = function() local _, _, _, h = board_box() return math.max(0, h) end, clip = true,
        ui.Rect { anchors = { fill = true }, color = function() return P().view_bg end },
        -- (Squares kept to the image's corner; the drawing covers the
        -- screen and moves only by whole pairs of squares.)
        ui.Path { x = function() local x = board_box() return (-x) - ((-x) % (CHECK * 2)) end,
          y = function() local _, y = board_box() return (-y) - ((-y) % (CHECK * 2)) end,
          width = function() return (t.width or 0) + CHECK * 4 end, height = function() return (t.height or 0) + CHECK * 4 end,
          d = checker_d, fill_color = function() local p = P() return p.window_fg:alpha(0.08) end } },
      item = function(item, s)
        local box = ui.Rect { anchors = { fill = true }, color = "transparent",
          radius = function() return (item.shape == "ellipse") and math.min(s.item().w or 0, s.item().h or 0) / 2 or 0 end,
          border_width = function() return 2 / math.max(0.0001, s.zoom()) end,
          border_color = function() local p = P() return (s.selected() or s.hovered()) and p.accent_bg or (p.paper or p.accent_fg):alpha(0.9) end }
        if item.label then
          ui.reparent(ui.Rect { x = 0, y = function() return -30 / math.max(0.0001, s.zoom()) end,
            width = 72, height = 24, radius = math.min(12, R.large), scale = function() return 1 / math.max(0.0001, s.zoom()) end,
            transform_origin_x = 0, transform_origin_y = 0, color = function() return P().accent_bg end,
            draw.text { anchors = { fill = true }, text = item.label, font_size = theme.size.small, font_weight = 700,
              horizontal_alignment = "center", vertical_alignment = "center", color = function() return P().accent_fg end } }, box)
        end
        return box
      end,
      selection = function() return ui.Item {} end,
      overlay = ui.Row { anchors = { horizontal_center = true, bottom = true, bottom_margin = 14 }, gap = 6, align = "center",
        osd_button("zoom_out", "Zoom out", function() send("zoom_by", 1 / 1.25, (t.width or 0) / 2, (t.height or 0) / 2) end),
        (draw.canvas_osd_label or function(text)
          return ui.Rect { width = 72, height = 36, radius = 18, color = "#00000099",
            draw.text { anchors = { fill = true }, text = text, font_size = theme.size.small, font_weight = 700,
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
          stroke_color = function() local p = P() return p.window_fg:alpha(p.strong and 0.3 or 0.06) end }),
        ui.Path { anchors = { fill = true }, fill_color = "transparent", stroke_width = 1,
          d = function()
            local parts = {}
            for i = 1, 3 do parts[#parts + 1] = ("M0 %.1f H%.1f"):format(H() * i / 4, t.width or 0) end
            return table.concat(parts, " ")
          end, stroke_color = function() local p = P() return p.window_fg:alpha(p.strong and 0.3 or 0.08) end },
        draw.text { x = 8, y = function() return H() / 4 - 20 end, height = 18, font_size = theme.size.small,
          text = ("%g%s"):format(lo + (hi - lo) * 0.75, spec.unit and (" " .. spec.unit) or ""), color = function() return P().dim_fg end },
        draw.text { x = 8, y = function() return H() / 2 - 20 end, height = 18, font_size = theme.size.small,
          text = ("%g"):format(lo + (hi - lo) * 0.5), color = function() return P().dim_fg end },
        draw.text { x = 8, y = function() return H() * 3 / 4 - 20 end, height = 18, font_size = theme.size.small,
          text = ("%g"):format(lo + (hi - lo) * 0.25), color = function() return P().dim_fg end } },
      content = ui.Item { anchors = { fill = true },
        ui.Path { anchors = { fill = true }, d = function() return line_d(true) end,
          fill_color = function() local p = P() return p.accent_bg:mix(p.view_bg, p.area or (p.dark and 0.86 or 0.9)) end },
        ui.Path { anchors = { fill = true }, d = function() return line_d(false) end, fill_color = "transparent",
          stroke_width = 2, stroke_join = "round", stroke_color = function() return P().accent_bg end } },
      band = ui.Rect { y = 0, height = H,
        x = function() return (select(1, canvas.band_box(t))) end,
        width = function() return (select(3, canvas.band_box(t))) end,
        visible = function() return t.band ~= nil and t.band ~= "" and t.gesture ~= "none" end,
        color = function() local p = P() return p.accent_bg:alpha(p.dark and 0.2 or 0.12) end },
      crosshair = ui.Item { anchors = { fill = true }, visible = function() return t.pointer_inside and value() ~= nil end,
        ui.Rect { width = 1, height = H, x = px, color = function() return P().window_fg:alpha(0.45) end },
        ui.Rect { width = 12, height = 12, radius = math.min(6, R.large), border_width = 2,
          x = function() return px() - 6 end, y = function() return sy(value() or 0) - 6 end,
          color = function() return P().view_bg end, border_color = function() return P().accent_bg end },
        ui.Rect { height = 26, radius = math.min(13, R.large), y = 8, width = function() return 26 + utf8.len(label()) * 8 end,
          x = function()
            local w = 26 + utf8.len(label()) * 8
            local x = px() + 10
            if x + w > (t.width or 0) - 6 then x = px() - 10 - w end
            return x
          end,
          color = function() return P().popover_bg end, border_width = 1, border_color = function() return P().border end,
          draw.text { anchors = { fill = true }, text = label, font_size = theme.size.small, font_weight = 700,
            horizontal_alignment = "center", vertical_alignment = "center", color = function() return P().window_fg end } } },
    }
  end

  -- -------------------------------------------------------------- timeline --

  local span, stamp = canvas.time_span, canvas.stamp

  --- A time ruler `height` tall across a canvas whose x is seconds: its
  --- ticks one drawing that slides with the pan (made again only as the
  --- zoom changes the span), its times a handful of labels moved along.
  --- Returns it and `lines(color)`, which makes the matching column
  --- lines for the body.
  function draw.canvas_ruler(t, height, color, props)
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
      ui.reparent(draw.text { y = props.label_y or 2, height = 16, font_size = theme.size.small, color = color,
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
    local ruler, ruler_lines = draw.canvas_ruler(t, RULER, function() return P().dim_fg end)
    local lanes = ui.Item { anchors = { fill = true } }
    for i = 1, #tracks do
      ui.reparent(ui.Rect { y = RULER + (i - 1) * TRACK, height = TRACK, anchors = { left = true, right = true },
        color = function() local p = P() return i % 2 == 0 and p.window_fg:mix(p.view_bg, p.dark and 0.975 or 0.975) or p.view_bg end }, lanes)
    end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().view_bg end },
      grid = ui.Item { anchors = { fill = true }, lanes,
        ruler_lines(function() local p = P() return p.window_fg:alpha(p.strong and 0.3 or 0.06) end) },
      content = ui.Item { anchors = { left = true, right = true, top = true }, height = RULER,
        ui.Rect { anchors = { fill = true }, color = function() return P().sidebar_bg end },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1, color = function() return P().border end },
        ruler },
      item = function(item, s)
        local function hue() return tone(s.item().tone) or P().accent_bg end
        local clip = ui.Rect { anchors = { fill = true }, radius = R.small, clip = true,
          color = function() local p = P() return draw.canvas_fill(s.item().tone or "accent", p.dark and 0.6 or 0.74) end,
          border_width = 1, border_color = function() local p = P() return s.selected() and p.accent_bg or hue():mix(p.view_bg, 0.3) end }
        ui.reparent(ui.Rect { anchors = { left = true, top = true, bottom = true }, width = 4, color = hue }, clip)
        ui.reparent(draw.text { anchors = { fill = true, left_margin = 12, right_margin = 6 }, text = item.label or "",
          font_size = theme.size.small, font_weight = 600, vertical_alignment = "center", elide = "right",
          color = function() return draw.canvas_on_fill(s.item().tone or "accent") end }, clip)
        return clip
      end,
      selection = function()
        return ui.Rect { anchors = { fill = true, margins = -3 }, color = "transparent", radius = R.small + 3,
          border_width = 2, border_color = function() return P().accent_bg end,
          scale = 1, opacity = 1, enter = { scale = 1.04, opacity = 0 },
          behavior = { scale = draw.spring(420, 22), opacity = quick() } }
      end,
      crosshair = ui.Item { anchors = { fill = true }, visible = function() return t.pointer_inside end,
        ui.Rect { width = 2, y = RULER, height = function() return (t.height or 0) - RULER end,
          x = function() return (t.pointer_x - t.view_x) * t.zoom_x - 1 end, color = function() return P().accent_bg end },
        ui.Path { width = 14, height = 12, y = RULER - 12, view_box = { 0, 0, 14, 12 }, d = "M0 0 H14 V5 L7 12 L0 5 Z",
          x = function() return (t.pointer_x - t.view_x) * t.zoom_x - 7 end, fill_color = function() return P().accent_bg end } },
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
    local function ink(a) return function() local p = P() return p.window_fg:alpha(p.strong and a * 3 or (p.dark and a * 0.55 or a)) end end
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().sidebar_bg end },
      grid = ui.Item { x = function() return (box()) end, y = function() local _, y = box() return y end,
        width = function() local _, _, w = box() return w end, height = function() local _, _, _, h = box() return h end,
        clip = true,
        ui.Rect { anchors = { fill = true }, color = function() return P().view_bg end },
        ui.Item { x = function() return -(select(1, box())) end, y = function() return -(select(2, box())) end,
          width = function() return t.width or 0 end, height = function() return t.height or 0 end,
          canvas.grid(t, { lo = 4, hi = 400, stroke_width = 1, stroke_color = ink(0.07) }),
          canvas.grid(t, { every = 8, lo = 4, hi = 400, stroke_width = 1, stroke_color = ink(0.16) }) } },
      item = function(item, s)
        return ui.Rect { anchors = { fill = true },
          color = function() local it = s.item() return (it.fill and get(it.fill)) or tone(it.tone) or P().window_fg end }
      end,
      selection = function()
        return ui.Rect { anchors = { fill = true, margins = -2 }, color = "transparent", border_width = 2,
          border_color = function() return P().accent_bg end, enter = { opacity = 0 }, opacity = 1,
          behavior = { opacity = quick() } }
      end,
    }
  end
end
