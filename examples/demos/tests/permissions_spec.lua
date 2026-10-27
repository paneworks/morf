-- An imported module that asks to run a program: denied while unapproved,
-- with the shell still running, and free once approved.
-- `morf test --no-dbus examples/demos/tests/permissions_spec.lua`.

local test = morf.test

local function load()
  test.load("permissions/config/shell.lua", { runtime_path = { "permissions/site" } })
end

local function permission_lines()
  local lines = {}
  for _, entry in ipairs(test.logs("warn")) do
    if entry.message:find("permission:", 1, true) == 1 then lines[#lines + 1] = entry.message end
  end
  return lines
end

test.describe("an imported module that runs a program at load", function()
  test.it("is denied with nil and a reason while unapproved", function()
    load()
    local ran, why = test.ipc("probe")
    test.eq(ran, false)
    test.contains(why, "permission")
    test.contains(why, "`fixture_probe`")
    test.contains(why, "run (morf.process.spawn)")
    test.eq(#permission_lines(), 1, "one log line for the refusal")
  end)

  test.it("leaves the shell running", function()
    load()
    test.eq(test.ipc("alive"), "alive")
    test.eq(test.surfaces()[1].kind, "primary")
  end)

  test.it("runs once approved", function()
    load()
    test.approve("fixture_probe")
    load()
    local ran, why = test.ipc("probe")
    test.eq(ran, true, tostring(why))
    test.eq(#permission_lines(), 0)
  end)
end)
