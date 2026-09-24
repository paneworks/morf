-- The list behind the Wi-Fi tile: NetworkDetail.qml.
--
-- A known or open network connects on click, the connected one disconnects,
-- and a new secured one opens a password field in place. Registered as the
-- "wifi" panel (the tile's chevron) and used again as the network module's
-- detail, where there is no back arrow because there is nothing to go back
-- to.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local controls = require("components.controls")
local network = require("services.network")

local C = theme.color
local M = {}

local WIDTH, HEIGHT = 420, 500

local function header(values, width)
  local left = { anchors = { left = true, vertical_center = true }, gap = 10, align = "center" }
  if values.backable then
    left[#left + 1] = controls.icon_button { icon = "󰅁", icon_size = 14, on_click = values.on_back }
  end
  left[#left + 1] = ui.Column {
    gap = 1,
    kit.text { text = values.title, size = theme.size.medium, weight = 600 },
    kit.text { text = values.subtitle, size = theme.size.label, color = C.textMuted },
  }
  local right = { anchors = { right = true, vertical_center = true }, gap = 14, align = "center" }
  if values.link then right[#right + 1] = values.link end
  if values.switch then right[#right + 1] = values.switch end
  return ui.Item {
    width = width, height = 32,
    ui.Row(left),
    ui.Row(right),
  }
end
M.header = header

--- One network: signal, name, a second line, a lock or a tick; the
--- password row below it when it is opened.
local function entry(row, width, expanded)
  local ssid = controls.signal("wifi.ssid", row.ssid or "")
  local strength = controls.signal("wifi.strength", tonumber(row.strength) or 0)
  local secure = controls.signal("wifi.secure", row.secure == true)
  local known = controls.signal("wifi.known", row.known == true)
  local active = controls.signal("wifi.active", row.in_use == true)
  local hovered = controls.signal("wifi.hover", false)
  local current = row
  local open = function() return expanded:get() == ssid:get() and ssid:get() ~= "" end
  local busy = function() return network.busy_ssid:get() == ssid:get() end

  local password
  password = ui.TextInput {
    anchors = { left = true, right = true, left_margin = 10, right_margin = 10, vertical_center = true },
    height = 30, vertical_alignment = "center",
    password = true, placeholder = "Password", placeholder_color = C.textMuted,
    font_family = function() return theme.font() end, font_size = theme.size.small,
    color = C.text, selection_color = C.accent, selected_text_color = C.accentText,
    caret_color = C.accent,
    focus = function() return open() end,
    on_accepted = function(text)
      network.connect(current, text)
      expanded:set("")
    end,
    on_escape = function() expanded:set("") end,
  }

  local node = ui.ClipRect {
    width = width,
    height = function() return open() and 88 or 48 end,
    radius = theme.radius_medium,
    color = function()
      return (active:get() or hovered:get() or open()) and C.islandSurfaceHover or C.islandSurface
    end,
    border_width = 1,
    border_color = function() return active:get() and C.accent() or C.islandBorder end,
    behavior = { height = theme.behave("fast"), color = theme.behave("fast") },
    -- The row's own click, above the password row's height when open.
    ui.MouseArea {
      x = 0, y = 0, width = width, height = 48, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function()
        if active:get() then
          network.disconnect(ssid:get())
        elseif known:get() or not secure:get() then
          network.connect(current)
        else
          expanded:set(open() and "" or ssid:get())
        end
      end,
    },
    ui.Row {
      x = 10, y = 10, height = 28, gap = 10, align = "center",
      kit.glyph {
        glyph = function() return network.strength_icon(strength:get()) end, size = 15, width = 18,
        color = function() return active:get() and C.accent() or C.textMuted() end,
      },
      ui.Column {
        gap = 0,
        kit.text {
          text = function() return ssid:get() end, size = theme.size.small,
          width = width - 20 - 18 - 10 - 30, elide = "right",
          weight = function() return active:get() and 600 or 400 end,
          color = function() return active:get() and C.accent() or C.text() end,
        },
        kit.text {
          text = function()
            if busy() then return "Working…" end
            if active:get() then return "Connected" end
            local bits = { string.format("%d%%", math.floor(strength:get() + 0.5)) }
            if secure:get() then bits[#bits + 1] = "secured" end
            if known:get() then bits[#bits + 1] = "saved" end
            return table.concat(bits, " · ")
          end,
          size = 9, color = C.textMuted, width = width - 20 - 18 - 10 - 30, elide = "right",
        },
      },
    },
    kit.glyph {
      x = width - 30, y = 14, width = 20,
      glyph = function() return active:get() and "󰄬" or "󰌾" end,
      size = function() return active:get() and 13 or 11 end,
      color = function() return active:get() and C.accent() or C.textMuted() end,
      visible = function() return active:get() or secure:get() end,
    },
    -- Only drawn once the row has opened for it.
    ui.Row {
      x = 10, y = 48, gap = 8, align = "center",
      visible = open,
      ui.Rect {
        width = width - 20 - 8 - 80, height = 30, radius = theme.radius_small,
        color = C.island, border_width = 1,
        border_color = function() return open() and C.accent() or C.islandBorder end,
        password,
      },
      controls.pill { text = "Connect", active = true, width = 80, on_click = function()
        network.connect(current, password.text or "")
        expanded:set("")
      end },
    },
  }
  return node, function(next)
    current = next
    ssid:set(next.ssid or "")
    strength:set(tonumber(next.strength) or 0)
    secure:set(next.secure == true)
    known:set(next.known == true)
    active:set(next.in_use == true)
  end
end

--- The list, sized for `width` x `height`. `backable` shows the back arrow,
--- which calls `on_back`.
function M.build(options)
  options = options or {}
  local width = options.width or (WIDTH - 2 * theme.panel_padding)
  local height = options.height or (HEIGHT - 2 * theme.panel_padding)
  local expanded = controls.signal("wifi.expanded", "")
  -- What NetworkManager already knows; no sweep of the band.
  network.scan(false)
  local list = ui.Repeater {
    as = "column", gap = 4,
    model = network.networks,
    delegate = function(row) return entry(row, width, expanded) end,
  }
  return ui.Column {
    gap = 12,
    header({
      backable = options.backable,
      on_back = options.on_back,
      title = "Wi-Fi",
      subtitle = function()
        if not network.radio_on() then return "Radio off" end
        return network.scanning:get() and "Scanning…" or network.connection_name()
      end,
      link = controls.link { text = "Rescan", visible = network.radio_on,
        enabled = function() return not network.scanning:get() end,
        on_click = function() network.scan(true) end },
      switch = controls.switch { checked = network.radio_on,
        on_toggled = function(on) network.set_wifi(on) end },
    }, width),
    ui.ClipRect {
      width = width, height = height - 32 - 12, color = "#00000000",
      kit.text {
        anchors = { center_in = true },
        visible = function() return (list.layout_height or 0) < 1 end,
        text = function()
          if not network.radio_on() then return "Turn Wi-Fi on to see what is around" end
          return network.scanning:get() and "Looking for networks…" or "Nothing in range"
        end,
        size = theme.size.small, color = C.textMuted,
      },
      list,
    },
  }
end

island.register("wifi", {
  size = function() return WIDTH, HEIGHT end,
  build = function()
    return M.build { backable = true, on_back = function() island.open("controls") end }
  end,
})

return M
