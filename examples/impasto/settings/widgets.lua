-- Settings, Desktop: what the modules themselves are told (the weather's
-- place, whose GitHub graph, how the pet and the notes are drawn), and the
-- look every desktop widget follows unless it has its own (WidgetsSection).
-- Widgets are placed, resized and styled on the desktop itself; this page
-- only hands over to that mode, when the desktop has one.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local pets = require("services.pets")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local swatch = require("components.swatch")

local C = theme.color

local M = {}

M.themes = { { id = "modern", label = "Modern" }, { id = "analogue", label = "Analogue" } }
M.styles = {
  { id = "capsule", label = "Capsule" }, { id = "accent", label = "Accent" },
  { id = "outline", label = "Outline" }, { id = "bare", label = "No capsule" },
}

local function note_of(list, id)
  for _, entry in ipairs(list) do if entry.id == id then return entry.note or "" end end
  return ""
end

--- The desktop's own arranging mode, when a desktop is running here.
local function desktop()
  local ok, service = pcall(require, "services.desktop")
  if ok and type(service) == "table" and type(service.edit) == "function" then return service end
  return nil
end

--- A widget face in miniature: a figure over its caption, or a dial.
local function face_picture(id)
  if id == "analogue" then
    return ui.Item {
      anchors = { center_in = true }, width = 44, height = 44,
      ui.Rect { anchors = { fill = true }, radius = 22, color = C.island, border_width = 1,
        border_color = C.islandBorder },
      ui.Rect { x = 21, y = 9, width = 2, height = 14, radius = 1, color = C.text,
        rotation = 0, transform_origin_y = 1 },
      ui.Rect { x = 21, y = 13, width = 2, height = 10, radius = 1, color = C.accent,
        rotation = 120, transform_origin_y = 1 },
      ui.Rect { x = 20, y = 20, width = 4, height = 4, radius = 2, color = C.text },
    }
  end
  return ui.Column {
    anchors = { center_in = true }, gap = 0, align = "center",
    kit.text { text = "09:41", size = 20, weight = 600 },
    kit.text { text = "Thursday", size = theme.size.label, color = C.textMuted },
  }
end

local function modules_part(W)
  local pet_tiles = {}
  for _, style in ipairs(pets.styles) do
    pet_tiles[#pet_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 78, caption = style.label,
        selected = function() return settings.petStyle == style.id end,
        on_picked = function() settings.set("petStyle", style.id) end,
        stage = function(hovered)
          -- An egg is an egg in three of the four styles, so the tiles
          -- draw the creature inside it.
          local face = require("pets.face")
          return ui.Item {
            anchors = { center_in = true }, width = 58, height = 58,
            face.new {
              size = 58, lively = true,
              style = function() return style.id end,
              record = function()
                local out = {}
                for k, v in pairs(pets.pet() or {}) do out[k] = v end
                out.hatchedAt = 1
                return out
              end,
            },
          }
        end,
      }
    end
  end
  return {
    setting.group {
      width = W, title = "Weather", note = "A city, a postcode or an airport code.",
      hint = "Left empty, wttr.in guesses from your connection's address, which can be far off.",
      setting.field { width = W, label = "Location", placeholder = "Wherever the request comes from",
        value = settings.weatherPlace,
        on_edited = function(text) settings.set("weatherPlace", (text:match("^%s*(.-)%s*$"))) end },
    },
    setting.group {
      width = W, title = "GitHub", note = "Whose public contribution graph to draw.",
      hint = "The graph is read from the public profile page, so no token or account is needed.",
      setting.field { width = W, label = "Username", placeholder = "Nobody yet",
        value = settings.githubUser,
        on_edited = function(text)
          settings.set("githubUser", (text:match("^%s*@?(.-)%s*$")))
        end },
    },
    setting.group {
      width = W, title = "Pet", note = "How the creature is drawn, wherever it is drawn.",
      hint = "The species decides the colour and what the creature is; the style decides how it is drawn.",
      setting.tiles { width = W, label = "Style", tiles = pet_tiles,
        reading = function() return note_of(pets.styles, settings.petStyle) end },
    },
    setting.group {
      width = W, title = "Notes",
      note = "A note on the wallpaper is written the same way as one in the panel.",
      setting.switch_row { width = W, label = "Handwriting",
        reading = function() return settings.notesHandwriting and "The signature's script" or "The interface face" end,
        checked = function() return settings.notesHandwriting end,
        on_toggled = function(on) settings.set("notesHandwriting", on) end },
      setting.switch_row { width = W, label = "Edges only on an empty workspace",
        reading = function() return settings.deckOnEmpty and "Gone while a window is open" or "Over the windows" end,
        checked = function() return settings.deckOnEmpty end,
        on_toggled = function(on) settings.set("deckOnEmpty", on) end },
    },
    setting.group {
      width = W, title = "Spectrum",
      note = "Sound bars from whatever is playing, on the grid or along an edge.",
      setting.switch_row { width = W, label = "Only on an empty workspace",
        reading = function() return settings.spectrumOnEmpty and "Gone while a window is open" or "Under the windows" end,
        checked = function() return settings.spectrumOnEmpty end,
        on_toggled = function(on) settings.set("spectrumOnEmpty", on) end },
    },
  }
end

local function widgets_part(W, page)
  local face_tiles, style_tiles = {}, {}
  for _, entry in ipairs(M.themes) do
    face_tiles[#face_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 64, caption = entry.label,
        selected = function() return settings.desktopTheme == entry.id end,
        on_picked = function() settings.set("desktopTheme", entry.id) end,
        stage = function() return face_picture(entry.id) end,
      }
    end
  end
  for _, entry in ipairs(M.styles) do
    style_tiles[#style_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 56, caption = entry.label,
        selected = function() return settings.desktopStyle == entry.id end,
        on_picked = function() settings.set("desktopStyle", entry.id) end,
        stage = function() return swatch.style { anchors = { center_in = true }, style = entry.id, factor = 1.6 } end,
      }
    end
  end
  local placed = function()
    local count = 0
    for _, row in ipairs(settings.desktopWidgets or {}) do
      if type(row) == "table" and not row.edge then count = count + 1 end
    end
    return count
  end
  return {
    setting.group {
      width = W, title = "The desktop", note = "Widgets are arranged directly on the wallpaper.",
      hint = "Arranging brings the widgets in front of the windows, with a card of every module. This window closes so the desktop can be seen.",
      setting.row { width = W, label = "Arrange the desktop",
        locked = function() return desktop() == nil end,
        reason = "The desktop's widgets are not running in this shell",
        reading = function()
          local n = placed()
          return n > 0 and (n .. " on the wallpaper") or "Nothing on the wallpaper yet"
        end,
        control = controls.pill { text = "Edit", height = 30, active = true, on_click = function()
          local d = desktop()
          if d then d.edit(true) page.close() end
        end } },
    },
    setting.group {
      width = W, title = "Look",
      note = "Every widget follows these unless it was given a look of its own.",
      hint = "While arranging, click a widget to override these for it alone.",
      setting.tiles { width = W, label = "Face", tiles = face_tiles },
      setting.tiles { width = W, label = "Style", tiles = style_tiles },
      setting.slider { width = W, label = "Background", from = 20, to = 100, step = 5, unit = "%",
        value = function() return settings.desktopOpacity end,
        on_moved = function(v) settings.set("desktopOpacity", v) end },
    },
  }
end

function M.build(page)
  return setting.parts(page, {
    { id = "modules", build = modules_part },
    { id = "widgets", build = function(W) return widgets_part(W, page) end },
  })
end

return M
