-- Settings, Displays: the screens, the one picked, the laptop's lid and the
-- night light (MonitorsSection, MonitorCanvas, LidPreview).
--
-- Read-only where the original wrote Hyprland: the arrangement, modes,
-- scale and rotation are shown as the compositor reports them (Hyprland's
-- list through lib/hyprland when it is there, else the outputs morf sees),
-- and saving a layout is left to the compositor's own configuration. The
-- lid policy is kept in impasto's settings (`lidPolicy`) for whoever acts
-- on the lid; the night light is `services/night.lua`.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local night = require("services.night")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local canvas = require("settings.monitor_canvas")
local tr = require("services.tr")

local C = theme.color

local M = {}

local ROTATIONS = { [0] = "None", [1] = "90°", [2] = "180°", [3] = "270°" }

--- The screens: `{ key, name, make, model, x, y, width, height, scale,
--- transform, refresh, disabled }`.
function M.monitors()
  local out = {}
  local ok, workspaces = pcall(require, "services.workspaces")
  local rows = ok and workspaces.monitors() or {}
  for _, m in ipairs(rows or {}) do
    out[#out + 1] = {
      key = (m.description ~= "" and m.description) or m.name, name = m.name,
      make = m.make or "", model = m.model or "", x = m.x or 0, y = m.y or 0,
      width = m.width or 0, height = m.height or 0, scale = m.scale or 1,
      transform = m.transform or 0, refresh = m.refresh_rate or 0, disabled = m.disabled,
    }
  end
  if #out == 0 then
    local x = 0
    for _, s in ipairs(morf.screens or {}) do
      out[#out + 1] = {
        key = s.description or s.name, name = s.name or "?", make = s.make or "", model = s.model or "",
        x = s.x or x, y = s.y or 0, width = s.width or 0, height = s.height or 0,
        scale = s.scale or 1, transform = 0, refresh = 0, disabled = false,
      }
      x = x + (s.width or 0)
    end
  end
  return out
end

--- The laptop's own panel, by its connector name (eDP, LVDS, DSI).
function M.internal(list)
  for _, m in ipairs(list) do
    if m.name:match("^eDP") or m.name:match("^LVDS") or m.name:match("^DSI") then return m end
  end
  return nil
end

--- A laptop, its screen lit or dark behind the lid, and a screen beside it.
local function lid_picture(lit, external)
  return ui.Item {
    anchors = { center_in = true }, width = 100, height = 50,
    ui.Rect { x = 4, y = 6, width = 44, height = 30, radius = 3, color = C.island,
      border_width = 1, border_color = C.islandBorder,
      ui.Rect { x = 3, y = 3, width = 38, height = 24, radius = 2,
        color = lit and C.accent or C.islandSurface, opacity = lit and 0.8 or 1 } },
    ui.Rect { x = 0, y = 37, width = 52, height = 5, radius = 2, color = C.islandBorder },
    ui.Item {
      visible = external,
      ui.Rect { x = 58, y = 2, width = 40, height = 28, radius = 3, color = C.island,
        border_width = 1, border_color = C.islandBorder,
        ui.Rect { x = 3, y = 3, width = 34, height = 22, radius = 2, color = C.accent } },
      ui.Rect { x = 74, y = 31, width = 8, height = 8, color = C.islandBorder },
      ui.Rect { x = 68, y = 39, width = 20, height = 3, radius = 1, color = C.islandBorder },
    },
  }
end

function M.build(page)
  local list = M.monitors()
  local chosen = controls.signal("monitors.chosen", "")
  local current = function()
    local key = chosen:get()
    for _, m in ipairs(list) do if m.key == key then return m end end
    return list[1]
  end
  local single = #list < 2
  local internal = M.internal(list)

  local arrangement = function(W)
    return {
      setting.group {
        width = W, title = tr("The screens"),
        note = "As the compositor has them. Click one to see it.",
        hint = "Arranging screens is the compositor's own configuration, which this shell never writes.",
        setting.block { width = W,
          canvas.new { width = W - 28, height = 220, monitors = list,
            selected = function() local m = current() return m and m.key or "" end,
            on_picked = function(key) chosen:set(key) end } },
        setting.row { width = W, label = tr("Arrangement"),
          reading = single and "One screen" or tr("Extended across all of them"),
          control = kit.text { text = #list .. (#list == 1 and " screen" or " screens"),
            size = theme.size.small, color = C.textMuted } },
      },
    }
  end

  local screen = function(W)
    local m = current()
    if not m then return { setting.group { width = W, setting.row { width = W, label = "No screen" } } } end
    local picker = {}
    for _, each in ipairs(list) do picker[#picker + 1] = { id = each.key, label = each.name } end
    local title = m.name
    local made = (m.make .. " " .. m.model):match("^%s*(.-)%s*$")
    if made ~= "" then title = title .. " · " .. made end
    return {
      setting.group {
        width = W, title = tr("Which screen"), visible = not single,
        setting.row { width = W, label = "Showing", reading = made,
          control = controls.segmented { options = picker,
            current = function() local c = current() return c and c.key or "" end,
            on_selected = function(id) chosen:set(id) end } },
      },
      setting.group {
        width = W, title = function() local c = current() return c and c.name or "" end,
        note = "As the compositor reports it.",
        setting.row { width = W, label = tr("Resolution"),
          reading = function() local c = current() return string.format("%d × %d", c.width, c.height) end },
        setting.row { width = W, label = tr("Refresh rate"),
          reading = function()
            local c = current()
            return c.refresh > 0 and string.format("%.2f Hz", c.refresh) or "Not reported here"
          end },
        setting.row { width = W, label = tr("Scale"),
          reading = function() return string.format("%.2f×", current().scale) end },
        setting.row { width = W, label = tr("Rotation"),
          reading = function() return ROTATIONS[current().transform] or tostring(current().transform) end },
        setting.row { width = W, label = "Changing it",
          reading = "In the compositor's own configuration, which the shell leaves alone" },
      },
    }
  end

  local lid = function(W)
    local tiles = {}
    for _, choice in ipairs {
      { id = "off", label = tr("Switch it off"), lit = false, external = true },
      { id = "keep", label = tr("Leave it on"), lit = true, external = true },
      { id = "system", label = tr("The system decides"), lit = true, external = false },
    } do
      tiles[#tiles + 1] = function(tw)
        return setting.tile {
          width = tw, caption = choice.label,
          selected = function() return settings.lidPolicy == choice.id end,
          on_picked = function() settings.set("lidPolicy", choice.id) end,
          stage = function() return lid_picture(choice.lit, choice.external) end,
        }
      end
    end
    local others = {}
    for _, m in ipairs(list) do if m ~= internal then others[#others + 1] = m.name end end
    return {
      setting.group {
        width = W, title = tr("When the lid closes"),
        note = tr("Only applies with another screen connected."),
        hint = "Kept in the shell's settings. With nothing else connected, closing the lid is left to logind, which suspends.",
        setting.tiles { width = W, label = tr("The laptop's screen"), tiles = tiles,
          locked = internal == nil, reason = tr("No laptop panel on this machine"),
          reading = function()
            local p = settings.lidPolicy
            if p == "off" then return tr("Switched off, and its workspaces move over") end
            if p == "keep" then return tr("Left on behind the lid") end
            return tr("Left to the system")
          end },
      },
      setting.group {
        width = W, title = tr("Right now"), note = "The current state, as the shell sees it.",
        setting.row { width = W, label = tr("The laptop's panel"),
          reading = internal and (internal.name .. " — on") or tr("Not on this machine"),
          control = kit.glyph { glyph = internal and "󰍹" or "󰶐", size = 13, color = C.textMuted } },
        setting.row { width = W, label = tr("Other screens"),
          reading = #others == 0 and tr("None — the system handles the lid") or table.concat(others, ", "),
          control = kit.text { text = tostring(#others), size = theme.size.small, color = C.textMuted } },
      },
    }
  end

  local night_part = function(W)
    return {
      setting.group {
        width = W, title = tr("Night light"), note = tr("Warmer colours for the evening."),
        hint = "It adjusts the gamma ramp with hyprsunset, so screenshots keep their colours. There is no schedule: it stays on until you turn it off.",
        setting.switch_row { width = W, label = tr("Warm the screen"),
          reading = function()
            if not night.available() then return tr("Needs hyprsunset, which is not installed") end
            return settings.nightLight and (settings.nightTemperature .. " K") or tr("Off")
          end,
          alarm = function() return not night.available() end,
          checked = function() return settings.nightLight end,
          on_toggled = function(on) night.set(on) end },
        setting.slider { width = W, label = tr("Colour temperature"), from = night.WARMEST, to = night.COOLEST,
          step = 100, unit = " K", figure_width = 70,
          value = function() return settings.nightTemperature end,
          on_moved = function(v) night.set_temperature(v) end },
        setting.block { width = W,
          ui.Rect { width = W - 28, height = 24, radius = theme.radius_small,
            gradient = { angle = 90, stops = { "#ff8a3c", "#ffd6a0", "#ffffff", "#cfe3ff" } },
            ui.Rect {
              y = -3, width = 4, height = 30, radius = 2, color = C.text,
              x = function()
                local f = (settings.nightTemperature - night.WARMEST) / (night.COOLEST - night.WARMEST)
                return math.max(0, math.min(1, f)) * (W - 28) - 2
              end,
            } },
        },
      },
    }
  end

  return setting.parts(page, {
    { id = "arrangement", build = arrangement },
    { id = "screen", build = screen },
    { id = "lid", build = lid },
    { id = "night", build = night_part },
  })
end

return M
