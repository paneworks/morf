-- The right panel's Power page, behind the battery row's ">": everything
-- about power in one place -- the battery (charge, state, time left, draw),
-- the power profile, the battery's health and how it is charged, and the way
-- to the dashboard's Battery tab for its graphs.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")

local C = theme.color
local M = {}

function M.build(model, w, h)
  local viewport
  local first = model.battery
  local function active()
    local value = model.profile()
    return value == "" and "balanced" or value
  end

  local function card(height, props)
    props.width, props.height, props.radius = w, height, 22
    return kit.card(props)
  end
  local function label(text_value, y)
    return kit.heading { viewport = function() return viewport end, id = "power-heading-" .. text_value:lower():gsub("%s+", "-"), scope = "settings.power", level = "section", x = 18, y = y, text = text_value, font_size = theme.size.small,
      color = function() return C.onSurfaceVariant end }
  end

  -- The battery: its charge large, its state and what that means in time.
  local summary = card(132, {
    id = "power-summary",
    kit.icon(function()
      local b = first()
      if b.status == "Charging" then return "battery_charging_full" end
      local pct = b.capacity or 0
      if pct > 90 then return "battery_full" end
      if pct > 50 then return "battery_5_bar" end
      if pct > 20 then return "battery_3_bar" end
      return "battery_alert"
    end, 40, function() return C.primary end, { x = 18, y = 20, fill = true }),
    kit.text {
      id = "power-percent", x = 70, y = 14, font_size = theme.size.extra, font_weight = 700,
      text = function() return ("%d%%"):format(math.floor((first().capacity or 0) + 0.5)) end,
    },
    kit.subtitle {
      x = 70, y = 56, width = w - 90, elide = "right",
      color = function() return C.onSurfaceVariant end,
      text = model.summary,
    },
    -- The charge, as a bar, the charge limit marked on it.
    kit.surface {
      x = 18, y = 96, width = w - 36, height = 12, radius = 6,
      color = function() return C.surfaceContainerHighest end,
      kit.surface {
        height = 12, radius = 6,
        width = function() return (w - 36) * math.max(0, math.min(1, (first().capacity or 0) / 100)) end,
        color = function() return C.primary end,
        behavior = { width = { duration = 500 } },
      },
      kit.surface {
        width = 2, height = 12,
        x = function() return (w - 36) * (first().charge_limit or 100) / 100 - 1 end,
        visible = function() local l = first().charge_limit return l ~= nil and l < 100 end,
        color = function() return C.onSurface end,
      },
    },
  })

  -- The power profile: three choices, each with what it is for.
  local buttons = {}
  local bw = (w - 36 - 16) / 3
  for _, p in ipairs(model.PROFILES) do
    local function on() return active() == p.id end
    local area
    area = kit.action {
      id = "power-profile-" .. p.id,
      width = bw, height = 78, cursor = "pointer",
      on_clicked = function() model.select(p.id) end,
      kit.surface {
        anchors = { fill = true },
        radius = function() return on() and 14 or 22 end,
        color = function()
          local base = on() and C.primary or C.surfaceContainerHighest
          if area and area.hovered then return base:mix(on() and C.onPrimary or C.onSurface, 0.08) end
          return base
        end,
        behavior = { color = { duration = theme.duration.small }, radius = kit.spring(260, 16) },
      },
      ui.Column {
        anchors = { center_in = true }, gap = 2, align = "center",
        kit.icon(p.icon, 24, function() return on() and C.onPrimary or C.onSurfaceVariant end),
        kit.menu_label { text = p.name, font_size = theme.size.small, font_weight = 600,
          color = function() return on() and C.onPrimary or C.onSurface end },
        kit.subtitle { text = p.hint, font_size = theme.size.small - 3,
          color = function() return on() and C.onPrimary:alpha(0.8) or C.onSurfaceVariant end },
      },
    }
    buttons[#buttons + 1] = area
  end
  local profile = card(136, {
    id = "power-profiles",
    label("Power mode", 14),
    ui.Row { x = 18, y = 40, gap = 8, table.unpack(buttons) },
  })

  -- The battery's health, and how it is charged -- set by the firmware
  -- (and root), shown here.
  local function fact(name, value)
    return ui.Item {
      width = w - 36, height = 26,
      kit.section_label { text = name, color = function() return C.onSurfaceVariant end },
      kit.text { anchors = { right = true }, text = value, font_weight = 500 },
    }
  end
  local health = card(236, {
    id = "power-health",
    label("Battery", 14),
    ui.Column {
      x = 18, y = 40, gap = 4,
      table.unpack((function()
        local facts = {}
        for _, entry in ipairs(model.FACTS) do facts[#facts + 1] = fact(entry.name, entry.value) end
        return facts
      end)()),
    },
  })

  -- The graphs are the dashboard's.
  local more_area
  more_area = kit.action {
    id = "power-open-battery",
    width = w, height = 52, cursor = "pointer",
    on_clicked = model.open_battery,
    kit.surface {
      anchors = { fill = true }, radius = 26,
      color = function()
        local base = C.secondaryContainer
        return (more_area and more_area.hovered) and base:mix(C.onSecondaryContainer, 0.08) or base
      end,
    },
    ui.Row {
      anchors = { center_in = true }, gap = 8, align = "center",
      kit.icon("monitoring", 20, function() return C.onSecondaryContainer end),
      kit.menu_label { text = "Battery graphs and details", font_weight = 500,
        color = function() return C.onSecondaryContainer end },
    },
  }

  viewport = ui.Flickable {
    id = "power-scroll", width = w, height = h, clip = true,
    ui.Column { gap = 12, width = w, summary, profile, health, more_area },
  }
  return viewport
end

return M
