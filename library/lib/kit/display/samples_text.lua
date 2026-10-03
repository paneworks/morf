-- Gallery samples for the text display widgets: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local ui = require("morf.ui")

local CODE = [[
-- the reading, smoothed
local function ease(v, to)
  local d = to - v
  if math.abs(d) < 0.01 then
    return to
  end
  return v + d * 0.25
end]]

return {
  markup = function(kit)
    return kit.markup { width = 260, text = "Set in <b>bold</b>, <i>italic</i>, <u>underlined</u> and "
      .. "<s>struck</s> runs, with a <a href=\"https://example.org\">link</a> in the middle of a "
      .. "wrapped paragraph &amp; an entity." }
  end,
  code_block = function(kit)
    return kit.code_block { width = 260, text = CODE, numbers = true, language = "lua" }
  end,
  quote = function(kit)
    return kit.quote { width = 260, text = "Simplicity is prerequisite for reliability.",
      cite = "Edsger W. Dijkstra" }
  end,
  mono = function(kit)
    return ui.Column { gap = 10,
      kit.mono { text = "uuid 3f2a9c1e-77b0-4d1e" },
      kit.mono { text = "sha  9f86d081884c7d65" },
      kit.mono { text = "A long mono run wraps at its width like any other text.", width = 260 } }
  end,
  link_text = function(kit)
    return ui.Column { gap = 14,
      kit.link_text { text = "Open the documentation", href = "https://example.org/docs" },
      kit.link_text { before = "Read the ", text = "release notes", after = " before updating your system.",
        href = "https://example.org/notes", width = 260 } }
  end,
}
