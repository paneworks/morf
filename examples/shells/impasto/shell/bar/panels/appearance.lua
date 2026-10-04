-- The island's appearance panel: the wallpapers and the palettes, each as a
-- strip with the tile in the middle large (AppearancePanel.qml).
--
-- Left and Right slide, Enter applies, Home and End go to the ends, Page Up
-- and Page Down jump five. The palettes are the page below the wallpapers:
-- Down and Up switch, as does opening the panel as `palette`, and the
-- chevron in the footer does it with the pointer. The folder is read again
-- each time the panel opens, so there is no refresh button.
--
-- `morf ipc call appearance` and `morf ipc call palette` toggle it on
-- either page; `appearance_key <left|right|up|down|return|home|end>`
-- presses a key, for testing without a keyboard.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local wallpaper = require("services.wallpaper")
local theme_service = require("services.theme")
local thumbnails = require("services.thumbnails")
local kit = require("components.kit")
local controls = require("components.controls")
local carousel = require("components.carousel")
local palette = require("lib.util.palette")

local C = theme.color
local fast = function() return theme.behave("fast") end

local W, H = 940, 196
local PAD = theme.panel_padding
local INNER_W, INNER_H = W - 2 * PAD, H - 2 * PAD
local GAP = 12
local TILE_W, TILE_H, CENTRE = 160, 100, 1.25
local FOOTER = 18
local STRIP_H = INNER_H - FOOTER - GAP
local DAUB = 12

local KEY = {
  LEFT = 0xff51, UP = 0xff52, RIGHT = 0xff53, DOWN = 0xff54,
  PAGE_UP = 0xff55, PAGE_DOWN = 0xff56, HOME = 0xff50, END = 0xff57,
  RETURN = 0xff0d, KP_ENTER = 0xff8d, ESCAPE = 0xff1b,
}

local on_palette = function() return island.state.open_panel() == "palette" end

local wall_current = morf.signal("impasto.appearance.wallpaper", 0)
local palette_current = morf.signal("impasto.appearance.palette", 0)
local wall_model = morf.list_model({})
local palette_model = morf.list_model({})

-- The check on what is applied, apart from the accent ring, which marks
-- the place in the strip.
local function applied_mark(visible)
  return ui.Rect {
    anchors = { top = true, right = true, margins = 6 }, width = 18, height = 18, radius = 9,
    color = C.accent, visible = visible,
    kit.glyph { anchors = { center_in = true }, glyph = "󰄬", size = 10, color = C.accentText },
  }
end

local function daubs_of(id)
  local keys = { "accent", "green", "yellow", "red", "blue" }
  local out = {}
  if id == "adaptive" then
    local p = theme_service.derived
    local hex = p and palette.to_hex(p)
    for i, key in ipairs(keys) do out[i] = hex and hex[key] or theme.color[key]() end
    return out
  end
  for _, preset in ipairs(palette.presets) do
    if preset.id == id then
      for i, key in ipairs(keys) do out[i] = preset.colors[key] end
    end
  end
  return out
end

local function badge_of(id)
  if id == "adaptive" then return "Read from the wallpaper" end
  for _, preset in ipairs(palette.presets) do
    if preset.id == id then return preset.badge or "" end
  end
  return ""
end

local function name_of(path) return (path:match("([^/]+)$") or path):gsub("%.[^.]+$", ""):gsub("[-_]", " ") end

local function build()
  -- Read the folder again, and open on what is applied.
  local list = wallpaper.scan()
  local wall_rows, at = {}, 0
  for index, path in ipairs(list) do
    wall_rows[index] = { key = path, path = path, index = index - 1 }
    if path == wallpaper.current:get() then at = index - 1 end
  end
  wall_model:replace(wall_rows, "key")
  wall_current:set(at)
  local themes = theme_service.available()
  local palette_rows, active = {}, 0
  for index, entry in ipairs(themes) do
    palette_rows[index] = { key = entry.id, id = entry.id, name = entry.name, index = index - 1 }
    if entry.id == theme_service.active_id:get() then active = index - 1 end
  end
  palette_model:replace(palette_rows, "key")
  palette_current:set(active)

  local walls = carousel.new {
    width = INNER_W, height = STRIP_H, tile_width = TILE_W, tile_height = TILE_H,
    centre_scale = CENTRE, gap = GAP, reach = 4,
    model = wall_model, current = wall_current,
    on_activated = function(index) local row = wall_rows[index + 1] if row then wallpaper.apply(row.path) end end,
    tile = function(row, index, centred, distance)
      return ui.ClipRect {
        anchors = { fill = true }, radius = theme.radius_medium, color = C.islandSurface,
        border_width = function() return centred() and 2 or 1 end,
        border_color = function() return centred() and C.accent() or C.islandBorder end,
        behavior = { border_color = fast() },
        -- Only tiles near the middle ask for their picture.
        ui.Image {
          anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
          source = function()
            if math.abs(distance()) > 8 then return "" end
            return thumbnails.of(row.path, TILE_W * 2, TILE_H * 2)
          end,
        },
        applied_mark(function() return wallpaper.current:get() == row.path end),
      }
    end,
  }
  local palettes = carousel.new {
    width = INNER_W, height = STRIP_H, tile_width = TILE_W, tile_height = TILE_H,
    centre_scale = CENTRE, gap = GAP, reach = 4,
    model = palette_model, current = palette_current,
    on_activated = function(index)
      local row = palette_rows[index + 1]
      if row then theme_service.set_theme(row.id) end
    end,
    tile = function(row, index, centred, distance, hovered)
      local daubs = { gap = 6, align = "center" }
      for i = 1, 5 do
        daubs[i] = ui.Rect {
          width = DAUB, height = DAUB, radius = DAUB / 2, border_width = 1, border_color = C.hairline,
          color = function()
            if row.id == "adaptive" then theme_service.active_id:get() end
            return daubs_of(row.id)[i] or C.surface()
          end,
          behavior = { color = { duration = 260 } },
        }
      end
      return ui.Rect {
        anchors = { fill = true }, radius = theme.radius_medium,
        color = function() return hovered:get() and C.islandSurfaceHover or C.islandSurface end,
        border_width = function() return centred() and 2 or 1 end,
        border_color = function() return centred() and C.accent() or C.islandBorder end,
        behavior = { color = fast(), border_color = fast() },
        ui.Column {
          anchors = { center_in = true }, gap = 10, align = "center",
          ui.Row(daubs),
          kit.text { text = row.id == "adaptive" and "Adaptive" or row.name, size = theme.size.small,
            weight = function() return centred() and 600 or 400 end,
            color = function() return centred() and C.text() or C.textMuted() end },
        },
        applied_mark(function() return theme_service.active_id:get() == row.id end),
      }
    end,
  }

  local strip = function() return on_palette() and palettes or walls end
  local function apply()
    if on_palette() then
      local row = palette_rows[palette_current:get() + 1]
      if row then theme_service.set_theme(row.id) end
    else
      local row = wall_rows[wall_current:get() + 1]
      if row then wallpaper.apply(row.path) end
    end
  end
  local function turn(page) island.open(page == "palette" and "palette" or "appearance") end

  local function key(keysym)
    local s = strip()
    local current = on_palette() and palette_current or wall_current
    if keysym == KEY.LEFT then s.step(-1)
    elseif keysym == KEY.RIGHT then s.step(1)
    elseif keysym == KEY.DOWN then turn("palette")
    elseif keysym == KEY.UP then turn("wallpaper")
    elseif keysym == KEY.RETURN or keysym == KEY.KP_ENTER then apply()
    elseif keysym == KEY.HOME then s.go_to(0)
    elseif keysym == KEY.END then s.go_to(s.count() - 1)
    elseif keysym == KEY.PAGE_UP then s.step(-5)
    elseif keysym == KEY.PAGE_DOWN then s.step(5)
    elseif keysym == KEY.ESCAPE then island.close()
    end
    local _ = current
  end
  morf.ipc.appearance_key = function(name)
    local names = { left = KEY.LEFT, right = KEY.RIGHT, up = KEY.UP, down = KEY.DOWN,
      ["return"] = KEY.RETURN, home = KEY.HOME, ["end"] = KEY.END }
    if not names[name or ""] then return "unknown key" end
    key(names[name])
    return on_palette() and ("palette " .. palette_current:get()) or ("wallpaper " .. wall_current:get())
  end

  local motion = theme.behave("morph")
  local door_hover = controls.signal("appearance.door", false)
  return ui.MouseArea {
    width = INNER_W, height = INNER_H,
    focus = true,
    on_key_pressed = key,
    -- Both strips live here, the hidden one parked a strip's height away,
    -- so turning the page slides them past each other.
    ui.ClipRect {
      x = 0, y = 0, width = INNER_W, height = STRIP_H, color = "#00000000",
      ui.Item {
        width = INNER_W, height = STRIP_H,
        y = function() return on_palette() and -STRIP_H or 0 end,
        opacity = function() return on_palette() and 0 or 1 end,
        behavior = { y = motion, opacity = motion },
        walls.node,
      },
      ui.Item {
        width = INNER_W, height = STRIP_H,
        y = function() return on_palette() and 0 or STRIP_H end,
        opacity = function() return on_palette() and 1 or 0 end,
        behavior = { y = motion, opacity = motion },
        palettes.node,
      },
    },
    ui.Item {
      x = 0, y = STRIP_H + GAP, width = INNER_W, height = FOOTER,
      kit.text {
        anchors = { left = true, vertical_center = true }, width = INNER_W - 120, elide = "right",
        size = theme.size.small, color = C.textMuted,
        text = function()
          if on_palette() then
            local row = palette_rows[palette_current:get() + 1]
            return row and badge_of(row.id) or ""
          end
          local row = wall_rows[wall_current:get() + 1]
          return row and name_of(row.path) or ""
        end,
      },
      ui.Row {
        anchors = { right = true, vertical_center = true }, gap = 12, align = "center",
        kit.text {
          size = theme.size.small, color = C.textMuted,
          text = function()
            local s = strip()
            if s.count() == 0 then return on_palette() and "" or "No pictures in the folder" end
            local current = on_palette() and palette_current or wall_current
            return (current:get() + 1) .. "/" .. s.count()
          end,
        },
        ui.Item {
          width = 16, height = 16,
          kit.glyph {
            anchors = { center_in = true }, size = theme.size.regular,
            glyph = function() return on_palette() and "󰅃" or "󰅀" end,
            color = function() return door_hover:get() and C.text() or C.textMuted() end,
            behavior = { color = fast() },
          },
          controls.hit { anchors = { fill = true, margins = -6 }, hovered = door_hover,
            on_click = function() turn(on_palette() and "wallpaper" or "palette") end },
        },
      },
    },
  }
end

local panel = {
  size = function() return W, H end,
  build = build,
}
island.register("appearance", panel)
island.register("palette", panel)

morf.ipc.appearance = function()
  island.toggle("appearance")
  return island.state.open_panel()
end
morf.ipc.palette = function()
  island.toggle("palette")
  return island.state.open_panel()
end
