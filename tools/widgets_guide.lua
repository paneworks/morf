-- Writes docs/UI.md's widgets chapter reference sections from the contract
-- (library/lib/kit/contract.lua) and the head comments of the kit's glue
-- and composites, so the guide says what the code does:
--
--     lua tools/widgets_guide.lua > /tmp/widgets.md
--
-- Run from the repository root.
package.path = "library/?.lua;library/?/init.lua;" .. package.path
local contract = dofile("library/lib/kit/contract.lua")

local function head(path)
  local f = io.open(path)
  if not f then return "" end
  local out = {}
  for line in f:lines() do
    local text = line:match("^%-%-%s?(.*)$")
    if not text then break end
    out[#out + 1] = text
  end
  f:close()
  -- An indented block (`--     code`) is a code example.
  local md, code = {}, false
  for _, line in ipairs(out) do
    local is_code = line:match("^    ") ~= nil
    if is_code and not code then md[#md + 1] = "```lua" code = true end
    if not is_code and code and line ~= "" then md[#md + 1] = "```" code = false end
    md[#md + 1] = is_code and line:sub(5) or line
  end
  if code then md[#md + 1] = "```" end
  return table.concat(md, "\n")
end

local function list(t) return t and table.concat(t, ", ") or "" end

local GLUE = { Control = "control", Press = "widgets", Range = "widgets", Plane = "widgets", Selection = "selection",
  Popup = "popup", TextField = "text_field", Scroll = "scroll", Collection = "collection", Disclosure = "disclosure",
  Drag = "drag", Navigation = "navigation", Shell = "shell", Canvas = "canvas", Dock = "dock",
  Transform = "transform", Sheet = "sheet", Roving = "roving", Form = "form", Overflow = "overflow" }
local ORDER = { "Press", "Range", "Plane", "Selection", "Popup", "TextField", "Scroll", "Collection", "Disclosure",
  "Drag", "Navigation", "Shell", "Canvas", "Dock", "Transform", "Sheet", "Roving", "Form", "Overflow" }

local out = {}
local function p(s) out[#out + 1] = s or "" end

p("### The archetypes")
p()
p("Each archetype below: its accessible roles, its state (fields of `t`), the signals")
p("it raises, the keys it answers, the slots its skin fills and the widgets that")
p("are it -- then how its glue is used.")
for _, name in ipairs(ORDER) do
  local a = contract.archetypes[name]
  p()
  p("#### " .. name)
  p()
  p("| | |")
  p("|---|---|")
  p("| roles | " .. list(a.role) .. " |")
  p("| state | " .. list(a.state) .. " |")
  if a.modes then p("| modes | " .. list(a.modes) .. " |") end
  if a.tools then p("| tools | " .. list(a.tools) .. " |") end
  p("| signals | " .. list(a.signals) .. " |")
  p("| keys | " .. list(a.keys) .. " |")
  p("| slots | background, content, " .. list(a.slots) .. " |")
  p("| widgets | " .. list(a.widgets) .. " |")
  p("| arrived | stage " .. a.stage .. " |")
  p()
  local glue = GLUE[name]
  if glue ~= "widgets" or name == "Press" then
    p(head("library/lib/kit/" .. glue .. ".lua"))
  else
    p("Made through `lib.kit.widgets` like a press: `widgets." .. (a.widgets[1] or "") .. " { ... }`.")
  end
end

p()
p("### Display widgets")
p()
p("No input, a kit function each (`kit.<function>(spec)`), drawn by every theme;")
p("those a theme does not draw itself come from the shared composition in")
p("`library/lib/kit/display/` through its style (see the head of")
p("`library/lib/kit/display/init.lua`).")
p()
p("| group | widget | function |")
p("|---|---|---|")
local groups = {}
for g in pairs(contract.display) do groups[#groups + 1] = g end
table.sort(groups)
for _, g in ipairs(groups) do
  for _, e in ipairs(contract.display[g]) do p(("| %s | %s | `kit.%s` |"):format(g, e.name:gsub("_", " "), e.fn)) end
end

p()
p("### Domain instruments")
p()
p("Over the display widgets and the archetypes, in `library/lib/kit/domain/`:")
p()
local areas = {}
for area in pairs(contract.domain) do areas[#areas + 1] = area end
table.sort(areas)
for _, area in ipairs(areas) do
  local d = contract.domain[area]
  p(("- **%s** (stage %d): %s"):format(area, d.stage, list(d.widgets)))
end

p()
p("### Composites")
p()
p("Shared by every theme, built only from archetypes, in")
p("`library/lib/kit/composites/` (`require(\"lib.kit.composites\").<name>(spec)`).")
local names = {}
for name in pairs(contract.composites) do names[#names + 1] = name end
table.sort(names)
for _, name in ipairs(names) do
  local c = contract.composites[name]
  p()
  p("#### " .. name:gsub("_", " "))
  p()
  p("Built from " .. list(c.parts) .. (c.variants and ("; variants: " .. list(c.variants)) or "") .. ".")
  p()
  p(head("library/lib/kit/composites/" .. name .. ".lua"))
end
print(table.concat(out, "\n"))
