-- The dashboard's Sound tab: everything about sound in one place.
--
--   Output   the default output's volume, mute, and each of its channels
--            on its own slider (left and right, or channel 1, 2, ...)
--   Devices  every output, the default one chosen; a click makes another
--            the default
--   Apps     what is playing: each app's volume and mute, and chips for
--            the outputs to send it to
--   Input    the default input's volume and mute, and every input to
--            choose from
--
-- All of it is `morf.audio` (docs/IO.md, "Devices and streams"); without a
-- sound server each card says so and nothing is sent.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")

local C = theme.color
local M = {}

M.WIDTH, M.HEIGHT = 840, 560
local GAP, PAD = 12, 16
local COL_W = (M.WIDTH - GAP) / 2
local INNER = COL_W - 2 * PAD
local MAX_CHANNELS = 8

local audio = morf.audio

local function available()
  local ok, yes = pcall(function() return audio and audio.available() end)
  return ok and yes
end

local function sink() return available() and audio.default_sink() or nil end
local function source() return available() and audio.default_source() or nil end

--- A card's title, with an icon button at its right when `button` is given.
local function title(text, button)
  local row = ui.Item {
    x = PAD, y = PAD, width = INNER, height = 30,
    kit.text {
      anchors = { vertical_center = true },
      text = text, font_size = theme.size.larger, font_weight = 600,
    },
  }
  if button then ui.reparent(button, row) end
  return row
end

--- A round icon button for mute: a wash when on.
local function mute_button(id, target, icon_on, icon_off)
  local function muted() local t = target() return t and t.muted end
  local area
  area = ui.MouseArea {
    id = id, width = 36, height = 30, cursor = "pointer",
    anchors = { right = true, vertical_center = true },
    on_clicked = function()
      local t = target()
      if t then audio.set_mute(t.id, not t.muted) end
    end,
    ui.Rect {
      anchors = { fill = true },
      radius = function() return muted() and 10 or 15 end,
      color = function() return muted() and C.errorContainer or C.surfaceContainerHighest end,
      behavior = { color = { duration = theme.duration.small }, radius = kit.spring(260, 16) },
    },
    kit.icon(function() return muted() and icon_on or icon_off end, 20, function()
      return muted() and C.onErrorContainer or C.onSurfaceVariant
    end, { anchors = { center_in = true } }),
  }
  return area
end

local function nothing(text)
  return kit.text {
    anchors = { center_in = true },
    text = text, font_size = theme.size.normal,
    color = function() return C.onSurfaceVariant end,
    visible = function() return not available() end,
  }
end

--- A row to choose a device by: its name, a check when it is the default.
local function device_row(id_prefix, row)
  local area
  local function live() return audio.device(row.id) end
  local function chosen() local d = live() return d and d.default end
  area = ui.MouseArea {
    id = id_prefix .. "-" .. math.floor(row.id),
    width = INNER, height = 40, cursor = "pointer",
    on_clicked = function() audio.set_default(row.id) end,
    ui.Rect {
      anchors = { fill = true }, radius = function() return chosen() and 12 or 20 end,
      color = function()
        if chosen() then return C.secondaryContainer end
        return area and area.hovered and C.onSurface:alpha(0.06) or C.onSurface:alpha(0)
      end,
      behavior = { color = { duration = theme.duration.small }, radius = kit.spring(260, 16) },
    },
    kit.icon(function() return chosen() and "radio_button_checked" or "radio_button_unchecked" end, 20,
      function() return chosen() and C.onSecondaryContainer or C.onSurfaceVariant end,
      { x = 12, anchors = { vertical_center = true } }),
    kit.text {
      x = 44, width = INNER - 56, elide = "right",
      anchors = { vertical_center = true },
      text = row.description or row.name or "?",
      font_size = theme.size.normal,
      color = function() return chosen() and C.onSecondaryContainer or C.onSurface end,
    },
  }
  return area
end

-- ------------------------------------------------------------------ output --

local function output_card()
  local function vol() local s = sink() return s and s.volume or 0 end
  local channels = {}
  for i = 1, MAX_CHANNELS do
    local function count() local s = sink() return s and s.channels or 0 end
    local function name()
      if count() == 2 then return i == 1 and "L" or "R" end
      return tostring(i)
    end
    channels[i] = ui.Row {
      gap = 8, align = "center",
      visible = function() return count() > 1 and i <= count() end,
      kit.text {
        width = 18, text = name, horizontal_alignment = "center",
        font_size = theme.size.small, font_weight = 600,
        color = function() return C.onSurfaceVariant end,
      },
      kit.slider {
        id = "sound-channel-" .. i, width = INNER - 26, height = 26,
        value = function()
          local s = sink()
          return s and s.volumes and s.volumes[i] or 0
        end,
        set = function(v)
          local s = sink()
          if not s then return end
          local list = {}
          for k = 1, s.channels do list[k] = (s.volumes and s.volumes[k]) or s.volume end
          list[i] = v
          audio.set_channel_volumes(s.id, list)
        end,
      },
    }
  end
  return kit.card {
    id = "sound-output",
    width = COL_W, height = 250,
    title("Output", mute_button("sound-output-mute", sink, "volume_off", "volume_up")),
    ui.Column {
      x = PAD, y = PAD + 38, gap = 4,
      visible = available,
      kit.text {
        width = INNER, elide = "right",
        text = function() local s = sink() return s and s.description or "" end,
        font_size = theme.size.small,
        color = function() return C.onSurfaceVariant end,
      },
      kit.slider {
        id = "sound-output-volume", width = INNER,
        value = vol,
        set = function(v) local s = sink() if s then audio.set_volume(s.id, v) end end,
        icon = function()
          local s = sink()
          if not s or s.muted or s.volume <= 0 then return "volume_mute" end
          return s.volume < 0.5 and "volume_down" or "volume_up"
        end,
      },
      ui.Column { gap = 0, table.unpack(channels) },
    },
    nothing("No sound server"),
  }
end

local function devices_card()
  return kit.card {
    id = "sound-devices",
    width = COL_W, height = M.HEIGHT - 250 - GAP,
    clip = true,
    title("Output device"),
    ui.Column {
      x = PAD, y = PAD + 38, gap = 4, width = INNER, height = M.HEIGHT - 250 - GAP - PAD - 38 - PAD,
      visible = available,
      ui.Repeater {
        as = "column", gap = 4,
        model = audio and audio.sinks,
        delegate = function(row) return device_row("sound-sink", row) end,
      },
    },
    nothing("No outputs"),
  }
end

-- -------------------------------------------------------------------- apps --

--- One app playing: its name and what it plays, its volume and mute, and a
--- chip for each output to send it to.
local function app_row(row)
  if row.direction ~= "playback" then return ui.Item { width = 0, height = 0, visible = false } end
  local id = row.id
  local function live() return audio.stream(id) end
  local chips = ui.Repeater {
      as = "row", gap = 6,
      model = audio.sinks,
      delegate = function(device)
        local area
        local function on() local s = live() return s and s.device == device.id end
        area = ui.MouseArea {
          id = "sound-app-" .. math.floor(id) .. "-to-" .. math.floor(device.id),
          width = 104, height = 26, cursor = "pointer",
          on_clicked = function() audio.move_stream(id, device.id) end,
          ui.Rect {
            anchors = { fill = true },
            radius = function() return on() and 8 or 13 end,
            color = function()
              if on() then return C.primary end
              return area and area.hovered and C.surfaceContainerHighest:mix(C.onSurface, 0.08)
                or C.surfaceContainerHighest
            end,
            behavior = { color = { duration = theme.duration.small }, radius = kit.spring(260, 16) },
          },
          kit.text {
            x = 10, width = 84, elide = "right", anchors = { vertical_center = true },
            text = device.description or device.name or "?",
            font_size = theme.size.small,
            color = function() return on() and C.onPrimary or C.onSurfaceVariant end,
          },
        }
        return area
      end,
  }
  local function muted() local s = live() return s and s.muted end
  local mute
  mute = ui.MouseArea {
    id = "sound-app-" .. math.floor(id) .. "-mute",
    width = 30, height = 30, cursor = "pointer",
    anchors = { right = true },
    on_clicked = function() local s = live() if s then audio.set_mute(id, not s.muted) end end,
    kit.icon(function() return muted() and "volume_off" or "volume_up" end, 20, function()
      return muted() and C.error or C.onSurfaceVariant
    end, { anchors = { center_in = true } }),
  }
  return ui.Column {
    gap = 4,
    ui.Item {
      width = INNER, height = 30,
      ui.Column {
        gap = 0, anchors = { vertical_center = true },
        kit.text {
          width = INNER - 40, elide = "right",
          text = row.app_name or row.binary or "App", font_size = theme.size.normal, font_weight = 600,
        },
        kit.text {
          width = INNER - 40, elide = "right",
          text = function() local s = live() return s and s.media_name or "" end,
          font_size = theme.size.small,
          color = function() return C.onSurfaceVariant end,
        },
      },
      mute,
    },
    kit.slider {
      id = "sound-app-" .. math.floor(id) .. "-volume", width = INNER, height = 30,
      value = function() local s = live() return s and s.volume or 0 end,
      set = function(v) audio.set_volume(id, v) end,
    },
    ui.Item { width = INNER, height = 30, clip = true, chips },
    ui.Item { width = INNER, height = 6 },
  }
end

local function apps_card()
  local function playing()
    if not available() then return 0 end
    local n, streams = 0, audio.streams
    for i = 1, streams:len() do
      if streams:get(i).direction == "playback" then n = n + 1 end
    end
    return n
  end
  return kit.card {
    id = "sound-apps",
    width = COL_W, height = 330,
    clip = true,
    title("Apps"),
    ui.Column {
      x = PAD, y = PAD + 38, gap = 6, width = INNER, height = 330 - PAD - 38 - PAD,
      visible = available,
      ui.Repeater {
        as = "column", gap = 6,
        model = audio and audio.streams,
        delegate = app_row,
      },
    },
    kit.text {
      anchors = { center_in = true },
      text = function() return available() and "Nothing playing" or "No sound server" end,
      font_size = theme.size.normal,
      color = function() return C.onSurfaceVariant end,
      visible = function() return playing() == 0 end,
    },
  }
end

-- ------------------------------------------------------------------- input --

local function input_card()
  return kit.card {
    id = "sound-input",
    width = COL_W, height = M.HEIGHT - 330 - GAP,
    clip = true,
    title("Input", mute_button("sound-input-mute", source, "mic_off", "mic")),
    ui.Column {
      x = PAD, y = PAD + 38, gap = 4,
      visible = available,
      kit.slider {
        id = "sound-input-volume", width = INNER, height = 36,
        value = function() local s = source() return s and s.volume or 0 end,
        set = function(v) local s = source() if s then audio.set_volume(s.id, v) end end,
        icon = function() local s = source() return (s and s.muted) and "mic_off" or "mic" end,
      },
      ui.Repeater {
        as = "column", gap = 4,
        model = audio and audio.sources,
        delegate = function(row)
          -- A monitor is an output's echo, not a microphone.
          if tostring(row.name or ""):match("%.monitor$") then
            return ui.Item { width = 0, height = 0, visible = false }
          end
          return device_row("sound-source", row)
        end,
      },
    },
    nothing("No inputs"),
  }
end

function M.build()
  return ui.Item {
    id = "dashboard-sound-tab",
    width = M.WIDTH, height = M.HEIGHT,
    ui.Row {
      gap = GAP,
      ui.Column { gap = GAP, output_card(), devices_card() },
      ui.Column { gap = GAP, apps_card(), input_card() },
    },
  }
end

-- Built as the module loads, with its own instruction budget.
M.page = M.build()

return M
