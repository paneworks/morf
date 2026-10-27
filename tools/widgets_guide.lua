-- Writes docs/UI.md's widgets chapter reference sections from the contract
-- (library/lib/kit/contract.lua) and the head comments of the archetype
-- modules and composites, so the guide says what the code does:
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
    if not is_code and code and line ~= "" then
      while md[#md] == "" do md[#md] = nil end
      md[#md + 1] = "```"
      md[#md + 1] = ""
      code = false
    end
    md[#md + 1] = is_code and line:sub(5) or line
  end
  if code then md[#md + 1] = "```" end
  return table.concat(md, "\n")
end

local function list(t) return t and table.concat(t, ", ") or "" end

local function names(entries)
  local out = {}
  for _, e in ipairs(entries) do
    out[#out + 1] = "`" .. e.name .. "`" .. (e.variants and (" (" .. list(e.variants) .. ")") or "")
  end
  return table.concat(out, ", ")
end

local FILE = { TextField = "text_field" }
local ORDER = { "Press", "Range", "Plane", "Selection", "Popup", "TextField", "Scroll", "Collection", "Disclosure",
  "Drag", "Navigation", "Shell" }
local ROWS = { action_row = true, switch_row = true, combo_row = true, entry_row = true, password_entry_row = true,
  spin_row = true, expander_row = true, button_row = true, preferences_group = true, preferences_page = true }

local out = {}
local function p(s) out[#out + 1] = s or "" end

local function archetype(name, a)
  p()
  p("#### " .. name .. (a.adw and (" (" .. a.adw .. ")") or ""))
  p()
  p("| | |")
  p("|---|---|")
  p("| roles | " .. list(a.role) .. " |")
  p("| state | " .. list(a.state) .. " |")
  if a.tools then p("| tools | " .. list(a.tools) .. " |") end
  p("| signals | " .. list(a.signals) .. " |")
  p("| keys | " .. list(a.keys) .. " |")
  p("| slots | background, content, " .. list(a.slots) .. " |")
  if #a.widgets > 0 then p("| widgets | " .. names(a.widgets) .. " |") end
  p()
  p(head("library/lib/kit/archetypes/" .. (FILE[name] or name:lower()) .. ".lua"))
end

p("### The archetypes")
p()
p("Each archetype below: its accessible roles, its state (fields of `t`), the signals")
p("it raises, the keys it answers, the slots its skin fills and the widgets made from")
p("it, with their variants -- then how its module is used. Every widget is made")
p("through `lib.kit.widgets`: `widgets.<name> { ... }`.")
for _, name in ipairs(ORDER) do archetype(name, contract.archetypes[name]) end

p()
p("### Display widgets")
p()
p("No input, each drawn by the current theme's skin of its name")
p("(`widgets.<name>(spec)`, through `lib.kit.skin.draw`):")
p()
p("| widget | class | variants |")
p("|---|---|---|")
for _, e in ipairs(contract.display) do p(("| `%s` | %s | %s |"):format(e.name, e.adw, list(e.variants))) end

p()
p("### Composites")
p()
p("Shared by every theme, built only from archetypes, in")
p("`library/lib/kit/composites/` (`widgets.<name>(spec)`, or")
p("`require(\"lib.kit.composites\").<name>(spec)`).")
for _, c in ipairs(contract.composites) do
  p()
  p("#### " .. c.name .. " (" .. c.adw .. ")")
  p()
  p("Built from " .. list(c.parts) .. (c.variants and ("; variants: " .. list(c.variants)) or "") .. ".")
  p()
  p(head("library/lib/kit/composites/" .. (ROWS[c.name] and "rows/" or "") .. c.name .. ".lua"))
end

p()
p("### Shell widgets")
p()
p("What a desktop shell needs that libadwaita lacks, each for the surface that")
p("needs it.")
for _, s in ipairs(contract.shell) do
  p()
  p("#### " .. s.name)
  p()
  p("For " .. s.surface .. "; built from " .. list(s.parts) .. ".")
  p()
  p(head("library/lib/kit/composites/" .. s.name .. ".lua"))
end

p()
p("### The editor group")
p()
p("The Canvas and Dock archetypes, for editor-like applications; a theme may")
p("skip the group (`morf check --kit` warns of what it lacks).")
for _, name in ipairs { "Canvas", "Dock" } do archetype(name, contract.editor[name]) end
print(table.concat(out, "\n"))
