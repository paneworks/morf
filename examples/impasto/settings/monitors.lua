-- Settings, Displays: the screens, the one picked, the laptop's lid and the
-- night light (MonitorsSection, MonitorCanvas, LidPreview).
--
-- Under Hyprland the page edits the arrangement services/displays.lua keeps
-- per set of connected monitors and pushes as monitor rules: drag a screen
-- on the canvas, mirror or extend, choose the main screen, switch a screen
-- off or on, its scale, rotation, variable refresh, resolution and refresh
-- rate, and forget the arrangement. Changes apply at once, with no
-- confirmation. What is shown is what the compositor reports, so a change
-- appears once it has taken. Elsewhere the screens morf sees are shown,
-- the controls locked, and the page says why. The lid policy is kept in
-- `lidPolicy` (services/lid.lua); the night light is services/night.lua.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local night = require("services.night")
local displays = require("services.displays")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local canvas = require("settings.monitor_canvas")
local tr = require("services.tr")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

-- Hyprland's transform 0-3; the flipped ones (4-7) are left out.
M.rotations = {
  { id = "0", label = tr("None") }, { id = "1", label = "90°" },
  { id = "2", label = "180°" }, { id = "3", label = "270°" },
}

local NOT_HERE = "Only under Hyprland"

--- The screens: `{ key, name, make, model, x, y, width, height, scale,
--- transform, refresh, disabled, vrr, dpms, resolutions }`. Hyprland's
--- (every output, lit or not) when it is there, else the outputs morf
--- sees. Tracks the list in a binding.
function M.monitors()
  local out = {}
  if displays.available() then
    for _, m in ipairs(displays.monitors()) do
      out[#out + 1] = {
        key = displays.key(m), name = m.name, make = m.make or "", model = m.model or "",
        x = m.x or 0, y = m.y or 0, width = m.width or 0, height = m.height or 0,
        scale = m.scale or 1, transform = m.transform or 0, refresh = m.refresh or 0,
        disabled = m.disabled == true, vrr = m.vrr or 0, dpms = m.dpms ~= false,
        resolutions = m.resolutions or {}, focused = m.focused, mirror = m.mirror or "none",
      }
    end
    return out
  end
  local x = 0
  for _, s in ipairs(morf.screens or {}) do
    out[#out + 1] = {
      key = s.description or s.name, name = s.name or "?",
      make = (s.make ~= "Unknown" and s.make) or "", model = (s.model ~= "Unknown" and s.model) or "",
      x = s.x or x, y = s.y or 0, width = s.width or 0, height = s.height or 0,
      scale = s.scale or 1, transform = 0, refresh = 0, disabled = false, vrr = 0, dpms = true,
      resolutions = {},
    }
    x = x + (s.width or 0)
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

--- One resolution in the list: the size, its fastest rate, lit when in use.
local function resolution_line(W, entry, active, locked, on_pick)
  local hovered = controls.signal("displays.resolution", false)
  return ui.Rect {
    width = W, height = 32, radius = theme.radius_small,
    color = function()
      if active then return C.accent() end
      return hovered:get() and C.islandSurfaceHover or "#00000000"
    end,
    behavior = { color = fast() },
    kit.text { anchors = { left = true, left_margin = 10, vertical_center = true },
      text = string.format("%d × %d", entry.width, entry.height), mono = true, size = theme.size.small,
      color = active and C.accentText or C.text },
    kit.text { anchors = { right = true, right_margin = 10, vertical_center = true },
      text = string.format("%g Hz", entry.refreshes[1] or 0), mono = true, size = theme.size.label,
      color = active and C.accentText or C.textMuted },
    setting.hit { hovered = hovered, enabled = function() return not locked end, on_click = on_pick },
  }
end

--- Says why nothing here reaches the compositor, shown only then.
local function unavailable_group(W)
  return setting.group {
    width = W, visible = function() return not displays.available() end,
    setting.row { width = W, label = tr("Not available here"),
      reading = "Arranging screens goes through Hyprland, and this compositor is not Hyprland; they are shown as they are.",
      control = kit.glyph { glyph = "󰅙", size = 13, color = C.textMuted } },
  }
end

--- A part built again whenever `revision()` changes: `build(W)`'s groups
--- in a one-row Repeater whose row is keyed by the revision, so a new
--- revision replaces the row. (A Loader builds once, when it is shown.)
local function rebuilding(W, revision, build)
  local model = morf.list_model({ { key = tostring(revision()) } })
  local seen = nil
  return {
    setting.watch(function()
      local now = tostring(revision())
      if seen ~= nil and now ~= seen then
        morf.timer(1, function() model:replace({ { key = now } }, "key") end, false)
      end
      seen = now
    end),
    ui.Repeater {
      as = "flex", direction = "column", align = "start", width = W, gap = 0,
      model = model,
      delegate = function()
        local children = { direction = "column", gap = 20, align = "start", width = W }
        for _, node in ipairs(build(W)) do children[#children + 1] = node end
        return ui.Flex(children)
      end,
    },
  }
end
M.rebuilding = rebuilding

function M.build(page)
  local chosen = controls.signal("monitors.chosen", "")

  -- Read inside each part's build, so the part is built again when the
  -- compositor reports the screens anew.
  -- The screen picked: the one clicked, else the main one, else the first.
  local function pick(list, key)
    local current
    for _, m in ipairs(list) do if m.key == key then current = m end end
    if not current and displays.available() then
      local primary = displays.primary_name(true)
      for _, m in ipairs(list) do if m.name == primary then current = m end end
    end
    return current or list[1]
  end
  local function snapshot()
    local list = M.monitors()
    local current = pick(list, chosen:get())
    local lit = 0
    for _, m in ipairs(list) do if not m.disabled then lit = lit + 1 end end
    return list, current, lit
  end

  local arrangement = function(W)
    -- Not `snapshot()`: a click picks a screen, and must not build the
    -- canvas again under the pointer.
    local list = M.monitors()
    local editable = displays.available()
    local single = #list < 2
    -- Bindings, not values: the store changing (a drop on the canvas) must
    -- not build the part again; the compositor's answer does.
    local mirrored = function() return editable and displays.mirroring() end
    local picker = {}
    for _, each in ipairs(list) do picker[#picker + 1] = { id = each.key, label = each.name } end
    local group = {
      width = W, title = tr("The screens"),
      note = editable and tr("Drag one to move it, click one to change it.") or "As the compositor has them. Click one to see it.",
      hint = "Arrangements are saved per set of connected monitors, known by the monitor rather than the port, so moving a cable or closing the lid keeps them. They are pushed to Hyprland as monitor rules; its configuration files are never written.",
      setting.block { width = W,
        ui.Item { width = W - 28, height = 230, opacity = function() return mirrored() and 0.5 or 1 end,
          canvas.new { width = W - 28, height = 230, monitors = list,
            editable = function() return editable and not mirrored() end,
            selected = function() local c = pick(list, chosen:get()) return c and c.key or "" end,
            primary = function() return editable and displays.primary_name() or "" end,
            on_picked = function(key) chosen:set(key) end,
            on_arranged = function(places) displays.remember_positions(places) end } } },
    }
    -- A screen that is off has no place on the canvas; a row each, which
    -- picks it.
    for _, m in ipairs(list) do
      if m.disabled then
        group[#group + 1] = setting.row { width = W, label = m.name .. " — off, and still plugged in",
          control = setting.pill { text = tr("Edit"), on_click = function() chosen:set(m.key) page.go("monitors", "screen") end } }
      end
    end
    group[#group + 1] = setting.row { width = W, label = tr("Arrangement"),
      reading = function() return mirrored() and tr("Every screen shows the main one's") or tr("Extended across all of them") end,
      locked = not editable or single,
      reason = not editable and NOT_HERE or tr("Only one screen is plugged in"),
      control = controls.segmented {
        options = { { id = "extend", label = tr("Extend") }, { id = "mirror", label = tr("Mirror") } },
        current = function() return mirrored() and "mirror" or "extend" end,
        on_selected = function(id) displays.remember_mirror(id == "mirror") end } }
    group[#group + 1] = setting.row { width = W, label = tr("The main screen"),
      reading = "Where anything without a screen of its own goes, and what mirroring copies.",
      locked = not editable or single,
      reason = not editable and NOT_HERE or tr("Only one screen is plugged in"),
      control = controls.segmented { options = picker,
        current = function()
          if not editable then return "" end
          local name = displays.primary_name()
          for _, m in ipairs(list) do if m.name == name then return m.key end end
          return ""
        end,
        on_selected = function(key) displays.remember_primary(key) end } }
    return {
      unavailable_group(W),
      setting.group(group),
      setting.group {
        width = W, title = tr("This arrangement"), visible = function() return editable and displays.arranged() end,
        note = "Kept against these screens and no others.",
        setting.row { width = W, label = tr("Forget it"),
          reading = "Hands these screens back to Hyprland's own configuration",
          control = setting.pill { text = tr("Forget"), on_click = function() displays.forget() end } },
      },
    }
  end

  local screen = function(W)
    local list, m, lit = snapshot()
    if not m then return { setting.group { width = W, setting.row { width = W, label = "No screen" } } } end
    local editable = displays.available()
    local off = m.disabled
    local change = function(fields) displays.remember(m.key, fields) end
    local picker = {}
    for _, each in ipairs(list) do picker[#picker + 1] = { id = each.key, label = each.name } end
    local made = (m.make .. " " .. m.model):match("^%s*(.-)%s*$")
    local title = m.name .. (made ~= "" and (" · " .. made) or "")
    local locked_off = not editable or off
    local off_reason = not editable and NOT_HERE or tr("The screen is off")

    local group = setting.group {
      width = W, title = title,
      setting.switch_row { width = W, label = tr("On"),
        reading = off and tr("Off — out of the layout, and still plugged in")
          or (not m.dpms and "Dark — the panel is asleep, and its workspaces are still on it" or ""),
        -- The last lit screen cannot be switched off: there would be no way
        -- back.
        locked = not editable or (lit <= 1 and not off),
        reason = not editable and NOT_HERE or tr("The only screen there is"),
        checked = not off,
        on_toggled = function(on) change({ disabled = not on }) end },
      setting.slider { width = W, label = tr("Scale"), from = 1, to = 3, step = 0.05, decimals = 2, unit = "×",
        locked = locked_off, reason = off_reason,
        value = function() return m.scale end,
        on_moved = function(v) change({ scale = math.floor(v * 20 + 0.5) / 20 }) end },
      setting.row { width = W, label = tr("Rotation"), locked = locked_off, reason = off_reason,
        control = controls.segmented { options = M.rotations, current = tostring(m.transform),
          on_selected = function(id) change({ transform = tonumber(id) }) end } },
      setting.switch_row { width = W, label = tr("Variable refresh"), locked = locked_off, reason = off_reason,
        checked = (m.vrr or 0) ~= 0,
        on_toggled = function(on) change({ vrr = on and 1 or 0 }) end },
    }

    local lines = { direction = "column", gap = 2, align = "start", width = W - 12 }
    local refreshes = {}
    for index, entry in ipairs(m.resolutions) do
      if index > 8 then break end
      local active = entry.width == m.width and entry.height == m.height
      if active then refreshes = entry.refreshes end
      lines[#lines + 1] = resolution_line(W - 12, entry, active, locked_off, function()
        change({ mode = string.format("%dx%d@%.2f", entry.width, entry.height, entry.refreshes[1] or 60) })
      end)
    end
    local pills = { gap = 6, align = "center" }
    for _, rate in ipairs(refreshes) do
      pills[#pills + 1] = setting.pill { text = string.format("%g Hz", rate),
        active = math.abs(m.refresh - rate) < 0.05,
        on_click = function() change({ mode = string.format("%dx%d@%.2f", m.width, m.height, rate) }) end }
    end
    local resolution = setting.group {
      width = W, title = tr("Resolution"),
      note = #m.resolutions > 0 and "What the monitor itself reported, largest first."
        or "The monitor's modes are not reported here.",
      hint = "Only the modes the monitor reports are listed. Choosing a resolution takes its highest refresh rate.",
      setting.block { width = W, padding = 6, visible = #m.resolutions > 0,
        ui.Flex(lines) },
      setting.row { width = W, label = tr("Refresh rate"),
        reading = m.refresh > 0 and string.format("%.2f Hz", m.refresh) or "Not reported here",
        locked = locked_off or #refreshes < 2,
        reason = locked_off and off_reason or tr("The only rate at this resolution"),
        control = #refreshes > 0 and ui.Row(pills)
          or kit.text { text = "—", size = theme.size.small, color = C.textMuted } },
    }
    return {
      unavailable_group(W),
      setting.group {
        width = W, title = tr("Which screen"), visible = #list > 1,
        setting.row { width = W, label = "Editing", reading = made,
          control = controls.segmented { options = picker, current = m.key,
            on_selected = function(id) chosen:set(id) end } },
      },
      group,
      resolution,
    }
  end

  local lid = function(W)
    local list = M.monitors()
    local internal = M.internal(list)
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
    local panel_reading = tr("Not on this machine")
    if internal then
      panel_reading = internal.disabled and (internal.name .. " — off, and still plugged in") or (internal.name .. " — on")
    end
    return {
      setting.group {
        width = W, title = tr("When the lid closes"),
        note = tr("Only applies with another screen connected."),
        hint = "With nothing else connected, closing the lid is left to logind, which suspends. With another screen connected, the laptop's panel can be switched off (under Hyprland, out of the layout, its workspaces moved over) and lit again when the lid opens.",
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
        setting.row { width = W, label = tr("The laptop's panel"), reading = panel_reading,
          control = kit.glyph { glyph = (internal and not internal.disabled) and "󰍹" or "󰶐", size = 13, color = C.textMuted } },
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

  local by_revision = function() return displays.revision() end
  local by_screen = function() return displays.revision() .. "/" .. chosen:get() end
  return setting.parts(page, {
    { id = "arrangement", build = function(W) return rebuilding(W, by_revision, arrangement) end },
    { id = "screen", build = function(W) return rebuilding(W, by_screen, screen) end },
    { id = "lid", build = function(W) return rebuilding(W, by_revision, lid) end },
    { id = "night", build = night_part },
  })
end

return M
