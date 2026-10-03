-- Gallery samples for the audio instruments: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local ui = require("morf.ui")
local morf = require("morf")

local function bands(n)
  local t = {}
  for i = 1, n do t[i] = .25 + .6 * math.abs(math.sin(i * 1.7)) * (1 - i / (n * 1.6)) end
  return t
end

return {
  automation_lane = function(kit)
    return kit.automation_lane { width = 260, height = 190, label = "Cutoff", length = 8, position = .42,
      points = { { 0, .2 }, { .18, .75 }, { .4, .55 }, { .62, .9 }, { .8, .35 }, { 1, .5 } } }
  end,
  audio_visualiser = function(kit)
    return kit.audio_visualiser { width = 260, height = 190, values = bands(8), label = "Spectrum" }
  end,
  compressor_curve = function(kit)
    return kit.compressor_curve { width = 260, height = 190, threshold = -20, ratio = 4, knee = 8 }
  end,
  eq_bars = function(kit)
    local level = morf.channel { size = 5, mode = "frame" }
    level:set { .5, .9, .35, .7, .6 }
    return ui.Item { width = 260, height = 190,
      kit.eq_bars { x = 20, y = 40, width = 72, height = 64, bars = 4 },
      kit.eq_bars { x = 120, y = 40, width = 120, height = 64, bars = 5, channel = level, color = "info" },
      kit.eq_bars { x = 20, y = 130, width = 40, height = 32, bars = 4, playing = false, color = "ok" },
      kit.eq_bars { x = 84, y = 130, width = 28, height = 24, bars = 3, color = "warn" } }
  end,
  lissajous = function(kit)
    local pts = {}
    for k = 0, 600 do
      local t = k / 600 * 2 * math.pi
      pts[#pts + 1] = .85 * math.sin(3 * t + .6)
      pts[#pts + 1] = .85 * math.sin(2 * t)
    end
    return kit.lissajous { width = 260, height = 190, values = pts, label = "Phase" }
  end,
  mixer_strip = function(kit)
    local level = morf.channel { size = 4 }
    level:push(.82)
    return ui.Row { gap = 4,
      kit.mixer_strip { label = "Kick", width = 84, height = 190, volume = .75, pan = 0, level = .7 },
      kit.mixer_strip { label = "Bass", width = 84, height = 190, volume = .6, pan = -.4, level = level, mute = true },
      kit.mixer_strip { label = "Vox", width = 84, height = 190, volume = .9, pan = .3, level = .95, solo = true } }
  end,
  piano_keyboard = function(kit)
    return ui.Item { width = 260, height = 190,
      kit.piano_keyboard { y = 30, width = 260, height = 130, octaves = 2, start = 48, notes = { 52, 55, 60 } } }
  end,
  parametric_eq = function(kit)
    return kit.parametric_eq { width = 260, height = 190 }
  end,
  tuner = function(kit)
    return kit.tuner { width = 260, height = 190, frequency = 443.1 }
  end,
}
