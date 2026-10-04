-- Display widgets: media. See lib.kit.display.
--
-- Pictures and the people in them: an image (with a placeholder while it
-- loads or when it is missing), an avatar or an overlapped group of
-- them, a captioned thumbnail, and a video's still with its play mark.
local ui = require("morf.ui")
local U = require("lib.kit.display.util")

local M = {}
local get = U.get

-- `fit` as the Image element's fill mode.
local FIT = { contain = "preserve_aspect_fit", cover = "preserve_aspect_crop", fill = "stretch",
  stretch = "stretch" }

-- A picture filling its parent, and a placeholder shown until it draws:
-- Material the track tone with a glyph; Tsugumori hatching and a caps word.
local function picture(spec, style, w, h, inset)
  inset = inset or 0
  local empty = spec.source == nil or spec.source == ""
  local hold
  if style.hatched then
    hold = ui.Item { x = inset, y = inset, width = w - inset * 2, height = h - inset * 2, visible = empty,
      ui.Rect { anchors = { fill = true }, color = U.alpha(style.ink_lo, 0.06) },
      style.stripes.box { width = w - inset * 2, height = h - inset * 2, gap = 7, weight = 1,
        color = U.alpha(style.ink_lo, 0.22) } }
    if w >= 70 and h >= 34 then
      ui.reparent(ui.Rect { anchors = { center_in = true }, width = math.min(w - 16, 74), height = 18,
        color = style.surface }, hold)
      ui.reparent(U.caption(style, { anchors = { center_in = true }, text = empty and "No image" or "No signal",
        font_size = style.size.small - 3, letter_spacing = 1, color = style.ink_lo }), hold)
    end
  else
    hold = ui.Item { x = inset, y = inset, width = w - inset * 2, height = h - inset * 2, visible = empty,
      style.icon("image", math.max(16, math.min(40, math.floor(math.min(w, h) * 0.32))), style.ink_lo,
        { anchors = { center_in = true } }) }
  end
  -- With no source the placeholder stands alone; with one, the plain
  -- surface holds the place while it loads and the placeholder comes up
  -- if the source fails (a missing or broken file).
  local img = ui.Image { x = inset, y = inset, width = w - inset * 2, height = h - inset * 2,
    source = spec.source or "", fill_mode = FIT[spec.fit or "cover"] or "preserve_aspect_crop",
    on_status = function(status) hold.visible = status == "error" or status == "none" end }
  return img, hold
end

--- A picture: `source`, `width` (200), `height` (140), `fit` ("contain"
--- | "cover" | "fill"), `radius` (the style's), `alt` (its words to a
--- screen reader). A placeholder stands in until it draws, and for good
--- when the file is missing or broken.
function M.image(spec, style)
  local w, h = spec.width or 200, spec.height or 140
  local node
  if style.hatched then
    node = ui.Item(U.place(spec, { width = w, height = h, clip = true }))
  else
    node = ui.Rect(U.place(spec, { width = w, height = h, clip = true, color = style.track,
      radius = spec.radius or math.min(16, math.min(w, h) / 4) }))
  end
  local img, hold = picture(spec, style, w, h, style.hatched and 1 or 0)
  ui.reparent(hold, node)
  ui.reparent(img, node)
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = "transparent", border_width = 1,
      border_color = style.line }, node)
  end
  node.accessible_role = "image"
  node.accessible_name = spec.alt or spec.label or "Image"
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

-- A palette tone picked by the name, so a person keeps their colour.
local function tone_of(style, name)
  local sum = 0
  for i = 1, #tostring(name or "") do sum = sum + tostring(name):byte(i) * i end
  return style.series(sum % 6 + 1)
end

-- One avatar of `s`: the picture or the initials on the name's tone.
-- `ring`: a gap of the surface around it, for a group's overlap.
local function face(spec, style, name, s, ring)
  local color = spec.color and U.color(spec, style) or tone_of(style, name)
  local node
  if style.hatched then
    node = ui.Item { width = s, height = s, clip = true,
      ring and ui.Rect { anchors = { fill = true }, color = style.surface } or nil }
    ui.reparent(ui.Rect { anchors = { fill = true, margins = ring and 2 or 0 }, color = U.alpha(color, 0.12),
      border_width = 1, border_color = color }, node)
    ui.reparent(style.stripes.box { x = 2, y = 2, width = s - 4, height = s - 4, gap = 6, weight = 1,
      color = U.alpha(color, 0.25) }, node)
  else
    node = ui.Rect { width = s, height = s, radius = s / 2, clip = true,
      color = function() return get(style.raised):mix(get(color), 0.3) end,
      border_width = ring and 2 or 0, border_color = style.surface }
  end
  local words = spec.text or initials(name)
  ui.reparent(style.text { anchors = { center_in = true }, text = words,
    font_size = math.max(style.size.small - 3, math.floor(s * (ring and 0.34 or 0.38))), font_weight = 600,
    color = style.hatched and color or style.ink, letter_spacing = style.hatched and 0.5 or 0 }, node)
  if spec.source then
    -- Material's is cut to the circle (a mask: only its alpha counts).
    ui.reparent(ui.Image { anchors = { fill = true, margins = ring and 2 or (style.hatched and 1 or 0) },
      source = spec.source, fill_mode = "preserve_aspect_crop",
      mask = not style.hatched and ui.Rect { radius = s / 2, color = style.ink } or nil }, node)
  end
  return node
end

--- A person's mark: `name` (its initials stand in), `source` (a picture,
--- optional), `size` (40); or `group`, a list of names (or `{ name,
--- source }`), drawn overlapped, `max` (4) of them and a `+n` for the rest.
function M.avatar(spec, style)
  local s = spec.size or 40
  if not spec.group then
    local node = face(spec, style, spec.name, s, false)
    U.place(spec, node)
    node.accessible_role = "image"
    node.accessible_name = spec.name or spec.alt or "Avatar"
    return node
  end
  local list, max = spec.group, spec.max or 4
  local shown = math.min(#list, max)
  local extra = #list - shown
  local step = math.floor(s * (style.hatched and 0.78 or 0.74))
  local count = shown + (extra > 0 and 1 or 0)
  local node = ui.Item(U.place(spec, { width = s + step * math.max(0, count - 1), height = s }))
  for i = 1, shown do
    local entry = list[i]
    local name, source = entry, nil
    if type(entry) == "table" then name, source = entry.name or entry[1], entry.source end
    local f = face({ source = source }, style, name, s, true)
    f.x = (i - 1) * step
    ui.reparent(f, node)
  end
  if extra > 0 then
    local f = face({ text = "+" .. extra, color = style.ink_lo }, style, "", s, true)
    f.x = shown * step
    ui.reparent(f, node)
  end
  local names = {}
  for i, entry in ipairs(list) do names[i] = type(entry) == "table" and (entry.name or entry[1]) or tostring(entry) end
  node.accessible_role = "list"
  node.accessible_name = table.concat(names, ", ")
  return node
end

--- A captioned picture: `source`, `width` (160), `height` (120, the
--- whole), `caption`, `index` (Tsugumori prints it as a code).
function M.thumbnail(spec, style)
  local w, h = spec.width or 160, spec.height or 120
  local ch = spec.caption and 24 or 0
  local ih = h - ch
  local node = ui.Item(U.place(spec, { width = w, height = h }))
  ui.reparent(M.image({ source = spec.source, width = w, height = ih, fit = spec.fit or "cover",
    alt = spec.alt or spec.caption, radius = spec.radius }, style), node)
  if spec.caption then
    if style.hatched then
      ui.reparent(ui.Rect { y = ih + 4, width = 5, height = 5, color = style.accent }, node)
      ui.reparent(U.caption(style, { text = spec.caption, x = 10, y = ih + 2, height = 18, width = w - 40,
        elide = "right", font_size = style.size.small - 3, letter_spacing = 0.8, color = style.ink }), node)
      if spec.index then
        ui.reparent(U.caption(style, { text = ("%02d"):format(spec.index), anchors = { right = true }, y = ih + 2,
          height = 18, font_size = style.size.small - 3, color = style.ink_lo }), node)
      end
    else
      ui.reparent(style.text { text = spec.caption, x = 2, y = ih + 4, height = 18, width = w - 4, elide = "right",
        font_size = style.size.small - 2, font_weight = 500, color = style.ink }, node)
    end
  end
  return node
end

--- A video's place: the engine has no video element, so this draws its
--- still (`source`, a picture; a placeholder without one) with a play
--- mark, the `duration` ("3:42") and how far it has `played` (0..1).
--- `width` (260), `height` (150), `title`, `fit`.
function M.video(spec, style)
  local w, h = spec.width or 260, spec.height or 150
  local node = M.image({ source = spec.source, width = w, height = h, fit = spec.fit or "cover",
    alt = spec.title or spec.alt or "Video", radius = spec.radius }, style)
  U.place(spec, node)
  -- A wash so the marks read over any still.
  ui.reparent(ui.Rect { anchors = { fill = true }, color = function() return get(style.surface):alpha(0.25) end }, node)
  local p = 52
  if style.hatched then
    p = 44
    ui.reparent(ui.Item { anchors = { center_in = true }, width = p, height = p,
      ui.Rect { anchors = { fill = true }, color = function() return get(style.surface):alpha(0.85) end,
        border_width = 1, border_color = style.accent },
      ui.Path { anchors = { center_in = true }, width = 16, height = 18, view_box = { 0, 0, 16, 18 },
        d = "M2 1 L15 9 L2 17 Z", fill_color = style.accent },
      style.marks(style.accent),
    }, node)
  else
    ui.reparent(ui.Rect { anchors = { center_in = true }, width = p, height = p, radius = p / 2,
      color = style.accent,
      style.icon("play_arrow", 32, style.on_accent, { anchors = { center_in = true }, fill = true }),
    }, node)
  end
  if spec.title then
    -- On a chip of the surface, so it reads over any still.
    local fs = style.size.small - (style.hatched and 2 or 1)
    local tw = math.min(w - 44, math.ceil((utf8.len(tostring(get(spec.title))) or 0) * fs * (style.hatched and 0.66 or 0.56)) + 2)
    local words = style.hatched
      and U.caption(style, { text = spec.title, width = tw, height = 18, elide = "right", font_size = fs,
        letter_spacing = 0.6, color = style.ink })
      or style.text { text = spec.title, width = tw, height = 18, elide = "right", font_size = fs, font_weight = 600,
        color = style.ink }
    ui.reparent(ui.Rect { x = 10, y = 10, radius = style.hatched and 0 or 12,
      color = function() return get(style.surface):alpha(0.85) end,
      ui.Inset { left_margin = 10, right_margin = 10, top_margin = 3, bottom_margin = 3, words } }, node)
  end
  if spec.duration then
    local d = tostring(get(spec.duration))
    local fs = style.size.small - 3
    local cw = math.ceil(#d * fs * 0.62) + 14
    ui.reparent(ui.Rect { anchors = { right = true, bottom = true, right_margin = 8, bottom_margin = 12 },
      width = cw, height = 20, radius = style.hatched and 0 or 10,
      color = function() return get(style.surface):alpha(0.85) end,
      style.text { anchors = { center_in = true }, text = d, font_family = style.mono_font, font_size = fs,
        font_weight = 600, color = style.ink },
    }, node)
  end
  if spec.played then
    ui.reparent(ui.Rect { x = 0, y = h - 3, width = w, height = 3, color = function() return get(style.ink):alpha(0.2) end }, node)
    ui.reparent(ui.Rect { x = 0, y = h - 3, height = 3, color = style.accent,
      width = function() return math.max(1, U.clamp01(get(spec.played)) * w) end,
      behavior = { width = style.spring() } }, node)
  end
  node.accessible_role = "image"
  node.accessible_name = spec.title or spec.alt or "Video"
  if spec.duration then node.accessible_description = "Video, " .. tostring(get(spec.duration)) end
  return node
end

return M
