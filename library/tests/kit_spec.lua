-- `library/lib/kit/`: the widget contract, the catalogue and the check.
--
--     morf test library/tests/kit_spec.lua

local test = morf.test

local HOST = [[
  local contract = require("lib.kit.contract")
  local catalogue = require("lib.kit.catalogue")
  local check = require("lib.kit.check")
  local function join(list) return table.concat(list, "\n") end
  morf.ipc.empty_kit = function() return join(check.kit({})) end
  morf.ipc.not_a_kit = function() return join(check.kit(42)) end
  morf.ipc.catalogue = function() return #catalogue, join(check.catalogue()) end
  morf.ipc.stage = function() return contract.stage end
  -- A kit that has every function due at the current stage.
  morf.ipc.full_kit = function()
    local kit = {}
    for _, entry in ipairs(contract.functions) do
      if not entry.value then kit[entry.name] = function() end end
    end
    for _, list in pairs(contract.display) do
      for _, entry in ipairs(list) do
        if entry.stage <= contract.stage then kit[entry.fn] = function() end end
      end
    end
    kit.skins = {}
    for name, archetype in pairs(contract.archetypes) do
      if archetype.stage <= contract.stage then kit.skins[name] = function() end end
    end
    return join(check.kit(kit))
  end
  -- Every archetype says what it is; every composite is made of archetypes.
  morf.ipc.shapes = function()
    local wrong = {}
    for name, a in pairs(contract.archetypes) do
      for _, field in ipairs { "role", "state", "signals", "keys", "slots", "widgets" } do
        if type(a[field]) ~= "table" or #a[field] == 0 then wrong[#wrong + 1] = name .. "." .. field end
      end
      if type(a.stage) ~= "number" then wrong[#wrong + 1] = name .. ".stage" end
    end
    for name, c in pairs(contract.composites) do
      for _, part in ipairs(c.parts) do
        if not contract.archetypes[part] then wrong[#wrong + 1] = name .. " uses " .. part end
      end
    end
    return join(wrong)
  end
  morf.ipc.archetype_count = function()
    local n = 0
    for _ in pairs(contract.archetypes) do n = n + 1 end
    return n
  end
]]

local function load() test.load { source = HOST } end

test.it("names every function an empty kit lacks", function()
  load()
  local problems = test.ipc("empty_kit")
  test.contains(problems, "kit.text (text) is missing")
  test.contains(problems, "kit.ring (reading) is missing")
  test.contains(problems, "display readings.gauge: kit.gauge is missing")
end)

test.it("refuses a kit that is not a table", function()
  load()
  test.contains(test.ipc("not_a_kit"), "the kit is not a table")
end)

test.it("passes a kit that has everything due", function()
  load()
  test.eq(test.ipc("full_kit"), "")
end)

test.it("gives every catalogue row an owner the contract defines", function()
  load()
  local rows, problems = test.ipc("catalogue")
  test.eq(rows, 960)
  test.eq(problems, "")
end)

test.it("describes all twelve archetypes and builds composites from them", function()
  load()
  test.eq(test.ipc("archetype_count"), 12)
  test.eq(test.ipc("shapes"), "")
end)
