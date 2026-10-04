-- Gallery samples for the editor instruments: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
return {
  envelope_editor = function(kit)
    return kit.envelope_editor { width = 260, height = 190, attack = .25, decay = .6, sustain = .55, release = 1.6 }
  end,
  node_editor = function(kit)
    return kit.node_editor { width = 260, height = 190,
      nodes = { { id = "osc", title = "Osc", x = 4, y = 10, inputs = { "FM" }, outputs = { "Out" } },
        { id = "filter", title = "Filter", x = 80, y = 84, inputs = { "In", "Cut" }, outputs = { "Out" } },
        { id = "out", title = "Output", x = 160, y = 14, inputs = { "L", "R" }, outputs = {} } },
      links = { { "osc", "Out", "filter", "In" }, { "filter", "Out", "out", "L" }, { "filter", "Out", "out", "R" } } }
  end,
  piano_roll = function(kit)
    return kit.piano_roll { width = 260, height = 190, position = .3 }
  end,
  step_sequencer = function(kit)
    return kit.step_sequencer { width = 260, height = 190, position = function() return 5 end,
      pattern = { { 1, 5, 9, 13 }, { 5, 13 }, { 1, 3, 5, 7, 9, 11, 13, 15 }, { 8, 16 } } }
  end,
}
