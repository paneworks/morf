-- Settings, Appearance: the palette and the wallpaper, how windows are
-- drawn, the typefaces and the motion (AppearanceSection).
--
-- The original's Windows part pushed rounding, gaps, borders and blur to
-- Hyprland; this port never writes the compositor, so that part keeps the
-- two options that belong to the shell itself (glass and shadow) and says
-- where the rest lives. Palettes and wallpapers are also chosen in the
-- island (`morf ipc call appearance`), which the first row opens.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local wallpaper = require("services.wallpaper")
local theme_service = require("services.theme")
local thumbnails = require("services.thumbnails")
local palette = require("lib.palette")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local font_picker = require("settings.font_picker")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

-- ----------------------------------------------------------------- data --

M.motion_presets = {
  { id = "smooth", label = "Smooth", scale = 100, curve = "OutCubic" },
  { id = "snappy", label = "Snappy", scale = 70, curve = "OutQuint" },
  { id = "springy", label = "Springy", scale = 110, curve = "OutBack" },
  { id = "off", label = "None", scale = 0, curve = "Linear" },
}

-- Hyprland's presets in the original; stored here for whoever animates the
-- windows, and drawn as the motion they name.
-- Each is one Bezier and the time windows take to arrive (Motion.qml's
-- `slow`, in deciseconds), and a window that slides or pops in.
M.window_presets = {
  { id = "macos", label = "Glide", note = "Scales into place and decelerates with a long tail.",
    curve = { 0.32, 0.72, 0, 1 }, slow = 2.6, windows = "slide" },
  { id = "snappy", label = "Brisk", note = "Half the distance and half the time.",
    curve = { 0.22, 1, 0.36, 1 }, slow = 1.3, windows = "popin 94%" },
  { id = "smooth", label = "Calm", note = "Long and soft, and workspaces fade rather than slide.",
    curve = { 0.25, 0.1, 0.25, 1 }, slow = 4.2, windows = "popin 85%" },
  { id = "springy", label = "Bounce", note = "Overshoots a little and settles back.",
    curve = { 0.05, 0.9, 0.1, 1.15 }, slow = 3.2, windows = "popin 80%" },
  { id = "off", label = "None", note = "Nothing moves." },
}

M.greetings = {
  { id = "lava", label = "Lava lamp", glyph = "󰈸" },
  { id = "critters", label = "Critters", glyph = "󰩃" },
  { id = "koi", label = "Koi", glyph = "󰈺" },
  { id = "invaders", label = "Invaders", glyph = "󰊠" },
  { id = "random", label = "Random", glyph = "󰒝" },
}

local EASING = { OutCubic = "out_cubic", OutQuint = "out_quint", OutBack = "out_back", Linear = "linear" }

--- The five daubs of a palette entry: accent, green, yellow, red, blue.
function M.daubs_of(id)
  local keys = { "accent", "green", "yellow", "red", "blue" }
  if id == "adaptive" then
    local p = theme_service.derived
    if p then
      local hex = palette.to_hex(p)
      local out = {}
      for i, key in ipairs(keys) do out[i] = hex[key] end
      return out
    end
    local out = {}
    for i, key in ipairs(keys) do out[i] = theme.color[key]() end
    return out
  end
  for _, preset in ipairs(palette.presets) do
    if preset.id == id then
      local out = {}
      for i, key in ipairs(keys) do out[i] = preset.colors[key] end
      return out
    end
  end
  return {}
end

-- ------------------------------------------------------------- pictures --

--- A palette as five daubs over its name, on the island's surface.
function M.palette_face(entry, size)
  local d = size or 12
  local daubs = { gap = 6, align = "center" }
  for i = 1, 5 do
    daubs[i] = ui.Rect {
      width = d, height = d, radius = d / 2, border_width = 1, border_color = C.hairline,
      color = function()
        if entry.id == "adaptive" then theme_service.active_id:get() end
        return M.daubs_of(entry.id)[i] or C.surface()
      end,
      behavior = { color = { duration = 260 } },
    }
  end
  return ui.Column {
    anchors = { center_in = true }, gap = 8, align = "center",
    ui.Row(daubs),
  }
end

-- ----------------------------------------------------- transition preview --
--
-- WallpaperTransitionPreview.qml: the current wallpaper and the next one in
-- the folder, the next revealed over the last in the transition's own way,
-- then held, then the two swap roles, so the loop never cuts. "random"
-- cycles through the five effects, one per pass.
--
-- The original fills a Shape with the incoming picture. morf clips to a
-- rounded rectangle only, so the circles are round ClipRects, and the
-- diagonal wipe and the wavy edge are the picture cut into thin bands, each
-- revealed on its own schedule: a staircase of 16 steps across 48 pixels.

local PREVIEW_W, PREVIEW_H = 84, 48
local BANDS = 16
local POOL = { "fade", "wipe", "wave", "circle", "outer" }

-- The current wallpaper and the one after it, in `list` (the folder, read
-- once for the page).
local function pictures(list)
  local current = wallpaper.current:get()
  local at = 1
  for i, path in ipairs(list) do if path == current then at = i end end
  local first = list[at] or ""
  local second = #list > 1 and list[at % #list + 1] or ""
  return first, second
end

local function thumb(path)
  if path == "" then return "" end
  return thumbnails.of(path, PREVIEW_W * 2, PREVIEW_H * 2)
end

function M.transition_picture(kind)
  local W, H = PREVIEW_W, PREVIEW_H
  local reach = math.sqrt(W * W + H * H) / 2
  local showing = controls.signal("transition.showing", 1)   -- which picture is under
  local drawn = controls.signal("transition.drawn", kind == "random" and POOL[1] or kind)
  local pass = 0
  local list = wallpaper.scan()
  local function under() local a, b = pictures(list) return showing:get() == 1 and a or b end
  local function over() local a, b = pictures(list) return showing:get() == 1 and b or a end
  local function picture(source, x, y)
    return ui.Image { x = x or 0, y = y or 0, width = W, height = H, fill_mode = "preserve_aspect_crop",
      source = function() return thumb(source()) end }
  end
  -- The arriving picture, on the accent where there is none.
  local function arriving(x, y)
    return ui.Item { x = x or 0, y = y or 0, width = W, height = H,
      ui.Rect { width = W, height = H, color = C.accent }, picture(over) }
  end
  local is = function(name) return function() return drawn:get() == name end end

  -- Fade and a cut: the whole picture, its opacity animated.
  local whole = ui.Item { width = W, height = H, opacity = 0,
    visible = function() local d = drawn:get() return d == "fade" or d == "none" or d == "outer" end,
    arriving() }
  -- Bands: a wipe and a wave.
  local bands = {}
  local band_h = H / BANDS
  for i = 1, BANDS do
    local y = (i - 1) * band_h
    local inner = arriving(0, -y)
    bands[i] = { inner = inner, clip = ui.ClipRect { x = 0, y = y, width = 0.001, height = band_h + 0.5,
      color = "#00000000", inner } }
  end
  local band_nodes = { width = W, height = H, visible = function() local d = drawn:get() return d == "wipe" or d == "wave" end }
  for i, b in ipairs(bands) do band_nodes[i] = b.clip end
  local band_layer = ui.Item(band_nodes)
  -- A circle growing with the arriving picture in it; for "outer", one
  -- shrinking with the old picture in it, over the new.
  local circle_inner = ui.Item { width = W, height = H,
    ui.Item { width = W, height = H, visible = is("circle"), arriving() },
    ui.Item { width = W, height = H, visible = is("outer"),
      ui.Rect { width = W, height = H, color = C.surface }, picture(under) },
  }
  local circle = ui.ClipRect { x = W / 2, y = H / 2, width = 0.001, height = 0.001, radius = 0,
    color = "#00000000", visible = function() local d = drawn:get() return d == "circle" or d == "outer" end,
    circle_inner }

  local node = ui.ClipRect {
    anchors = { center_in = true }, width = W, height = H, radius = theme.radius_small,
    color = C.surface, border_width = 1, border_color = C.islandBorder,
    content_under_border = false,
    picture(under),
    whole, band_layer, circle,
  }

  -- One pass: every piece set to where the transition starts, then moved on
  -- one schedule, held, and the pictures swapped.
  local function tracks(d)
    local out = {}
    -- Keyframes strictly in order: a band whose schedule starts at the
    -- very beginning or ends at the very end drops the repeated point.
    local function track(target, property, frames)
      local kept, last = {}, -1
      for _, f in ipairs(frames) do
        if f.at > last then kept[#kept + 1] = f last = f.at end
      end
      out[#out + 1] = { node = target, property = property, duration = 900, keyframes = kept }
    end
    if d == "fade" then
      track(whole, "opacity", { { at = 0, value = 0 }, { at = 1, value = 1 } })
    elseif d == "none" then
      track(whole, "opacity", { { at = 0, value = 0 }, { at = 0.499, value = 0 }, { at = 0.5, value = 1 }, { at = 1, value = 1 } })
    elseif d == "wipe" then
      -- Top right to bottom left: at progress p a band at y shows from
      -- y + W - p (W + H) to the right edge.
      for i, b in ipairs(bands) do
        local y = (i - 0.5) * band_h
        local p0, p1 = y / (W + H), (y + W) / (W + H)
        local xs = { { at = 0, value = W }, { at = p0, value = W }, { at = p1, value = 0 }, { at = 1, value = 0 } }
        track(b.clip, "x", xs)
        local ws = {}
        for k, f in ipairs(xs) do ws[k] = { at = f.at, value = math.max(0.001, W - f.value) } end
        track(b.clip, "width", ws)
        local ins = {}
        for k, f in ipairs(xs) do ins[k] = { at = f.at, value = -f.value } end
        track(b.inner, "x", ins)
      end
    elseif d == "wave" then
      -- Left to right with a sinusoidal edge.
      local amplitude = H / 6
      for i, b in ipairs(bands) do
        local y = (i - 0.5) * band_h
        local offset = amplitude * math.sin(y / H * 2 * math.pi * 1.2) - amplitude
        -- edge = p (W + 2A) + offset, clamped to the frame
        local p0 = math.max(0, math.min(1, -offset / (W + 2 * amplitude)))
        local p1 = math.max(0, math.min(1, (W - offset) / (W + 2 * amplitude)))
        track(b.clip, "width", { { at = 0, value = 0.001 }, { at = p0, value = 0.001 },
          { at = p1, value = W }, { at = 1, value = W } })
      end
    elseif d == "circle" or d == "outer" then
      local r0, r1 = 0.001, reach
      if d == "outer" then r0, r1 = reach, 0.001 end
      track(circle, "x", { { at = 0, value = W / 2 - r0 }, { at = 1, value = W / 2 - r1 } })
      track(circle, "y", { { at = 0, value = H / 2 - r0 }, { at = 1, value = H / 2 - r1 } })
      track(circle, "width", { { at = 0, value = 2 * r0 }, { at = 1, value = 2 * r1 } })
      track(circle, "height", { { at = 0, value = 2 * r0 }, { at = 1, value = 2 * r1 } })
      track(circle, "radius", { { at = 0, value = r0 }, { at = 1, value = r1 } })
      track(circle_inner, "x", { { at = 0, value = r0 - W / 2 }, { at = 1, value = r1 - W / 2 } })
      track(circle_inner, "y", { { at = 0, value = r0 - H / 2 }, { at = 1, value = r1 - H / 2 } })
    end
    return out
  end
  local function reset(d)
    whole.opacity = d == "outer" and 1 or 0
    for _, b in ipairs(bands) do
      b.clip.width = 0.001
      b.clip.x = d == "wipe" and W or 0
      b.inner.x = d == "wipe" and -W or 0
    end
    local r = d == "outer" and reach or 0.001
    circle.x, circle.y, circle.width, circle.height, circle.radius = W / 2 - r, H / 2 - r, 2 * r, 2 * r, r
    circle_inner.x, circle_inner.y = r - W / 2, r - H / 2
  end
  local run
  run = function()
    local d = kind == "random" and POOL[pass % #POOL + 1] or kind
    drawn:set(d)
    reset(d)
    morf.animation.play {
      { pause = 700 },
      { parallel = tracks(d) },
      { pause = 700 },
      on_finished = function(reason)
        if reason ~= "completed" then return end
        showing:set(showing:get() == 1 and 2 or 1)
        pass = pass + 1
        run()
      end,
    }
  end
  morf.timer(1, run, false)
  return node
end

--- The island's own morph at a pace: a capsule growing and shrinking.
function M.island_motion(preset)
  local ms = math.max(1, math.floor(380 * preset.scale / 100 + 0.5))
  local node = ui.Rect {
    anchors = { center_in = true }, width = 34, height = 14, radius = 7, color = C.island,
    border_width = 1, border_color = C.islandBorder,
    loop = preset.scale > 0 and {
      width = { from = 34, to = 78, duration = ms, easing = EASING[preset.curve] or "out_cubic",
        alternate = true, delay = 500 },
      height = { from = 14, to = 30, duration = ms, easing = EASING[preset.curve] or "out_cubic",
        alternate = true, delay = 500 },
    } or nil,
    ui.Rect { anchors = { center_in = true }, width = 6, height = 6, radius = 3, color = C.accent },
  }
  return node
end

--- A window arriving on a tiny screen with the preset's curve and duration
--- (MotionPreview.qml): popping in for a popin preset, sliding up for a
--- slide one, appearing at once for None. It never fades: every preset turns
--- the fade off, so it shows, stays, and is gone, every 1.3 s.
function M.window_motion(preset)
  local still = preset.curve == nil
  local slides = tostring(preset.windows or ""):sub(1, 5) == "slide"
  local span = still and 0 or math.floor(preset.slow * 100 + 0.5)
  local win = ui.Rect {
    x = 10, y = 8, width = 72, height = 30, radius = 4,
    color = C.accent, opacity = 0,
  }
  local screen = ui.ClipRect {
    anchors = { center_in = true }, width = 92, height = 46, radius = 5,
    color = C.island, border_width = 1, border_color = C.islandBorder,
    win,
  }
  local function pass()
    local steps = { { node = win, property = "opacity", from = 0, to = 1, duration = 1 } }
    if not still then
      local easing = { x1 = preset.curve[1], y1 = preset.curve[2], x2 = preset.curve[3], y2 = preset.curve[4] }
      if slides then
        steps[#steps + 1] = { node = win, property = "translate_y", from = 12, to = 0, duration = span, easing = easing }
      else
        steps[#steps + 1] = { node = win, property = "scale", from = 0.8, to = 1, duration = span, easing = easing }
      end
    end
    morf.animation.play {
      loops = "forever",
      { parallel = steps },
      { pause = math.max(1, 1300 - span) },
      { node = win, property = "opacity", from = 1, to = 0, duration = 1 },
      { pause = 1300 },
    }
  end
  morf.timer(1, pass, false)
  return screen
end

-- ---------------------------------------------------------------- parts --

local function theme_part(W)
  local groups = {}
  groups[#groups + 1] = setting.group {
    width = W, title = "Theme",
    note = "The palette follows the wallpaper, and both are also chosen in the island.",
    hint = "Open shows the island's appearance panel, where wallpapers and palettes are strips you slide through.",
    setting.row { width = W, label = "Theme and wallpaper",
      reading = function()
        local id = theme_service.active_id:get()
        for _, entry in ipairs(theme_service.available()) do
          if entry.id == id then return entry.name end
        end
        return id
      end,
      control = controls.pill { text = "Open", icon = "󰏘", active = true, height = 30, width = 92,
        on_click = function() require("bar.island").open("appearance") end } },
  }

  -- Palettes: adaptive and the nine presets, two rows of five.
  local tiles = {}
  for _, entry in ipairs(theme_service.available()) do
    tiles[#tiles + 1] = function(tile_w)
      return setting.tile {
        width = tile_w, stage_height = 40, caption = entry.id == "adaptive" and "Adaptive" or entry.name,
        selected = function() return theme_service.active_id:get() == entry.id end,
        on_picked = function() theme_service.set_theme(entry.id) end,
        stage = function() return M.palette_face(entry, 11) end,
      }
    end
  end
  groups[#groups + 1] = setting.group {
    width = W, title = "Palette",
    note = "Adaptive reads its colours from the wallpaper; the others are fixed schemes.",
    setting.tiles { width = W, label = "Palette", columns = 5, tiles = tiles,
      reading = function()
        local id = theme_service.active_id:get()
        if id == "adaptive" then return "Drawn from the wallpaper" end
        for _, preset in ipairs(palette.presets) do
          if preset.id == id then return preset.badge or "" end
        end
        return ""
      end },
  }

  -- Wallpapers as thumbnails, made small once in the cache.
  local COLS = 5
  local tile_w = math.floor((W - 28 - (COLS - 1) * 10) / COLS)
  local tile_h = math.floor(tile_w * 0.62)
  local list = wallpaper.scan()
  local cells = { columns = COLS, gap = 10 }
  for _, path in ipairs(list) do
    local hovered = controls.signal("wallpaper.tile", false)
    local applied = function() return wallpaper.current:get() == path end
    cells[#cells + 1] = ui.ClipRect {
      width = tile_w, height = tile_h, radius = theme.radius_medium, color = C.islandSurface,
      border_width = function() return applied() and 2 or 1 end,
      border_color = function()
        if applied() then return C.accent() end
        return hovered:get() and C.islandBorder or C.islandSurfaceHover
      end,
      behavior = { border_color = fast() },
      ui.Image {
        anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
        source = function() return thumbnails.of(path, tile_w * 2, tile_h * 2) end,
      },
      ui.Rect {
        anchors = { top = true, right = true, margins = 6 }, width = 18, height = 18, radius = 9,
        color = C.accent, visible = applied,
        kit.glyph { anchors = { center_in = true }, glyph = "󰄬", size = 10, color = C.accentText },
      },
      setting.hit { hovered = hovered, on_click = function() wallpaper.apply(path) end },
    }
  end
  -- Only built when there is something to show: a node made and never
  -- placed is left at the top level of the scene.
  local grid = #list > 0 and ui.Grid(cells) or nil
  groups[#groups + 1] = setting.group {
    width = W, title = "Wallpaper",
    note = "Every picture in the wallpaper folder.",
    hint = "The folder is the wallpaperDir setting. Adding pictures there adds them here the next time this page opens.",
    setting.row { width = W, label = "Folder", reading = wallpaper.dir(),
      control = kit.text { text = #list .. (#list == 1 and " picture" or " pictures"),
        size = theme.size.small, color = C.textMuted } },
    setting.block { width = W, #list > 0 and grid or kit.text {
      text = "No pictures in the folder", size = theme.size.small, color = C.textMuted } },
  }

  local transitions = {}
  for _, entry in ipairs(wallpaper.TRANSITIONS) do
    transitions[#transitions + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 52, caption = entry.label,
        selected = function() return settings.wallpaperTransition == entry.id end,
        on_picked = function() settings.set("wallpaperTransition", entry.id) end,
        stage = function() return M.transition_picture(entry.id) end,
      }
    end
  end
  groups[#groups + 1] = setting.group {
    width = W, title = "Transition",
    note = "How the next wallpaper arrives over the last one.",
    hint = "Random picks one of the effects each time a wallpaper is put up.",
    setting.tiles { width = W, label = "Effect", tiles = transitions },
  }

  local greetings = {}
  for _, entry in ipairs(M.greetings) do
    greetings[#greetings + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 52, caption = entry.label,
        selected = function() return settings.greeting == entry.id end,
        on_picked = function() settings.set("greeting", entry.id) end,
        stage = function()
          return kit.glyph { anchors = { center_in = true }, glyph = entry.glyph, size = 24,
            color = C.accent }
        end,
      }
    end
  end
  groups[#groups + 1] = setting.group {
    width = W, title = "Greeting",
    note = "The scene shown beside fastfetch when you run fa.",
    setting.tiles { width = W, label = "Scene", tiles = greetings },
  }
  return groups
end

--- Two windows over the wallpaper, lifted and frosted as the switches say
--- (WindowsPreview, for the two options the shell owns).
local function windows_preview(W)
  local w, h = W - 28, 170
  local function window(x, y, ww, wh)
    return ui.Rect {
      x = x, y = y, width = ww, height = wh, radius = 12,
      color = function()
        return settings.windowGlass and C.surface():alpha(0.55) or C.surface()
      end,
      border_width = 1, border_color = C.islandBorder,
      shadow_color = function() return settings.windowShadow and "#00000099" or "#00000000" end,
      shadow_blur = 18, shadow_offset_y = 6,
      behavior = { color = fast(), shadow_color = fast() },
      ui.Rect { x = 12, y = 12, width = ww * 0.4, height = 8, radius = 4, color = C.textMuted, opacity = 0.6 },
      ui.Rect { x = 12, y = 28, width = ww * 0.6, height = 6, radius = 3, color = C.textMuted, opacity = 0.3 },
      ui.Rect { x = 12, y = 40, width = ww * 0.5, height = 6, radius = 3, color = C.textMuted, opacity = 0.3 },
    }
  end
  return ui.ClipRect {
    width = w, height = h, radius = theme.radius_medium, color = C.island,
    ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
      source = function() return thumbnails.of(wallpaper.current:get(), math.floor(w / 2), math.floor(h / 2)) end },
    window(24, 22, w * 0.46, h - 44),
    window(w * 0.46 + 38, 22, w * 0.54 - 62, (h - 44) / 2 - 7),
    window(w * 0.46 + 38, 22 + (h - 44) / 2 + 7, w * 0.54 - 62, (h - 44) / 2 - 7),
  }
end

local function windows_part(W)
  return {
    setting.group {
      width = W, title = "Depth",
      note = "What a window sits on, where the shell draws it.",
      hint = "Rounding, gaps, borders and blur belong to the compositor's own configuration; this shell never writes it.",
      setting.block { width = W, windows_preview(W) },
      setting.switch_row { width = W, label = "Glass",
        reading = function() return settings.windowGlass and "Frosted, where the compositor can" or "Clear" end,
        checked = function() return settings.windowGlass end,
        on_toggled = function(on) settings.set("windowGlass", on) end },
      setting.switch_row { width = W, label = "Shadow",
        reading = function() return settings.windowShadow and "Lifted off the wallpaper" or "Flat" end,
        checked = function() return settings.windowShadow end,
        on_toggled = function(on) settings.set("windowShadow", on) end },
    },
    setting.group {
      width = W, title = "The compositor's",
      note = "Rounding, gaps, borders, blur and window rules.",
      setting.row { width = W, label = "Where they live",
        reading = "In the compositor's own files, which the shell leaves alone" },
    },
  }
end

local function type_part(W)
  return {
    setting.group {
      width = W, title = "Interface", note = "Everything the shell draws.",
      setting.block { width = W, padding = 6,
        font_picker.new { width = W - 12, kind = "sans",
          current = function() return settings.fontFamily end,
          sample = "The quick brown fox 0123",
          on_picked = function(family) settings.set("fontFamily", family) end } },
    },
    setting.group {
      width = W, title = "Monospace", note = "The bar's glyphs and readings.",
      hint = "It must be a Nerd Font, or the bar's icons show as empty boxes.",
      setting.block { width = W, padding = 6,
        font_picker.new { width = W - 12, kind = "mono",
          current = function() return settings.fontMono end,
          sample = "il1 O0 {} => 0123",
          warning = "Icons need a Nerd Font patched family.",
          on_picked = function(family) settings.set("fontMono", family) end } },
    },
  }
end

local function motion_part(W)
  local shell_tiles = {}
  for _, preset in ipairs(M.motion_presets) do
    shell_tiles[#shell_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 52, caption = preset.label,
        selected = function()
          return settings.motionScale == preset.scale and settings.motionCurve == preset.curve
        end,
        on_picked = function()
          settings.set("motionScale", preset.scale)
          settings.set("motionCurve", preset.curve)
        end,
        stage = function() return M.island_motion(preset) end,
      }
    end
  end
  local window_tiles = {}
  for _, preset in ipairs(M.window_presets) do
    window_tiles[#window_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 52, caption = preset.label,
        selected = function() return settings.animationPreset == preset.id end,
        on_picked = function() settings.set("animationPreset", preset.id) end,
        stage = function() return M.window_motion(preset) end,
      }
    end
  end
  local hint = {}
  for _, preset in ipairs(M.window_presets) do hint[#hint + 1] = preset.label .. " — " .. preset.note end
  return {
    setting.group {
      width = W, title = "The shell",
      note = "The island, its panels, the chips — everything the shell draws.",
      setting.tiles { width = W, label = "Pace",
        reading = function()
          for _, preset in ipairs(M.motion_presets) do
            if settings.motionScale == preset.scale and settings.motionCurve == preset.curve then return "" end
          end
          return "Custom"
        end,
        tiles = shell_tiles },
      setting.slider { width = W, label = "Speed", from = 0, to = 200, step = 10, unit = "%",
        value = function() return settings.motionScale end,
        reading = function() return settings.motionScale == 0 and "Off" or (settings.motionScale .. "%") end,
        on_moved = function(v) settings.set("motionScale", v) end },
    },
    setting.group {
      width = W, title = "Windows",
      note = "How windows arrive. Kept for the compositor to read; nothing is pushed to it.",
      hint = table.concat(hint, " "),
      setting.tiles { width = W, label = "Preset", tiles = window_tiles },
    },
  }
end

function M.build(page)
  return setting.parts(page, {
    { id = "theme", build = theme_part }, { id = "windows", build = windows_part },
    { id = "type", build = type_part }, { id = "motion", build = motion_part },
  })
end

return M
