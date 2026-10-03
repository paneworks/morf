-- The audio and editor instruments (library/lib/kit/domain/audio.lua and
-- editor.lua) at work in each caelestia theme: an automation point drags
-- and a double click adds one, the compressor's threshold moves, an EQ
-- band drags, a mixer fader moves and mute toggles, piano keys sound and
-- stop (by the pointer and the computer keys), an ADSR point drags and
-- steps by the arrows, a node card moves, a piano-roll note moves, and
-- sequencer steps toggle by click and by the keys.
--
--     morf test --no-dbus examples/shells/caelestia/tests/domain_audio_editor_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  morf.surface.height = 900
  local log = {}
  local function note(s) log[#log + 1] = s end
  local last = {}
  local piano = kit.piano_keyboard { id = "piano", x = 20, y = 260, width = 260, height = 110, octaves = 1, start = 60,
    keyboard = true, on_note_on = function(n) note("on:" .. n) end, on_note_off = function(n) note("off:" .. n) end }
  local root = ui.Item { width = 1500, height = 900, piano,
    kit.automation_lane { id = "lane", x = 20, y = 20, width = 260, height = 190,
      points = { { 0, .2 }, { .4, .5 }, { 1, .6 } },
      on_changed = function(points) last.lane = points end },
    kit.compressor_curve { id = "comp", x = 300, y = 20, width = 260, height = 190, threshold = -20, ratio = 4,
      on_changed = function(t, r) last.threshold, last.ratio = t, r end },
    kit.parametric_eq { id = "peq", x = 580, y = 20, width = 260, height = 190,
      bands = { { freq = 200, gain = 0, q = 1 }, { freq = 2000, gain = 3, q = 1 } },
      on_changed = function(i, b) last.band = { i = i, freq = b.freq, gain = b.gain } end },
    kit.envelope_editor { id = "env", x = 860, y = 20, width = 260, height = 190, attack = .5, decay = .5, sustain = .5,
      release = 1, on_changed = function(a, d, s, r) last.env = { a = a, d = d, s = s, r = r } end },
    kit.mixer_strip { id = "strip", x = 1140, y = 20, width = 84, height = 190, volume = .75, level = .5,
      on_volume = function(v) last.volume = v end, on_mute = function(on) note("mute:" .. tostring(on)) end },
    kit.node_editor { id = "graph", x = 300, y = 260, width = 260, height = 190,
      nodes = { { id = "osc", title = "Osc", x = 10, y = 10, outputs = { "Out" } },
        { id = "out", title = "Out", x = 150, y = 60, inputs = { "In" } } },
      links = { { "osc", "Out", "out", "In" } },
      on_moved = function(id, x, y) last.node = { id = id, x = x, y = y } end },
    kit.step_sequencer { id = "seq", x = 580, y = 260, width = 260, height = 190, tracks = { "Kick", "Snare" }, steps = 8,
      pattern = { { 1, 5 }, {} }, on_changed = function(t, s, on) note(("step:%d:%d:%s"):format(t, s, tostring(on))) end },
    kit.piano_roll { id = "roll", x = 860, y = 260, width = 260, height = 190, notes = { { 64, 2, 2 } },
      on_changed = function(notes) last.roll = notes end },
  }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.last = function(key) return last[key] end
  morf.ipc.focus_piano = function() morf.focus.set(piano, true) end
]]

local function load(style)
  test.load("../shell/init.lua", { size = { 1500, 900 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
  test.settle(600)
end

local function centre(id)
  local n = test.get(id)
  return n.x + n.width / 2, n.y + n.height / 2
end

local function drag(id, dx, dy)
  local x, y = centre(id)
  test.drag({ x, y }, { x + dx, y + dy }, { steps = 10 })
  test.advance(50)
end

for _, style in ipairs { "material", "tsugumori" } do
  test.describe(style, function()
    test.it("drags an automation point, and a double click adds one", function()
      load(style)
      drag("lane-point-2", 0, -40)
      local points = test.ipc("last", "lane")
      test.truthy(points, "the lane said nothing")
      test.eq(#points, 3)
      test.truthy(points[2].v > .6, "the point did not rise: " .. tostring(points[2].v))
      -- A double click on the lane, away from the points.
      local lane = test.get("lane-point-1")
      local x, y = lane.x + 200, lane.y - 10
      test.click(x, y) test.advance(60) test.click(x, y) test.advance(60)
      test.eq(#test.ipc("last", "lane"), 4, "no point was added")
    end)

    test.it("moves the compressor's threshold and ratio", function()
      load(style)
      drag("comp-threshold", -30, 0)
      local t = test.ipc("last", "threshold")
      test.truthy(t and t < -20, "the threshold did not fall: " .. tostring(t))
      drag("comp-ratio", 0, -30)
      test.truthy(test.ipc("last", "ratio") < 4, "the ratio did not ease")
    end)

    test.it("drags an EQ band in frequency and gain", function()
      load(style)
      drag("peq-band-1", 30, -20)
      local b = test.ipc("last", "band")
      test.truthy(b, "the EQ said nothing")
      test.eq(b.i, 1)
      test.truthy(b.freq > 200, "the band's frequency did not rise: " .. tostring(b.freq))
      test.truthy(b.gain > 0, "the band's gain did not rise: " .. tostring(b.gain))
      -- The chip picks a band.
      test.click("peq-chip-2") test.advance(30)
    end)

    test.it("moves a mixer fader and toggles mute", function()
      load(style)
      local f = test.get("strip-fader")
      local x = f.x + f.width / 2
      test.drag({ x, f.y + f.height - 4 }, { x, f.y + f.height * .5 }, { steps = 8 })
      test.advance(50)
      local v = test.ipc("last", "volume")
      test.truthy(v and v > .3 and v < .7, "the fader did not follow: " .. tostring(v))
      -- Up by the keys, once focused.
      test.key("Up") test.advance(20)
      test.truthy(test.ipc("last", "volume") > v, "the arrow did not raise it")
      test.ipc("log")
      test.click("strip-mute") test.advance(30)
      test.eq(test.ipc("log"), "mute:true")
      test.click("strip-mute") test.advance(30)
      test.eq(test.ipc("log"), "mute:false")
    end)

    test.it("sounds and stops piano keys", function()
      load(style)
      test.ipc("log")
      local x, y = centre("piano-key-60")
      test.press(x, y + 30) test.advance(30)
      test.eq(test.ipc("log"), "on:60")
      test.release(x, y + 30) test.advance(30)
      test.eq(test.ipc("log"), "off:60")
      x, y = centre("piano-key-61")
      test.press(x, y) test.advance(30) test.release(x, y) test.advance(30)
      test.eq(test.ipc("log"), "on:61,off:61")
      -- The computer keys, once it has focus: A is the first C, E its D sharp.
      test.ipc("focus_piano") test.advance(20)
      test.key("a") test.advance(20)
      test.key("e") test.advance(20)
      test.eq(test.ipc("log"), "on:60,off:60,on:63,off:63")
    end)

    test.it("drags an ADSR point and steps it by the arrows", function()
      load(style)
      drag("env-attack", 20, 0)
      local e = test.ipc("last", "env")
      test.truthy(e and e.a > .5, "the attack did not lengthen")
      local a = e.a
      test.key("Right") test.advance(20)
      test.truthy(test.ipc("last", "env").a > a, "the arrow did not lengthen it")
      drag("env-sustain", 0, -20)
      test.truthy(test.ipc("last", "env").s > .5, "the sustain did not rise")
    end)

    test.it("moves a node card, and its link follows", function()
      load(style)
      drag("graph-node-osc", 40, 30)
      local n = test.ipc("last", "node")
      test.truthy(n, "the node editor said nothing")
      test.eq(n.id, "osc")
      test.near(n.x, 50, 2)
      test.near(n.y, 40, 2)
      test.key("Right") test.advance(20)
      test.near(test.ipc("last", "node").x, 58, 2)
    end)

    test.it("moves a piano-roll note by whole steps", function()
      load(style)
      local n = test.get("roll-note-1")
      local x, y = n.x + 4, n.y + n.height / 2
      test.drag({ x, y }, { x + n.width, y }, { steps = 10 })
      test.advance(50)
      local notes = test.ipc("last", "roll")
      test.truthy(notes, "the roll said nothing")
      test.eq(notes[1].start, 4)
      test.eq(notes[1].pitch, 64)
    end)

    test.it("toggles sequencer steps by click and by the keys", function()
      load(style)
      test.ipc("log")
      test.click("seq-step-1-2") test.advance(30)
      test.eq(test.ipc("log"), "step:1:2:true")
      test.click("seq-step-1-5") test.advance(30)
      test.eq(test.ipc("log"), "step:1:5:false")
      test.key("Down") test.advance(20)
      test.key("space") test.advance(20)
      test.eq(test.ipc("log"), "step:2:5:true")
      test.eq(#test.logs("error"), 0, "errors were logged")
    end)
  end)
end
