-- Real CLI tests use only a scratch TASKRC and TASKDATA, never the user's.
-- morf test --no-dbus library/tests/taskwarrior_spec.lua
local test = morf.test
local HOST = [[
  local lib = require("lib.integrations.taskwarrior")
  local root = morf.env("TASK_TEST_ROOT")
  local client = lib.new { env = { TASKRC = root .. "/taskrc", TASKDATA = root .. "/data" } }
  morf.ipc.refresh = client.refresh
  morf.ipc.tasks = function() return morf.json.encode(client.tasks:get()) end
  morf.ipc.error = function() return client.error:get() end
  morf.ipc.busy = function() return client.busy:get() end
  morf.ipc.save = function(id, json) return client.save(id ~= "" and id or nil, morf.json.decode(json)) end
  morf.ipc.action = client.action
  morf.ipc.date = lib.date
]]
local count = 0
local function load()
  count = count + 1
  local root = morf.env("XDG_CACHE_HOME") .. "/taskwarrior-spec-" .. count
  assert(morf.fs.write(root .. "/taskrc", "confirmation=off\n"))
  test.load { source = HOST, env = { TASK_TEST_ROOT = root } }
end
local function settled()
  test.wait(function() return not test.ipc("busy") end, 5000, "Taskwarrior command")
  test.eq(test.ipc("error"), "")
end
local function tasks() return morf.json.decode(test.ipc("tasks")) end
local function save(id, fields)
  test.eq(test.ipc("save", id or "", morf.json.encode(fields)), true)
  settled()
end

test.describe("taskwarrior", function()
  if not require("lib.util.poll").which("task") then
    test.skip("real task CRUD", "task executable is not installed")
  else
    test.it("adds, schedules, edits, starts, stops, completes and deletes real tasks", function()
      load()
      test.ipc("refresh") settled()
      test.eq(#tasks(), 0)
      local description = "Call Alex; +tag due:tomorrow $(literal)"
      save(nil, { description = description, project = "work.website", priority = "H",
        tags = "work,calls", scheduled = "2030-10-01T09:30", due = "2030-10-02T17:00" })
      local first = tasks()[1]
      test.eq(first.description, description)
      test.eq(first.project, "work.website")
      test.eq(first.priority, "H")
      test.eq(test.ipc("date", first.scheduled), "2030-10-01T09:30")
      test.eq(test.ipc("date", first.due), "2030-10-02T17:00")
      save(first.uuid, { description = "Updated task", priority = "L", tags = "home", due = "", scheduled = "" })
      first = tasks()[1]
      test.eq(first.description, "Updated task")
      test.eq(first.project, "work.website", "unmodified fields survive editing")
      test.eq(first.tags, { "home" })
      test.eq(first.due, nil)
      test.eq(first.scheduled, nil)
      test.ipc("action", first.uuid, "start") settled()
      test.truthy(tasks()[1].start)
      test.ipc("action", first.uuid, "stop") settled()
      test.eq(tasks()[1].start, nil)
      test.ipc("action", first.uuid, "done") settled()
      test.eq(#tasks(), 0)
      save(nil, { description = "Temporary task", wait = "2030-11-01" })
      test.eq(#tasks(), 1, "waiting tasks are included")
      test.ipc("action", tasks()[1].uuid, "delete") settled()
      test.eq(#tasks(), 0)
      save(nil, { description = "Weekly review", due = "today+9h", recur = "weekly", ["until"] = "today+30d" })
      test.truthy(#tasks() > 0, "the first recurring occurrence should be visible")
      test.eq(tasks()[1].description, "Weekly review")
      test.eq(tasks()[1].recur, "weekly")
    end)
  end
  test.it("rejects unscoped mutations and reports CLI errors", function()
    load()
    test.eq(test.ipc("action", "", "delete"), false)
    test.eq(test.ipc("save", "", '{"description":""}'), false)
    test.eq(test.ipc("save", "", '{"description":"Repeat","recur":"weekly"}'), false)
    test.stub_run("task", { code = 1, stderr = "Task database locked" })
    test.ipc("refresh")
    test.wait(function() return not test.ipc("busy") end, 1000)
    test.eq(test.ipc("error"), "Task database locked")
  end)

end)
