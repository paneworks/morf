-- A small mixer over `morf.audio`: every output with its volume, the
-- applications playing and recording, and a meter of what the default
-- output is playing right now.
--
-- Click an output's name to make it the default, its percentage to mute it,
-- and drag (or scroll) a slider to set its volume. Nothing here names a sound
-- server: `morf.audio` finds whichever one the machine runs.

local morf = require("morf")
local ui = require("morf.ui")
local audio = morf.audio

local W, H = 440, 600
local PAD = 16
local BANDS = 24

morf.surface.width = W
morf.surface.height = H
morf.surface.anchors = { top = true, right = true }
morf.surface.layer = "overlay"
morf.surface.keyboard_focus = "none"

local C = {
  bg = "#12161c", card = "#1b212a", fg = "#e8ecf1", muted = "#8a94a0",
  faint = "#2a323d", on = "#7aa2f7", hot = "#f7768e", meter = "#9ece6a",
}

local function percent(volume)
  return math.floor(volume * 100 + 0.5) .. "%"
end

--- A slider from 0 to 100%: `get` reads the volume, `set` asks for one.
local sliders = 0
local function slider(width, get, set)
  sliders = sliders + 1
  local held = morf.signal("audio.slider." .. sliders, false)
  local function at(x) return math.max(0, math.min(1, x / width)) end
  return ui.Item {
    width = width, height = 18,
    ui.Rect {
      y = 6, width = width, height = 6, radius = 3, color = C.faint,
      ui.Rect {
        height = 6, radius = 3, color = C.on,
        width = function() return math.max(6, math.min(1, get()) * width) end,
      },
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_pressed = function(_, _, x) held:set(true) set(at(x)) end,
      on_dragged = function(_, _, _, _, x) if held:get() then set(at(x)) end end,
      on_released = function() held:set(false) end,
      on_wheel = function(_, _, _, _, _, steps)
        if steps ~= 0 then set(math.max(0, math.min(1, get() - steps * 0.05))) end
      end,
    },
  }
end

local function label(values)
  values.color = values.color or C.fg
  values.font_size = values.font_size or 14
  return ui.Text(values)
end

--- One output: its name (the default is lit), its level, and a slider. The
--- row reads the device live through `audio.device`, so the delegate keeps
--- its nodes — and a drag in progress — while the volume moves under it.
local function output_row(row)
  local id = row.id
  local function device() return audio.device(id) end
  local node = ui.Rect {
    width = W - PAD * 2, height = 58, radius = 10, color = C.card,
    label {
      x = 12, y = 8, width = W - PAD * 2 - 90, elide = "right",
      text = function() local d = device() return d and d.description or "" end,
      color = function() local d = device() return d and d.default and C.on or C.fg end,
    },
    ui.MouseArea {
      x = 0, y = 0, width = W - PAD * 2 - 80, height = 28, cursor = "pointer",
      on_clicked = function() audio.set_default(id) end,
    },
    ui.MouseArea {
      x = W - PAD * 2 - 72, y = 4, width = 64, height = 24, cursor = "pointer",
      on_clicked = function() local d = device() if d then audio.set_mute(id, not d.muted) end end,
      label {
        anchors = { right = true, top = true, right_margin = 4, top_margin = 4 },
        text = function() local d = device() return d and (d.muted and "muted" or percent(d.volume)) or "" end,
        color = function() local d = device() return d and d.muted and C.hot or C.muted end,
      },
    },
    ui.Item {
      x = 12, y = 32,
      slider(W - PAD * 2 - 24,
        function() local d = device() return d and d.volume or 0 end,
        function(value) audio.set_volume(id, value) end),
    },
  }
  return node, function() end
end

--- One application's stream: who, what, and its own slider.
local function stream_row(row)
  local id = row.id
  local function stream() return audio.stream(id) end
  local node = ui.Rect {
    width = W - PAD * 2, height = 58, radius = 10, color = C.card,
    label {
      x = 12, y = 8, width = W - PAD * 2 - 90, elide = "right",
      text = function()
        local s = stream()
        if not s then return "" end
        local title = s.media_name and s.media_name ~= "" and ("  ·  " .. s.media_name) or ""
        return (s.direction == "record" and "● " or "") .. s.app_name .. title
      end,
    },
    ui.MouseArea {
      x = W - PAD * 2 - 72, y = 4, width = 64, height = 24, cursor = "pointer",
      on_clicked = function() local s = stream() if s then audio.set_mute(id, not s.muted) end end,
      label {
        anchors = { right = true, top = true, right_margin = 4, top_margin = 4 },
        text = function() local s = stream() return s and (s.muted and "muted" or percent(s.volume)) or "" end,
        color = function() local s = stream() return s and s.muted and C.hot or C.muted end,
      },
    },
    ui.Item {
      x = 12, y = 32,
      slider(W - PAD * 2 - 24,
        function() local s = stream() return s and s.volume or 0 end,
        function(value) audio.set_volume(id, value) end),
    },
  }
  return node, function() end
end

-- The meter: a peak per side and the spectrum, from the default output.
local peak_left = morf.signal("audio.peak.left", 0)
local peak_right = morf.signal("audio.peak.right", 0)
local bands = {}
for index = 1, BANDS do bands[index] = morf.signal("audio.band." .. index, 0) end

audio.monitor {
  rate_hz = 30,
  bands = BANDS,
  on_level = function(left, right, levels)
    -- A little fall-off, so the bars settle rather than flicker.
    peak_left:set(math.max(left, peak_left:get() * 0.8))
    peak_right:set(math.max(right, peak_right:get() * 0.8))
    for index = 1, BANDS do
      local value = levels and levels[index] or 0
      bands[index]:set(math.max(value, bands[index]:get() * 0.75))
    end
  end,
}

local function peak_bar(y, signal)
  return ui.Rect {
    x = PAD, y = y, width = W - PAD * 2, height = 6, radius = 3, color = C.faint,
    ui.Rect {
      height = 6, radius = 3, color = C.meter,
      width = function() return math.max(0, math.min(1, signal:get())) * (W - PAD * 2) end,
    },
  }
end

local spectrum = {}
local BAND_W = (W - PAD * 2) / BANDS
for index = 1, BANDS do
  spectrum[#spectrum + 1] = ui.Rect {
    x = PAD + (index - 1) * BAND_W + 1, width = BAND_W - 2, radius = 2, color = C.meter,
    height = function() return 2 + bands[index]:get() * 46 end,
    y = function() return 118 - bands[index]:get() * 46 end,
  }
end

ui.Rect {
  color = C.bg, radius = 14,
  width = W, height = H,
  label {
    x = PAD, y = 14, font_size = 18, font_weight = 700,
    text = "Sound",
  },
  label {
    anchors = { right = true, top = true, right_margin = PAD, top_margin = 18 },
    color = C.muted, font_size = 12,
    text = function()
      if not audio.available() then return "no sound server" end
      local sink = audio.default_sink()
      return sink and (sink.description .. "  " .. percent(sink.volume)) or "no output"
    end,
  },
  peak_bar(48, peak_left),
  peak_bar(58, peak_right),
  ui.Item { table.unpack(spectrum) },
  ui.Column {
    x = PAD, y = 132, gap = 8,
    label { color = C.muted, font_size = 12, text = "OUTPUTS" },
    ui.Repeater { as = "column", gap = 8, model = audio.sinks, delegate = output_row },
    label { color = C.muted, font_size = 12, text = "APPLICATIONS" },
    ui.Repeater { as = "column", gap = 8, model = audio.streams, delegate = stream_row },
  },
}
