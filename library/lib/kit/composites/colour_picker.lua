-- A colour picker (composite: Plane + Range hue + Range alpha + TextField
-- hex + Selection swatches; in a Popup when asked).
--
--     local node, picker = composites.colour_picker {
--       id = "tint", width = 320, height = 300,
--       value = function() return tint:get() end,     -- any colour notation
--       on_changed = function(hex) tint:set(hex) end, -- "#rrggbb" ("#rrggbbaa" with alpha)
--       alpha = true, swatches = { "#e53935", ... },  -- or values / bindings
--     }
--
-- A kit `colour_plane` holds saturation across and value up, under a hue
-- slider and an alpha slider (kit Ranges: dragged, or stepped by the
-- arrows once focused); a hex field (a kit entry: Return takes what is
-- typed, Escape gives it back) beside a sample of the colour; and a
-- `swatch_grid` (a kit Selection: the arrows walk it, a press or Return
-- takes a swatch). Without `swatches` it offers twelve hues and greys.
-- `trailing` (a node) goes at the end of the hex row; `popup = true`
-- makes it a press showing the colour that opens it in a popover. Ids:
-- `<id>-plane`, `<id>-hue`, `<id>-alpha`, `<id>-hex`, `<id>-swatches`,
-- `<id>-swatch-<i>`, `<id>-field`, `<id>-popup`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local function get(v) if type(v) == "function" then return v() end return v end

-- Hue (degrees), saturation, value and alpha of a colour.
local function hsva(c)
  local r, g, b = c.r, c.g, c.b
  -- (Channels read 0..1; 0..255 is taken too.)
  if r > 1 or g > 1 or b > 1 then r, g, b = r / 255, g / 255, b / 255 end
  local high, low = math.max(r, g, b), math.min(r, g, b)
  return c.h or 0, high > 0 and (high - low) / high or 0, high, c.a or 1
end
local function read(v)
  if v == nil or v == "" then return nil end
  local ok, c = pcall(morf.color, v)
  if ok and c then return c end
  return nil
end
local function default_swatches()
  local out = {}
  for i = 0, 11 do out[#out + 1] = morf.color.hsv(i * 30, 0.75, 0.9) end
  for _, v in ipairs { 1, 0.75, 0.5, 0.25, 0 } do out[#out + 1] = morf.color.hsv(0, 0, v) end
  return out
end

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local with_alpha = spec.alpha == true
  local W, H = spec.width or 320, spec.height or 300
  local start = read(get(spec.value)) or morf.color.hsv(210, 0.6, 0.85)
  local h0, s0, v0, a0 = hsva(start)
  local st = morf.state { h = h0, s = s0, v = v0, a = with_alpha and a0 or 1 }
  local function colour()
    local c = morf.color.hsv(st.h, st.s, st.v)
    return st.a < 1 and c:alpha(st.a) or c
  end
  local function hex()
    local c = morf.color.hsv(st.h, st.s, st.v)
    local out = c:hex():sub(1, 7)
    if st.a < 1 then out = out .. ("%02x"):format(math.floor(st.a * 255 + 0.5)) end
    return out
  end
  -- Each change, and the one the configuration hears.
  local function changed() if spec.on_changed then spec.on_changed(hex()) end end
  local function take(c, keep_hue)
    local h, s, v, a = hsva(c)
    -- A grey has no hue of its own: the one there stays.
    if not keep_hue and s > 0 then st.h = h end
    st.s, st.v = s, v
    if with_alpha then st.a = a end
  end
  local swatches = spec.swatches or default_swatches()
  local function swatch(i) return read(get(swatches[i])) end

  local panel
  local input
  local function build()
    if panel then return panel end
    local GAP = 8
    local BAR = 20
    local ROW = 36
    local count = #swatches
    local sw = spec.swatch_size or 22
    local columns = math.max(1, math.min(count, math.floor((W + 4) / (sw + 4))))
    local rows = count > 0 and math.ceil(count / columns) or 0
    local swatch_h = rows > 0 and rows * sw + (rows - 1) * 4 or 0
    local plane_h = H - (BAR + 8 + GAP) * (with_alpha and 2 or 1) - ROW - GAP - (rows > 0 and swatch_h + GAP or 0)
    plane_h = math.max(60, plane_h)
    local column = { gap = GAP, width = W }
    column[#column + 1] = widgets.colour_plane { id = id and (id .. "-plane"), accessible_name = "Saturation and value",
      width = W, height = plane_h, hue = function() return st.h end, y_from = 1, y_to = 0,
      x = function() return st.s end, y = function() return st.v end,
      on_moved = function(x, y) st.s, st.v = x, y changed() end }
    column[#column + 1] = widgets.slider { id = id and (id .. "-hue"), accessible_name = "Hue", width = W,
      height = BAR + 8, bar_height = BAR, label = false, value = function() return st.h / 360 end,
      on_moved = function(v) st.h = math.max(0, math.min(360, v * 360)) changed() end }
    if with_alpha then
      column[#column + 1] = widgets.slider { id = id and (id .. "-alpha"), accessible_name = "Opacity", width = W,
        height = BAR + 8, bar_height = BAR, label = false, value = function() return st.a end,
        on_moved = function(v) st.a = v changed() end }
    end
    -- The sample, the hex and the configuration's own control.
    local trailing = spec.trailing
    local tw = trailing and ((trailing.width and tonumber(get(trailing.width))) or 100) + GAP or 0
    local field_w = W - ROW - GAP - tw
    -- The theme's face for the field: what its text would be set in.
    local probe = { text = "" }
    ui.destroy(kit.text(probe), true)
    local field_node
    local focused = morf.state { on = false, bad = false }
    field_node, input = widgets.entry { id = id and (id .. "-hex"), accessible_name = "Hex colour",
      width = field_w - 20, height = ROW, x = 10, inset = { 0, 0, 0, 0 },
      text = hex(), placeholder = "#rrggbb",
      font_family = probe.font_family, font_source = probe.font_source, font_size = probe.font_size,
      color = kit.ink("hi"), placeholder_color = kit.ink("lo"),
      caret_color = kit.signal("accent"), selection_color = function() return kit.signal("accent")():alpha(0.3) end,
      vertical_alignment = "center",
      on_focus_changed = function(on) focused.on = on end,
      on_text_changed = function(text) focused.bad = text ~= "" and read(text) == nil end,
      on_accepted = function(text)
        local c = read(text)
        if not c then return end
        take(c)
        input.text = hex()
        changed()
      end,
      on_escape = function() input.text = hex() end }
    local row = ui.Item { width = W, height = ROW,
      kit.surface { width = ROW, height = ROW, radius = kit.round(ROW / 4), color = colour,
        border_width = 1, border_color = kit.stroke("quiet") },
      kit.field { x = ROW + GAP, width = field_w, height = ROW, focused = function() return focused.on end,
        error = function() return focused.bad end, field_node } }
    if trailing then
      trailing.anchors = { right = true, vertical_center = true }
      ui.reparent(trailing, row)
    end
    column[#column + 1] = row
    if count > 0 then
      column[#column + 1] = widgets.swatch_grid { id = id and (id .. "-swatches"), accessible_name = "Swatches",
        items = swatches, columns = columns, gap = 4, item_width = sw, item_height = sw, press_activates = true,
        current = 0,
        item_id = function(i) return id and (id .. "-swatch-" .. i) or nil end,
        delegate = function(i, _, s)
          return kit.surface { anchors = { fill = true, margins = 2 },
            radius = function() return kit.round(s.hovered() and sw / 6 or sw / 3) end,
            color = function() return swatch(i) or "transparent" end,
            border_width = 1, border_color = kit.stroke("faint"),
            behavior = { radius = kit.spring(420, 24) } }
        end,
        on_current_changed = function(i) local c = swatch(i) if c then take(c) changed() end end,
        on_activated = function(i)
          local c = swatch(i)
          if c then take(c) changed() end
          if spec.on_swatch then spec.on_swatch(i, c and c:hex()) end
        end }
    end
    panel = ui.Column(column)
    -- The field shows the colour, unless it is being typed in.
    morf.effect("kit.colour_picker.hex." .. tostring(panel), function()
      local now = hex()
      if not focused.on and input.text ~= now then input.text = now end
    end, { owner = panel })
    return panel
  end
  local handle = {}
  function handle.value() return hex() end
  function handle.colour() return colour() end
  function handle.set(v) local c = read(v) if c then take(c) changed() end end
  function handle.hsva() return st.h, st.s, st.v, st.a end
  local root
  if spec.popup then
    local popup
    local function toggle()
      if not popup then
        build()
        -- (Over the popover's ground, which its skin lays after its content.)
        panel.z = 1
        popup = widgets.popover { id = id and (id .. "-popup"), content = panel, padding = 12,
          placement = spec.placement or "bottom-start", root = spec.root, on_closed = spec.on_closed }
        handle.popup = popup
      end
      popup.toggle(root)
    end
    root = widgets.push { id = id and (id .. "-field"), accessible_name = spec.accessible_name or "Colour",
      label = hex, icon = spec.icon or "palette", width = spec.field_width or 160, height = spec.field_height or 36,
      x = spec.x, y = spec.y, anchors = spec.anchors, on_clicked = toggle }
    function handle.open() if not (popup and popup.is_open()) then toggle() end end
    function handle.close() if popup then popup.close("closed") end end
    function handle.is_open() return popup ~= nil and popup.is_open() end
  else
    root = build()
    for _, k in ipairs { "x", "y", "anchors" } do if spec[k] ~= nil then root[k] = spec[k] end end
  end
  -- The configuration's colour, when it keeps it.
  if type(spec.value) == "function" then
    local last
    morf.effect("kit.colour_picker.value." .. tostring(root), function()
      local v = spec.value()
      if v == last then return end
      last = v
      local c = read(v)
      if c and c:hex() ~= colour():hex() then take(c) end
    end, { owner = root })
  end
  return root, handle
end
