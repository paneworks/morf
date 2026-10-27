-- Conformance: a theme against the contract (contract.lua).
--
-- `check.kit(kit)` lists what the theme lacks: a token of every group
-- (`kit.tokens.colors.<name>` and the type, shape and motion ones), a skin
-- for every archetype (`kit.skins.<Archetype>`, a function of the state or
-- a table of one per slot) and for every display widget
-- (`kit.skins.<widget>`, a function of the spec), and -- the library's own
-- -- a module for every composite and shell widget. `check.editor(kit)`
-- lists what it lacks of the editor group, which a theme may skip. Both
-- return a list of problems, empty when all is well. `morf check --kit`
-- runs them inside a loaded configuration.

local contract = require("lib.kit.contract")
local M = {}

local function is_skin(v) return type(v) == "function" or type(v) == "table" end

--- What `kit` lacks, as messages.
function M.kit(kit)
  if type(kit) ~= "table" then
    return { "the kit is not a table (got " .. type(kit) .. ")" }
  end
  local problems = {}
  local tokens = type(kit.tokens) == "table" and kit.tokens or {}
  for group, names in pairs(contract.tokens) do
    local have = type(tokens[group]) == "table" and tokens[group] or {}
    for _, name in ipairs(names) do
      if have[name] == nil then problems[#problems + 1] = ("token %s.%s is missing"):format(group, name) end
    end
  end
  local skins = type(kit.skins) == "table" and kit.skins or {}
  for name in pairs(contract.archetypes) do
    if not is_skin(skins[name]) then problems[#problems + 1] = ("archetype %s: kit.skins.%s is missing"):format(name, name) end
  end
  for _, entry in ipairs(contract.display) do
    if type(skins[entry.name]) ~= "function" then
      problems[#problems + 1] = ("display %s: kit.skins.%s is missing"):format(entry.name, entry.name)
    end
  end
  local composites = require("lib.kit.composites")
  for _, group in ipairs { "composites", "shell" } do
    for _, entry in ipairs(contract[group]) do
      if not composites.has(entry.name) then
        problems[#problems + 1] = ("%s %s: lib.kit.composites.%s is missing"):format(group, entry.name, entry.name)
      end
    end
  end
  table.sort(problems)
  return problems
end

--- What `kit` lacks of the editor group (Canvas and Dock), as messages.
function M.editor(kit)
  local skins = type(kit) == "table" and type(kit.skins) == "table" and kit.skins or {}
  local problems = {}
  for name in pairs(contract.editor) do
    if not is_skin(skins[name]) then problems[#problems + 1] = ("editor %s: kit.skins.%s is missing"):format(name, name) end
  end
  table.sort(problems)
  return problems
end

--- How many names the contract has, and how many of them `kit` lacks; the
--- editor group's apart.
function M.summary(kit)
  local due = #contract.display + #contract.composites + #contract.shell
  for _, names in pairs(contract.tokens) do due = due + #names end
  for _ in pairs(contract.archetypes) do due = due + 1 end
  local editor = 0
  for _ in pairs(contract.editor) do editor = editor + 1 end
  return { due = due, missing = #M.kit(kit), editor = editor, editor_missing = #M.editor(kit) }
end

return M
