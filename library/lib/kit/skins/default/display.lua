-- The default kit's display widgets besides `label`, `box` and `frame`
-- (text.lua, containers.lua): image, avatar, spinner, progress_bar,
-- level_bar, separator, status_page, shortcut_label, window_title and
-- button_content, as libadwaita draws them. `draw.spinner` is the spinner
-- the presses and lists that wait draw too.
local morf = require("morf")
local ui = require("morf.ui")

local FIT = { contain = "preserve_aspect_fit", cover = "preserve_aspect_crop", fill = "stretch" }
local TONES = { "accent_bg", "success_bg", "warning_bg", "error_bg", "info_bg", "extra_bg" }
-- An icon's colour classes, as a label's.
local TONE = { dim = "dim_fg", accent = "accent", success = "success", warning = "warning", error = "error" }

local function clamp01(x)
  x = tonumber(x) or 0
  if x ~= x then return 0 end
  return x < 0 and 0 or (x > 1 and 1 or x)
end

-- The spec's own node fields, as given.
local function place(spec, extra)
  local out = {}
  for _, k in ipairs { "id", "x", "y", "z", "anchors", "visible", "opacity", "layout", "width", "height" } do
    out[k] = spec[k]
  end
  for k, v in pairs(extra or {}) do if out[k] == nil then out[k] = v end end
  return out
end

-- SVG path data for an arc of `sweep` degrees clockwise from `from` (0 at
-- twelve o'clock), radius `r` about `(cx, cy)`.
local function arc(cx, cy, r, from, sweep)
  local function at(deg)
    local a = math.rad(deg)
    return cx + r * math.sin(a), cy - r * math.cos(a)
  end
  local x, y = at(from)
  local d = { ("M%.3f %.3f"):format(x, y) }
  local pieces = math.max(1, math.ceil(sweep / 90))
  for i = 1, pieces do
    local ex, ey = at(from + sweep * i / pieces)
    d[#d + 1] = ("A%.3f %.3f 0 0 1 %.3f %.3f"):format(r, r, ex, ey)
  end
  return table.concat(d, " ")
end

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get = draw.get
  local press = require("lib.kit.archetypes.press")

  --- libadwaita's spinner: an arc of the ring turning, `size` across, in
  --- `color` (the dim ink). `props.active()` runs it; stopped, it rests as
  --- a faint ring. Only the turn moves: a transform, no path changes.
  function draw.spinner(size, color, props)
    props = props or {}
    local active = props.active or function() return true end
    local stroke = math.max(2, size / 9)
    local r = size / 2 - stroke / 2
    color = color or function() return P().dim_fg end
    return ui.Item { id = props.id, x = props.x, y = props.y, anchors = props.anchors, width = size, height = size,
      visible = props.visible, accessible_role = "progress", accessible_name = props.accessible_name or "Loading",
      ui.Path { anchors = { fill = true }, view_box = { 0, 0, size, size }, d = arc(size / 2, size / 2, r, 0, 360),
        fill_color = "transparent", stroke_width = stroke, stroke_color = function() return get(color):alpha(0.2) end },
      ui.Item { anchors = { fill = true },
        visible = function() return active() end,
        loop = function()
          if not active() or theme.reduced then return nil end
          return { rotation = { from = 0, to = 360, duration = 1100, easing = "linear" } }
        end,
        ui.Path { anchors = { fill = true }, view_box = { 0, 0, size, size }, d = arc(size / 2, size / 2, r, 0, 100),
          fill_color = "transparent", stroke_width = stroke, stroke_cap = "round", stroke_color = color } },
    }
  end

  --- GtkImage / GtkPicture: `icon` (a symbolic name) at `pixel_size` (16)
  --- in `color`, or `source` (a file) fitted by `fit` ("cover", "contain",
  --- "fill") with `radius`; `alt` its words to a screen reader.
  function S.image(spec)
    if spec.icon then
      local size = spec.pixel_size or spec.size or 16
      local color = spec.color
      for class, role in pairs(TONE) do
        if color == nil and press.has(spec, class) then color = function() return P()[role] end end
      end
      return draw.icon(spec.icon, size, color, place(spec, { width = size, height = size,
        accessible_name = spec.alt, fill = spec.fill }))
    end
    local node = ui.Rect(place(spec, { width = 200, height = 140, clip = true, radius = spec.radius or 0,
      color = function() return P().thumbnail_bg end }))
    ui.reparent(ui.Image { anchors = { fill = true }, source = spec.source or "",
      fill_mode = FIT[spec.fit or "cover"] or "preserve_aspect_crop" }, node)
    node.accessible_role = "image"
    node.accessible_name = spec.alt or "Image"
    return node
  end

  -- Initials for a name: the first letters of its first two words.
  local function initials(name)
    local out = {}
    for word in tostring(name or "?"):gmatch("%S+") do
      out[#out + 1] = utf8.char(utf8.codepoint(word, 1))
      if #out == 2 then break end
    end
    return table.concat(out):upper()
  end

  --- AdwAvatar: `text` (the name: its initials on a tone of its own),
  --- `size` (32), `source` (a picture instead), `show_initials` (true).
  function S.avatar(spec)
    local size = spec.size or 32
    local function name() return tostring(get(spec.text) or "") end
    local function tone()
      local sum, n = 0, name()
      for i = 1, #n do sum = sum + n:byte(i) * i end
      return P()[TONES[sum % #TONES + 1]]
    end
    local node = ui.Rect(place(spec, { width = size, height = size, radius = size / 2, clip = true,
      color = function() return tone():mix(P().view_bg, P().dark and 0.2 or 0.35) end }))
    if spec.show_initials ~= false then
      ui.reparent(draw.text { anchors = { center_in = true }, text = function() return initials(name()) end,
        font_size = math.floor(size * 0.4), font_weight = 700, color = function() return P().accent_fg end }, node)
    end
    if spec.source then
      ui.reparent(ui.Image { anchors = { fill = true }, source = spec.source, fill_mode = "preserve_aspect_crop" }, node)
    end
    node.accessible_role, node.accessible_name = "image", spec.accessible_name or name
    return node
  end

  --- AdwSpinner: `size` (32), `active` (a binding; on by default).
  function S.spinner(spec)
    return draw.spinner(spec.size or 32, spec.color, spec)
  end

  --- GtkProgressBar: `fraction` (0..1, a binding), `pulse` (busy with no
  --- fraction), `text` beside it; `osd` in white, for the dark osd ground.
  function S.progress_bar(spec)
    local osd = press.has(spec, "osd")
    local th = 6
    local node = ui.Item(place(spec, { width = 200, height = spec.text and 28 or th,
      accessible_role = "progress" }))
    local function w() return node.layout_width or 0 end
    local function fraction() return clamp01(get(spec.fraction)) end
    ui.reparent(ui.Rect { anchors = { left = true, right = true, bottom = true }, height = th, radius = th / 2,
      color = function() return osd and morf.color("#ffffff"):alpha(0.25) or P().track end }, node)
    ui.reparent(ui.Rect { anchors = { left = true, bottom = true }, height = th, radius = th / 2,
      width = function() return w() * fraction() end, color = function() return osd and morf.color("#ffffff") or P().accent_bg end,
      behavior = { width = draw.quick() } }, node)
    if spec.text then
      ui.reparent(draw.text { anchors = { left = true, top = true }, height = 18, text = spec.text,
        font_size = theme.size.small, color = function() return P().dim_fg end }, node)
    end
    node.accessible = { value = fraction, minimum = 0, maximum = 1 }
    return node
  end

  --- GtkLevelBar: `value` (a binding) between `min_value` (0) and
  --- `max_value` (1); `mode` "continuous" or "discrete" (one block per
  --- step); `offsets` { low, high } colour it warning below low and success
  --- above high, the accent between.
  function S.level_bar(spec)
    local lo, hi = spec.min_value or 0, spec.max_value or 1
    local th = spec.thickness or 6
    local vertical = spec.orientation == "vertical"
    local node = ui.Item(place(spec, { width = vertical and th or 200, height = vertical and 120 or th,
      accessible_role = "meter" }))
    local function fraction() return clamp01(((tonumber(get(spec.value)) or 0) - lo) / math.max(1e-9, hi - lo)) end
    local function tone()
      local p, f, offsets = P(), fraction(), spec.offsets or {}
      if offsets.low and f < offsets.low then return p.warning_bg end
      if offsets.high and f >= offsets.high then return p.success_bg end
      return p.accent_bg
    end
    local blocks = spec.mode == "discrete" and math.max(1, math.floor(hi - lo)) or 1
    local gap = blocks > 1 and 2 or 0
    for i = 1, blocks do
      local function from() return (i - 1) / blocks end
      local function extent()
        local size = vertical and (node.layout_height or 0) or (node.layout_width or 0)
        return (size - gap * (blocks - 1)) / blocks
      end
      local function at() return (i - 1) * (extent() + gap) end
      local function full() return clamp01((fraction() - from()) * blocks) end
      ui.reparent(ui.Rect { radius = th / 2, color = function() return P().track end,
        x = not vertical and at or 0, y = vertical and at or 0,
        width = vertical and th or extent, height = vertical and extent or th }, node)
      ui.reparent(ui.Rect { radius = th / 2, color = tone,
        x = not vertical and at or 0,
        y = vertical and function() return at() + extent() * (1 - full()) end or 0,
        width = vertical and th or function() return extent() * full() end,
        height = vertical and function() return extent() * full() end or th,
        behavior = { width = draw.quick(), height = draw.quick() } }, node)
    end
    node.accessible = { value = function() return tonumber(get(spec.value)) or 0 end, minimum = lo, maximum = hi }
    return node
  end

  --- GtkSeparator: a hairline along `orientation` ("horizontal"), across
  --- its parent unless its length is given; the `spacer` variant keeps its
  --- room and draws nothing.
  function S.separator(spec)
    local vertical = spec.orientation == "vertical"
    local sized = (vertical and spec.height ~= nil) or (not vertical and spec.width ~= nil)
    return ui.Rect(place(spec, { width = vertical and 1 or nil, height = vertical and nil or 1,
      anchors = spec.anchors or (not sized and (vertical and { top = true, bottom = true } or { left = true, right = true })) or nil,
      color = function()
        if press.has(spec, "spacer") then return P().border:alpha(0) end
        return P().border
      end }))
  end

  --- AdwStatusPage: an `icon_name` large and dim, `title`, `description`,
  --- and a child below; `compact` smaller, for a sidebar or a popover.
  function S.status_page(spec)
    local compact = press.has(spec, "compact")
    local column = { anchors = { center_in = true }, gap = compact and 6 or 12, align = "center" }
    if spec.icon_name then
      column[#column + 1] = draw.icon(spec.icon_name, compact and 32 or 96, function() return P().window_fg:alpha(0.55) end)
    end
    local width = spec.width and function() return math.max(1, (get(spec.width) or 0) - 48) end or 360
    if spec.title then
      local face = theme.type[compact and "title_2" or "title_1"]
      column[#column + 1] = draw.text { text = spec.title, font_size = face.size, font_weight = face.weight,
        horizontal_alignment = "center", wrap = true, width = width, accessible_role = "heading" }
    end
    if spec.description then
      column[#column + 1] = draw.text { text = spec.description, horizontal_alignment = "center", wrap = true,
        width = width }
    end
    for _, child in ipairs(spec) do column[#column + 1] = child end
    local node = ui.Item(place(spec, { width = 400, height = 320 }))
    ui.reparent(ui.Column(column), node)
    return node
  end

  --- GtkShortcutLabel: `accelerator` ("Ctrl+Shift+T"), each key a keycap.
  function S.shortcut_label(spec)
    local h = spec.height or 22
    local row = { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors, visible = spec.visible, gap = 4,
      align = "center", accessible_role = "label", accessible_name = spec.accelerator }
    for key in tostring(get(spec.accelerator) or ""):gmatch("[^+]+") do
      local width = math.max(h + 2, utf8.len(key) * 8 + 14)
      row[#row + 1] = ui.Item { width = width, height = h,
        ui.Rect { anchors = { fill = true }, radius = R.small, color = draw.wash("button"),
          border_width = 1, border_color = function() local p = P() return p.strong and p.border or p.window_fg:alpha(0.12) end },
        draw.text { anchors = { center_in = true }, text = key, font_size = theme.size.small - 1, font_weight = 700,
          color = function() return P().dim_fg end } }
    end
    return ui.Row(row)
  end

  --- AdwWindowTitle: `title` bold and `subtitle` dim under it, centred.
  function S.window_title(spec)
    local column = { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors or { center_in = true },
      visible = spec.visible, gap = 0, align = "center", accessible_role = "heading",
      draw.text { text = spec.title or "", font_weight = 700, elide = "right", width = spec.width } }
    if spec.subtitle then
      column[#column + 1] = draw.text { text = spec.subtitle, font_size = theme.size.small, elide = "right",
        width = spec.width, color = function() return P().dim_fg end }
    end
    return ui.Column(column)
  end

  --- AdwButtonContent: `icon_name` before `label`, as a button's child.
  function S.button_content(spec)
    local color = spec.color or function() return P().window_fg end
    return ui.Row { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors or { center_in = true },
      visible = spec.visible, gap = 6, align = "center",
      spec.icon_name and draw.icon(spec.icon_name, 16, color) or nil,
      draw.text { text = spec.label or "", font_weight = 700, color = color } }
  end
end
