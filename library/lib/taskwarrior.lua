-- Taskwarrior's CLI is the database boundary: no private storage formats,
-- no shell interpolation. Uses the person's TASKRC/TASKDATA as task does.
-- https://taskwarrior.org/docs/commands/export/
-- https://taskwarrior.org/docs/commands/modify/
local morf = require("morf")
local M = {}

function M.timestamp(value)
  if not value or value == "" then return nil end
  local iso = tostring(value):gsub("^(%d%d%d%d)(%d%d)(%d%d)T(%d%d)(%d%d)(%d%d)Z$", "%1-%2-%3T%4:%5:%6Z")
  return morf.time.parse(iso)
end
function M.date(value, format)
  local t = M.timestamp(value)
  return t and morf.time.format(format or "%Y-%m-%dT%H:%M", t) or ""
end
function M.day(value) return M.date(value, "%Y-%m-%d") end

local function uuid(value)
  return type(value) == "string" and value:match("^%x%x%x%x%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x$") ~= nil
end
local function trim(s) return tostring(s or ""):match("^%s*(.-)%s*$") end

function M.new(options)
  options = options or {}
  local self = {
    tasks = morf.signal("taskwarrior.tasks", {}),
    busy = morf.signal("taskwarrior.busy", false),
    error = morf.signal("taskwarrior.error", ""),
    loaded = morf.signal("taskwarrior.loaded", false),
    revision = morf.signal("taskwarrior.revision", 0),
  }
  local timer
  local function run(args, callback)
    local argv = { options.command or "task", "rc.confirmation=off", "rc.recurrence.confirmation=no",
      "rc.allow.empty.filter=off", "rc.verbose=nothing", "rc.color=off", "rc.json.array=on",
      "rc.context=", "rc.date.iso=on" }
    for _, arg in ipairs(args) do argv[#argv + 1] = arg end
    self.busy:set(true)
    self.error:set("")
    local function done(result)
      self.busy:set(false)
      if not result or not result.ok or result.truncated then
        local why = result and (result.error or result.stderr) or ""
        if not why or trim(why) == "" then why = "Taskwarrior did not complete the command." end
        if result and result.error then why = "Taskwarrior unavailable. Install task and check your configuration. " .. why end
        if result and result.truncated then why = "Taskwarrior returned too much data. Narrow your task database." end
        self.error:set(trim(why):sub(1, 500))
        callback(false)
        return
      end
      callback(true, result.stdout or "")
    end
    local ok, why = pcall(morf.run, argv, { env = options.env, timeout_ms = 15000, max_output = 8 * 1024 * 1024 }, done)
    if not ok then done { error = tostring(why), ok = false } end
  end

  function self.refresh()
    if self.busy:get() then return false end
    run({ "(", "status:pending", "or", "status:waiting", ")", "export" }, function(ok, output)
      if not ok then return end
      local parsed, rows = pcall(morf.json.decode, output)
      if not parsed or type(rows) ~= "table" or output:match("^%s*%[") == nil then
        self.error:set("Taskwarrior returned invalid JSON.")
        return
      end
      local tasks = {}
      for _, row in ipairs(rows) do
        if uuid(row.uuid) and type(row.description) == "string" then tasks[#tasks + 1] = row end
      end
      table.sort(tasks, function(a, b)
        if (a.start ~= nil) ~= (b.start ~= nil) then return a.start ~= nil end
        local au, bu = tonumber(a.urgency) or 0, tonumber(b.urgency) or 0
        if au ~= bu then return au > bu end
        return a.uuid < b.uuid
      end)
      self.tasks:set(tasks)
      self.revision:set(self.revision:get() + 1)
      self.loaded:set(true)
    end)
    return true
  end

  function self.watch(on)
    if timer then timer:cancel() timer = nil end
    if on then
      self.refresh()
      timer = morf.timer(10000, self.refresh, true)
    end
  end

  local function mutate(args, done)
    if self.busy:get() then return false end
    run(args, function(ok)
      if done then done(ok) end
      if ok then self.refresh() end
    end)
    return true
  end

  --- Fields may contain Taskwarrior dates (tomorrow, fri, ISO date/time).
  --- On edit, only fields passed by the caller are changed; "" clears one.
  function self.save(id, fields, done)
    if id and not uuid(id) then self.error:set("Select a task first.") return false end
    if trim(fields.description) == "" then self.error:set("Give the task a description.") return false end
    if fields.priority and not ({ [""] = true, H = true, M = true, L = true })[fields.priority] then
      self.error:set("Priority must be H, M, L, or empty.") return false
    end
    if fields.recur and fields.recur ~= "" and not id and trim(fields.due) == "" then
      self.error:set("A repeating task needs a due date.") return false
    end
    local args = id and { id, "modify" } or { "add" }
    for _, key in ipairs { "description", "project", "priority", "scheduled", "due", "wait", "recur", "until", "depends" } do
      if fields[key] ~= nil then args[#args + 1] = key .. ":" .. trim(fields[key]) end
    end
    if fields.tags ~= nil then
      local tags = {}
      for tag in tostring(fields.tags):gmatch("[^,%s]+") do
        if not tag:match("^[%w_%-]+$") then self.error:set("Tags use letters, numbers, underscores and hyphens.") return false end
        tags[#tags + 1] = tag
      end
      args[#args + 1] = "tags:" .. table.concat(tags, ",")
    end
    return mutate(args, done)
  end

  function self.action(id, action, done)
    if not uuid(id) then self.error:set("Select a task first.") return false end
    if not ({ done = true, delete = true, start = true, stop = true })[action] then return false end
    return mutate({ id, action }, done)
  end
  return self
end

return M
