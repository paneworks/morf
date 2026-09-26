-- Settings, the Dock: edge, alignment, size, behaviour and the applications
-- kept on it (DockSection). Icon order is also changed by dragging them on
-- the dock itself. Everything but the main switch is locked while the dock
-- is off.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local controls = require("components.controls")
local setting = require("components.setting")
local kept = require("settings.kept_applications")
local tr = require("services.tr")

local C = theme.color

local M = {}

M.edges = { { id = "bottom", label = tr("Bottom") }, { id = "left", label = tr("Left") }, { id = "right", label = tr("Right") } }
M.alignments = { { id = "start", label = tr("Start") }, { id = "center", label = tr("Middle") }, { id = "end", label = tr("End") } }

--- A schematic screen with the bar along the top and the dock on `edge`,
--- placed by the current alignment.
function M.edge_picture(edge)
  local SW, SH = 96, 54
  local upright = edge ~= "bottom"
  local w, h = upright and 6 or 40, upright and 32 or 6
  local motion = theme.behave("medium")
  return ui.Rect {
    anchors = { center_in = true }, width = SW, height = SH, radius = theme.radius_small,
    color = C.island, border_width = 1, border_color = C.islandBorder,
    ui.Rect { x = (SW - 26) / 2, y = 4, width = 26, height = 4, radius = 2, color = C.islandSurfaceHover },
    ui.Rect {
      width = w, height = h, radius = 3, color = C.accent,
      x = function()
        if edge == "left" then return 4 end
        if edge == "right" then return SW - w - 4 end
        local a = settings.dockAlignment
        if a == "start" then return 4 elseif a == "end" then return SW - w - 4 end
        return (SW - w) / 2
      end,
      y = function()
        if not upright then return SH - h - 4 end
        local a = settings.dockAlignment
        if a == "start" then return 8 elseif a == "end" then return SH - h - 4 end
        return (SH - h) / 2
      end,
      behavior = { x = motion, y = motion },
    },
  }
end

function M.build(page)
  local W = page.width
  local off = function() return not settings.dockEnabled end
  local reason = tr("The dock is off")
  local screens = #(morf.screens or {})

  local edge_tiles = {}
  for _, edge in ipairs(M.edges) do
    edge_tiles[#edge_tiles + 1] = function(tile_w)
      return setting.tile {
        width = tile_w, caption = edge.label,
        selected = function() return settings.dockEdge == edge.id end,
        on_picked = function() settings.set("dockEdge", edge.id) end,
        stage = function() return M.edge_picture(edge.id) end,
      }
    end
  end

  local groups = { direction = "column", gap = 20, align = "start", width = W }
  groups[#groups + 1] = setting.group {
    width = W, title = tr("The dock"),
    note = tr("Your kept and open applications, on one edge of the screen."),
    hint = "An empty dock is not drawn. Icons are reordered by dragging them on the dock itself.",
    setting.switch_row { width = W, label = tr("Show the dock"),
      checked = function() return settings.dockEnabled end,
      on_toggled = function(on) settings.set("dockEnabled", on) end },
    setting.tiles { width = W, label = tr("Edge"), locked = off, reason = reason, tiles = edge_tiles },
    setting.row { width = W, label = tr("Alignment"), locked = off, reason = reason,
      control = controls.segmented { options = M.alignments,
        current = function() return settings.dockAlignment end,
        on_selected = function(id) settings.set("dockAlignment", id) end } },
  }
  groups[#groups + 1] = setting.group {
    width = W, title = tr("Size"),
    note = tr("Everything on the dock scales with the icon size."),
    hint = "Background sets how opaque the capsule behind the icons is.",
    setting.slider { width = W, label = tr("Icon size"), from = 28, to = 72, step = 2, unit = " px",
      locked = off, reason = reason,
      value = function() return settings.dockIconSize end,
      on_moved = function(v) settings.set("dockIconSize", v) end },
    setting.slider { width = W, label = tr("Background"), from = 20, to = 100, step = 5, unit = "%",
      locked = off, reason = reason,
      value = function() return settings.dockOpacity end,
      on_moved = function(v) settings.set("dockOpacity", v) end },
  }
  groups[#groups + 1] = setting.group {
    width = W, title = tr("Behaviour"),
    note = tr("What else the dock shows, and which screens it is on."),
    hint = "The launcher button opens the island's launcher, and open applications appear after a divider.",
    setting.switch_row { width = W, label = tr("Launcher button"), locked = off, reason = reason,
      checked = function() return settings.dockLauncher end,
      on_toggled = function(on) settings.set("dockLauncher", on) end },
    setting.switch_row { width = W, label = tr("Open applications"), locked = off, reason = reason,
      checked = function() return settings.dockRunning end,
      on_toggled = function(on) settings.set("dockRunning", on) end },
    setting.switch_row { width = W, label = tr("On every screen"),
      reading = function()
        return settings.dockEverywhere and tr("One on each, all showing the same") or tr("Only on the screen you are on")
      end,
      locked = function() return off() or screens < 2 end,
      reason = function() return off() and reason or tr("Only one screen is on") end,
      checked = function() return settings.dockEverywhere end,
      on_toggled = function(on) settings.set("dockEverywhere", on) end },
    setting.switch_row { width = W, label = tr("Hide until pointed at"), locked = off, reason = reason,
      checked = function() return settings.dockAutohide end,
      on_toggled = function(on) settings.set("dockAutohide", on) end },
  }
  local kept_rows = kept.rows(W)
  kept_rows.width = W
  kept_rows.title = tr("Kept on it")
  kept_rows.note = tr("These stay whether they are running or not, and they lead the launcher's list too.")
  groups[#groups + 1] = setting.group(kept_rows)
  return ui.Flex(groups)
end

return M
