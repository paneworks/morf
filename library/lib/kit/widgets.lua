-- A constructor for every widget of the contract (contract.lua), the same
-- in every theme: `widgets.switch { checked = on, on_toggled = set }`,
-- `widgets.scale { value = level, on_moved = set_level }`,
-- `widgets.label { text = "Title", variant = "title_2" }`.
--
-- An archetype's widget is its archetype with the settings that make it
-- what it is (lib.kit.archetypes.<archetype>), drawn by the theme's skin
-- for it; it returns its node, and what its archetype gives besides (a
-- popup returns its handle instead: it lives in the overlay layer; a text
-- field the input inside it; a collection, a navigation, a shell and a
-- canvas their handle). A display widget is the theme's skin of that name
-- (lib.kit.skin's `draw`); a composite or a shell group widget is its
-- module of lib.kit.composites.
local contract = require("lib.kit.contract")
local skin = require("lib.kit.skin")

local M = {}

local function archetype(name) return require("lib.kit.archetypes." .. name) end

-- How each archetype's widgets are made.
local MAKE = {
  Press = function(widget, spec) return archetype("press").make(widget, spec) end,
  Range = function(widget, spec) return archetype("range").make(widget, spec) end,
  Plane = function(widget, spec) return archetype("plane").make(widget, spec) end,
  Selection = function(widget, spec) return archetype("selection").make(widget, spec) end,
  Popup = function(widget, spec) return archetype("popup").make(widget, spec) end,
  TextField = function(widget, spec) return archetype("text_field").make(widget, spec) end,
  Scroll = function(widget, spec) return archetype("scroll").make(widget, spec) end,
  Collection = function(widget, spec) return archetype("collection").make(widget, spec) end,
  Disclosure = function(widget, spec) return archetype("disclosure").make(widget, spec) end,
  Drag = function(widget, spec) return archetype("drag").make(widget, spec) end,
  Navigation = function(widget, spec)
    if widget == "carousel" then return archetype("navigation").carousel(spec) end
    if widget == "tab_view" then return archetype("navigation").tab_view(spec) end
    return archetype("navigation").make(widget, spec)
  end,
  Shell = function(widget, spec)
    if widget == "header_bar" then return archetype("shell").header_bar(spec) end
    return archetype("shell").make(widget, spec)
  end,
  Canvas = function(widget, spec) return archetype("canvas").make(widget, spec) end,
  Dock = function(widget, spec) return archetype("dock").make(widget, spec) end,
}

for _, group in ipairs { "archetypes", "editor" } do
  for name, entry in pairs(contract[group]) do
    for _, widget in ipairs(entry.widgets) do
      local make = MAKE[name]
      M[widget.name] = function(spec) return make(widget.name, spec) end
    end
  end
end
for _, widget in ipairs(contract.display) do
  M[widget.name] = function(spec) return skin.draw(widget.name, spec) end
end
for _, group in ipairs { "composites", "shell" } do
  for _, widget in ipairs(contract[group]) do
    M[widget.name] = function(spec) return require("lib.kit.composites")[widget.name](spec) end
  end
end

return M
