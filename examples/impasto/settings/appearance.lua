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
M.window_presets = {
  { id = "macos", label = "Glide", note = "Scales into place and decelerates with a long tail." },
  { id = "snappy", label = "Brisk", note = "Half the distance and half the time." },
  { id = "smooth", label = "Calm", note = "Long and soft, and workspaces fade rather than slide." },
  { id = "springy", label = "Bounce", note = "Overshoots a little and settles back." },
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

--- Two panes, the next revealed over the last in the transition's own way,
--- looping.
function M.transition_picture(kind)
  local W, H = 76, 40
  local old = ui.Rect { x = 0, y = 0, width = W, height = H, color = C.islandSurfaceHover }
  local reveal
  if kind == "fade" then
    reveal = ui.Rect { x = 0, y = 0, width = W, height = H, color = C.accent,
      loop = { opacity = { from = 0, to = 1, duration = 1400, easing = "in_out_cubic", alternate = true } } }
  elseif kind == "wipe" or kind == "wave" then
    reveal = ui.Rect { x = 0, y = 0, height = H, color = C.accent, width = 1,
      loop = { width = { from = 1, to = W, duration = 1400, alternate = true,
        easing = kind == "wave" and "in_out_sine" or "in_out_cubic" } } }
  elseif kind == "circle" then
    reveal = ui.Rect { anchors = { center_in = true }, width = 1, height = 1, radius = 0.5,
      color = C.accent,
      loop = { scale = { from = 1, to = 2 * W, duration = 1400, easing = "in_out_cubic", alternate = true } } }
  elseif kind == "outer" then
    reveal = ui.Rect { x = 0, y = 0, width = W, height = H, color = C.accent,
      ui.Rect { anchors = { center_in = true }, width = 1, height = 1, radius = 0.5,
        color = C.islandSurfaceHover,
        loop = { scale = { from = 2 * W, to = 1, duration = 1400, easing = "in_out_cubic", alternate = true } } } }
  elseif kind == "random" then
    reveal = kit.glyph { anchors = { center_in = true }, glyph = "󰒝", size = 18, color = C.accent }
  else
    reveal = ui.Rect { x = W / 2, y = 0, width = W / 2, height = H, color = C.accent }
  end
  return ui.ClipRect {
    anchors = { center_in = true }, width = W, height = H, radius = theme.radius_small,
    color = "#00000000", border_width = 1, border_color = C.islandBorder,
    old, reveal,
  }
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

--- A window arriving in a preset's manner.
function M.window_motion(preset)
  local loops = {
    macos = { scale = { from = 0.6, to = 1, duration = 520, easing = "out_cubic", alternate = true } },
    snappy = { scale = { from = 0.85, to = 1, duration = 220, easing = "out_quint", alternate = true } },
    smooth = { opacity = { from = 0.1, to = 1, duration = 900, easing = "in_out_sine", alternate = true } },
    springy = { scale = { from = 0.5, to = 1, duration = 600, easing = "out_back", alternate = true } },
  }
  return ui.Rect {
    anchors = { center_in = true }, width = 58, height = 34, radius = 6,
    color = C.islandSurfaceHover, border_width = 1, border_color = C.accent,
    loop = loops[preset.id],
    ui.Rect { x = 0, y = 0, width = 58, height = 7, radius = 6, color = C.islandBorder },
  }
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
  local grid = ui.Grid(cells)
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
