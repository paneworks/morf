-- morf.test: the spec side of `morf test`.
--
-- A spec runs in a runtime of its own. The configuration it tests runs in
-- another, loaded by `test.load`, and everything a spec does to it --
-- clicks, keys, time, IPC -- crosses to the runner through `__morf_test`,
-- a table of host functions that exists only here.

local host = __morf_test
__morf_test = nil

local test = {}
local cases = {}
local names = {}
local before, after = {}, {}

-- ------------------------------------------------------------ describing --

local function full(name)
  if #names == 0 then return name end
  return table.concat(names, " ") .. " " .. name
end

local function copy(list)
  local out = {}
  for index, value in ipairs(list) do out[index] = value end
  return out
end

function test.describe(name, body)
  names[#names + 1] = name
  local kept_before, kept_after = #before, #after
  body()
  for index = #before, kept_before + 1, -1 do before[index] = nil end
  for index = #after, kept_after + 1, -1 do after[index] = nil end
  names[#names] = nil
end

function test.before_each(fn) before[#before + 1] = fn end
function test.after_each(fn) after[#after + 1] = fn end

function test.it(name, body)
  cases[#cases + 1] = { name = full(name), body = body, before = copy(before), after = copy(after) }
end

-- A test that is written down but not run, with the reason it is not.
function test.skip(name, reason)
  cases[#cases + 1] = { name = full(name), skip = type(reason) == "string" and reason or "skipped" }
end

-- ------------------------------------------------------------ assertions --

local function show(value, depth)
  depth = depth or 0
  if type(value) == "string" then return string.format("%q", value) end
  if type(value) ~= "table" then return tostring(value) end
  if depth > 3 then return "{...}" end
  local keys = {}
  for key in pairs(value) do keys[#keys + 1] = key end
  table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
  local parts = {}
  for _, key in ipairs(keys) do
    local shown = type(key) == "number" and "" or (tostring(key) .. " = ")
    parts[#parts + 1] = shown .. show(value[key], depth + 1)
    if #parts > 12 then parts[#parts + 1] = "..." break end
  end
  return "{ " .. table.concat(parts, ", ") .. " }"
end
test.show = show

local function same(a, b)
  if a == b then return true end
  if type(a) ~= "table" or type(b) ~= "table" then return false end
  for key, value in pairs(a) do
    if not same(value, b[key]) then return false end
  end
  for key in pairs(b) do
    if a[key] == nil then return false end
  end
  return true
end

-- Raised at the spec's line, not this file's: level 3 is whoever called
-- the assertion.
local function fail(message, extra)
  if extra then message = extra .. ": " .. message end
  error(message, 3)
end

function test.fail(message) error(message or "failed", 2) end

function test.eq(actual, expected, message)
  if not same(actual, expected) then
    fail("expected " .. show(expected) .. ", got " .. show(actual), message)
  end
  return actual
end

function test.ne(actual, unexpected, message)
  if same(actual, unexpected) then
    fail("expected anything but " .. show(unexpected), message)
  end
  return actual
end

function test.truthy(value, message)
  if not value then fail("expected a true value, got " .. show(value), message) end
  return value
end

function test.falsy(value, message)
  if value then fail("expected a false value, got " .. show(value), message) end
  return value
end

function test.near(actual, expected, tolerance, message)
  if type(tolerance) == "string" then tolerance, message = nil, tolerance end
  tolerance = tolerance or 1e-6
  if type(actual) ~= "number" or math.abs(actual - expected) > tolerance then
    fail(("expected %s within %s, got %s"):format(show(expected), show(tolerance), show(actual)), message)
  end
  return actual
end

function test.contains(haystack, needle, message)
  if type(haystack) == "string" then
    if not haystack:find(needle, 1, true) then
      fail("expected " .. show(haystack) .. " to contain " .. show(needle), message)
    end
    return haystack
  end
  if type(haystack) == "table" then
    for _, value in pairs(haystack) do
      if same(value, needle) then return haystack end
    end
  end
  fail("expected " .. show(haystack) .. " to contain " .. show(needle), message)
end

function test.matches(text, pattern, message)
  if type(text) ~= "string" or not text:find(pattern) then
    fail("expected " .. show(text) .. " to match " .. show(pattern), message)
  end
  return text
end

function test.raises(fn, pattern, message)
  local ok, err = pcall(fn)
  if ok then fail("expected an error", message) end
  if pattern and not tostring(err):find(pattern) then
    fail("expected an error matching " .. show(pattern) .. ", got " .. show(tostring(err)), message)
  end
  return err
end

-- ------------------------------------------------------------- the subject --

-- `test.load(path, { size = {w, h}, args = {...}, env = {...}, screens = n })`
-- loads a configuration; `test.load { source = [[...]] }` one written here.
function test.load(path, options)
  if type(path) == "table" then path, options = nil, path end
  return host.load(path, options or {})
end

function test.source(source, options)
  options = options or {}
  options.source = source
  return host.load(nil, options)
end

function test.surfaces() return host.surfaces() end
function test.now() return host.now() end
--- Whether the configuration holds the compositor's shortcuts off it now
--- (`morf.shortcuts.inhibit`).
function test.shortcuts_inhibited() return host.shortcuts_inhibited() end

function test.advance(ms) host.advance(ms or 16, 0) end
function test.settle(limit_ms) return host.settle(limit_ms or 5000) end

-- Real time, for answers that come from real processes: steps frames of
-- virtual time while waiting up to `timeout_ms` of the wall clock for
-- `predicate` to hold.
function test.wait(predicate, timeout_ms, message)
  local deadline = host.wall_ms() + (timeout_ms or 2000)
  while true do
    local value = predicate()
    if value then return value end
    if host.wall_ms() > deadline then
      fail("timed out after " .. tostring(timeout_ms or 2000) .. " ms", message)
    end
    host.advance(16, 5)
  end
end

-- --------------------------------------------------------------- finding --

local function matches_query(node, query)
  if type(query) == "function" then return query(node) end
  if type(query) == "string" then return node.id == query or node.text == query end
  for key, wanted in pairs(query) do
    if key == "text_contains" then
      if not (node.text or ""):find(wanted, 1, true) then return false end
    elseif key == "surface" then
      if node.surface ~= wanted and node.surface_kind ~= wanted then return false end
    elseif node[key] ~= wanted then
      return false
    end
  end
  return true
end

function test.nodes() return host.nodes() end

function test.find_all(query)
  local found = {}
  for _, node in ipairs(host.nodes()) do
    if matches_query(node, query) then found[#found + 1] = node end
  end
  return found
end

function test.find(query)
  for _, node in ipairs(host.nodes()) do
    if matches_query(node, query) then return node end
  end
  return nil
end

-- Like `find`, but a spec that needs the node fails where it is missing.
function test.get(query)
  local node = test.find(query)
  if node == nil then fail("no node matches " .. show(query)) end
  return node
end

-- The accessible tree a screen reader would be given: one row per node,
-- with `role`, `name`, `value`, states and the accessible `parent` handle.
-- A query filters the rows as `test.find_all` does nodes.
function test.accessible(query)
  local rows = host.accessible()
  if query == nil then return rows end
  local found = {}
  for _, row in ipairs(rows) do
    if matches_query(row, query) then found[#found + 1] = row end
  end
  return found
end

-- Does what a screen reader asks: `action` is "click", "focus",
-- "increment", "decrement", "expand", "collapse" or "set_value" (with
-- `value`), on a row of `test.accessible` or a node.
function test.accessible_action(row, action, value)
  if row == nil then fail("accessible_action wants a row") end
  return host.accessible_action(row.handle, action, value)
end

-- Configures a window (by its surface label or title) to `width` by
-- `height`, as a compositor does when it is resized.
function test.resize_window(surface, width, height)
  return host.resize_window(surface, width, height)
end

function test.text_of(node)
  if node == nil then fail("text_of wants a node") end
  return host.text_of(node.handle)
end

-- ------------------------------------------------------------------ input --

-- A point: `x, y`, a node (its centre), or a query for one.
local function point(x, y, options)
  if type(x) == "table" or type(x) == "string" or type(x) == "function" then
    local node = x
    if type(x) ~= "table" or x.handle == nil then node = test.get(x) end
    return node.x + node.width / 2, node.y + node.height / 2, y or {}, node.surface
  end
  return x, y, options or {}, nil
end

function test.click(x, y, options)
  local px, py, opts, surface = point(x, y, options)
  host.click(px, py, opts.button or "left", opts.surface or surface, opts.modifiers)
end

function test.move(x, y, options)
  local px, py, opts, surface = point(x, y, options)
  host.move(px, py, opts.surface or surface)
end

function test.touch(phase, id, x, y, options)
  host.touch(phase, id, x or 0, y or 0, (options or {}).surface, (options or {}).time_ms)
end

function test.swipe(from, to, options)
  options = options or {}
  local duration, id = options.duration or 160, options.id or 0
  local steps = options.steps or math.max(1, math.ceil(duration / 16))
  test.touch("down", id, from[1], from[2], options)
  for step = 1, steps do
    test.advance(duration / steps)
    test.touch("move", id, from[1] + (to[1] - from[1]) * step / steps,
      from[2] + (to[2] - from[2]) * step / steps, options)
  end
  test.touch("up", id, to[1], to[2], options)
end

function test.leave(options)
  host.leave((options or {}).surface)
end

function test.press(x, y, options)
  local px, py, opts, surface = point(x, y, options)
  host.button(px, py, opts.button or "left", true, opts.surface or surface, opts.modifiers)
end

function test.release(x, y, options)
  local px, py, opts, surface = point(x, y, options)
  host.button(px, py, opts.button or "left", false, opts.surface or surface, opts.modifiers)
end

function test.drag(from, to, options)
  options = options or {}
  local x1, y1 = from[1], from[2]
  local x2, y2 = to[1], to[2]
  host.move(x1, y1, options.surface)
  host.button(x1, y1, options.button or "left", true, options.surface, options.modifiers)
  local steps = options.steps or 8
  for step = 1, steps do
    host.move(x1 + (x2 - x1) * step / steps, y1 + (y2 - y1) * step / steps, options.surface)
  end
  host.button(x2, y2, options.button or "left", false, options.surface, options.modifiers)
end

function test.wheel(dx, dy, options)
  options = options or {}
  host.wheel(dx or 0, dy or 0, options.x, options.y, options.surface, options.modifiers)
end

local function modifier_list(modifiers)
  if modifiers == nil then return {} end
  if type(modifiers) == "string" then
    local list = {}
    for name in modifiers:gmatch("[^+%s,]+") do list[#list + 1] = name end
    return list
  end
  return modifiers
end

function test.key(name, modifiers, options)
  options = options or {}
  host.key(name, modifier_list(modifiers), options.surface, options.phase)
end

function test.type(text, options)
  options = options or {}
  host.type(text, options.surface)
end

-- ------------------------------------------------------ talking to it --

function test.ipc(verb, ...)
  return host.ipc(verb, ...)
end

function test.ipc_verbs() return host.ipc_verbs() end

-- `level` is the least that counts: "debug", "info", "warn" or "error".
function test.logs(level) return host.logs(level or "debug") end
function test.clear_logs() host.clear_logs() end

-- Fakes what `morf.run` answers for a program, by its name or path:
-- `test.stub_run("nmcli", { stdout = "..." })`, or just the stdout.
function test.stub_run(program, result)
  if type(result) == "string" then result = { stdout = result } end
  host.stub_run(program, result or { ok = true, code = 0 })
end

function test.clear_stubs() host.clear_stubs() end

-- Every command the configuration asked `morf.run` for, stubbed or not.
function test.runs() return host.runs() end

-- A picture of a surface (default the primary; "screen" for all of them),
-- when there is a GPU: true and the path, or false and why not.
function test.snapshot(name, options)
  options = options or {}
  return host.snapshot(name, options.surface)
end

function test.note(message) host.note(tostring(message)) end
function test.log(...)
  local parts = {}
  for index = 1, select("#", ...) do parts[#parts + 1] = tostring((select(index, ...))) end
  host.note(table.concat(parts, "\t"))
end

-- --------------------------------------------------------------- running --

morf.ipc["__morf_test.list"] = function()
  local list = {}
  for index, case in ipairs(cases) do
    list[index] = { name = case.name, skip = case.skip or "" }
  end
  return list
end

morf.ipc["__morf_test.run"] = function(index)
  local case = cases[index]
  local ok, err = true, nil
  for _, hook in ipairs(case.before) do
    ok, err = pcall(hook)
    if not ok then break end
  end
  if ok then ok, err = pcall(case.body) end
  for _, hook in ipairs(case.after) do
    local hook_ok, hook_err = pcall(hook)
    if ok and not hook_ok then ok, err = false, hook_err end
  end
  return { ok = ok, message = ok and "" or tostring(err) }
end

morf.test = test
package.loaded["morf.test"] = test
return test
