-- Conformance: a theme's kit against the contract (contract.lua).
--
-- `check.kit(kit)` lists what the kit lacks of everything the contract
-- requires at its current stage: the kit's functions, the display widgets'
-- functions, and a skin for each archetype that has arrived.
-- `check.catalogue()` lists catalogue rows whose owner the contract does
-- not define. Both return a list of problems, empty when all is well.
-- `morf check --kit` runs both inside a loaded configuration.

local contract = require("lib.kit.contract")
local M = {}

local function due(entry) return (entry.stage or 1) <= contract.stage end

--- What `kit` lacks, as messages.
function M.kit(kit)
  local problems = {}
  if type(kit) ~= "table" then
    return { "the kit is not a table (got " .. type(kit) .. ")" }
  end
  for _, entry in ipairs(contract.functions) do
    if due(entry) and not entry.value and type(kit[entry.name]) ~= "function" then
      problems[#problems + 1] = ("kit.%s (%s) is missing"):format(entry.name, entry.group)
    end
  end
  for group, list in pairs(contract.display) do
    for _, entry in ipairs(list) do
      if due(entry) and type(kit[entry.fn]) ~= "function" then
        problems[#problems + 1] = ("display %s.%s: kit.%s is missing"):format(group, entry.name, entry.fn)
      end
    end
  end
  for name, archetype in pairs(contract.archetypes) do
    if due(archetype) then
      local skins = kit.skins
      local skin = type(skins) == "table" and skins[name] or nil
      -- A skin is a function of the state, or a table of one per slot.
      if type(skin) ~= "function" and type(skin) ~= "table" then
        problems[#problems + 1] = ("archetype %s: kit.skins.%s is missing"):format(name, name)
      end
    end
  end
  table.sort(problems)
  return problems
end

--- Whether `owner` (a catalogue row's) names something the contract has.
local function defined(owner)
  if contract.archetypes[owner] then return true end
  local kind, what = owner:match("^(%a+):(.+)$")
  if kind == "display" then return contract.display[what] ~= nil end
  if kind == "composite" then return contract.composites[what] ~= nil end
  if kind == "domain" then return contract.domain[what] ~= nil end
  if kind == "engine" then
    for _, name in ipairs(contract.engine) do if name == what then return true end end
    return false
  end
  return owner == "platform"
end

--- Catalogue rows whose owner the contract does not define.
function M.catalogue()
  local problems = {}
  for _, row in ipairs(require("lib.kit.catalogue")) do
    if not defined(row[4]) then
      problems[#problems + 1] = ("catalogue #%d %s: owner %s is not in the contract"):format(row[1], row[2], row[4])
    end
  end
  return problems
end

--- How many contract entries are due, and how many the kit has.
function M.summary(kit)
  local due_count, missing = 0, #M.kit(kit)
  for _, entry in ipairs(contract.functions) do if due(entry) and not entry.value then due_count = due_count + 1 end end
  for _, list in pairs(contract.display) do
    for _, entry in ipairs(list) do if due(entry) then due_count = due_count + 1 end end
  end
  for _, archetype in pairs(contract.archetypes) do if due(archetype) then due_count = due_count + 1 end end
  return { stage = contract.stage, due = due_count, missing = missing }
end

return M
