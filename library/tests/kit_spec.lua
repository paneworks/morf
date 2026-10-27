-- `library/lib/kit/`: the widget contract and the check.
--
--     morf test library/tests/kit_spec.lua

local test = morf.test

local HOST = [[
  local contract = require("lib.kit.contract")
  local check = require("lib.kit.check")
  local function join(list) return table.concat(list, "\n") end
  morf.ipc.empty_kit = function() return join(check.kit({})) end
  morf.ipc.not_a_kit = function() return join(check.kit(42)) end
  -- A theme with every token and every skin, and no editor group.
  morf.ipc.full_kit = function()
    local kit = { tokens = {}, skins = {} }
    for group, names in pairs(contract.tokens) do
      kit.tokens[group] = {}
      for _, name in ipairs(names) do kit.tokens[group][name] = 1 end
    end
    for name in pairs(contract.archetypes) do kit.skins[name] = function() end end
    for _, entry in ipairs(contract.display) do kit.skins[entry.name] = function() end end
    return join(check.kit(kit)), join(check.editor(kit))
  end
  -- Every archetype says what it is; every widget names its class; every
  -- composite is made of archetypes.
  morf.ipc.shapes = function()
    local wrong = {}
    for name, a in pairs(contract.archetypes) do
      for _, field in ipairs { "role", "state", "signals", "keys", "slots" } do
        if type(a[field]) ~= "table" or #a[field] == 0 then wrong[#wrong + 1] = name .. "." .. field end
      end
      if type(a.adw) ~= "string" then wrong[#wrong + 1] = name .. ".adw" end
      for _, w in ipairs(a.widgets) do
        if type(w.adw) ~= "string" then wrong[#wrong + 1] = w.name .. ".adw" end
      end
    end
    for _, c in ipairs(contract.composites) do
      for _, part in ipairs(c.parts) do
        if not contract.archetypes[part] then wrong[#wrong + 1] = c.name .. " uses " .. part end
      end
    end
    for _, s in ipairs(contract.shell) do
      if type(s.surface) ~= "string" then wrong[#wrong + 1] = s.name .. ".surface" end
    end
    return join(wrong)
  end
  morf.ipc.counts = function()
    local archetypes, names = 0, #contract.display + #contract.composites + #contract.shell
    for _, a in pairs(contract.archetypes) do archetypes = archetypes + 1 names = names + #a.widgets end
    return archetypes, names
  end
  -- Each name once across the contract: a constructor of lib.kit.widgets.
  morf.ipc.unique = function()
    local widgets = require("lib.kit.widgets")
    local seen, wrong = {}, {}
    local function see(name)
      if seen[name] then wrong[#wrong + 1] = name .. " twice" end
      seen[name] = true
      if type(widgets[name]) ~= "function" then wrong[#wrong + 1] = name .. " has no constructor" end
    end
    for _, a in pairs(contract.archetypes) do for _, w in ipairs(a.widgets) do see(w.name) end end
    for _, a in pairs(contract.editor) do for _, w in ipairs(a.widgets) do see(w.name) end end
    for _, group in ipairs { "display", "composites", "shell" } do
      for _, w in ipairs(contract[group]) do see(w.name) end
    end
    return join(wrong)
  end
]]

local function load() test.load { source = HOST } end

test.it("names every token and skin an empty kit lacks", function()
  load()
  local problems = test.ipc("empty_kit")
  test.contains(problems, "token colors.accent_bg is missing")
  test.contains(problems, "archetype Press: kit.skins.Press is missing")
  test.contains(problems, "display label: kit.skins.label is missing")
end)

test.it("refuses a kit that is not a table", function()
  load()
  test.contains(test.ipc("not_a_kit"), "the kit is not a table")
end)

test.it("passes a kit that has every token and skin, and reports the editor group apart", function()
  load()
  local problems, editor = test.ipc("full_kit")
  test.eq(problems, "")
  test.contains(editor, "editor Canvas: kit.skins.Canvas is missing")
end)

test.it("describes twelve archetypes and builds composites from them", function()
  load()
  local archetypes, names = test.ipc("counts")
  test.eq(archetypes, 12)
  test.eq(names, 99)
  test.eq(test.ipc("shapes"), "")
end)

test.it("names each widget once, and makes every one", function()
  load()
  test.eq(test.ipc("unique"), "")
end)
