-- Hyprland's configuration, changed at run time: options, animations,
-- monitor rules, workspaces moved between monitors, the cursor, reloads.
--
-- Opt-in, and built only on `lib/hyprland.lua` (its `eval`, `request`,
-- `batch`, `getoption`, `reload` and events); nothing here is known to the
-- engine, and nothing spawns `hyprctl`. The user's configuration files are
-- never written: whatever is set here lasts until the next reload, which is
-- why a configuration using this library pushes its values again from
-- `on_reload`.
--
-- Hyprland is configured in one of two languages, and they take changes
-- differently:
--
--   "lua"       a Lua config (0.56+). `keyword` is refused there, so every
--               change is an `eval` of an `hl.*` chunk, one per change so
--               nothing is applied half-way (`hl.config`, `hl.curve`,
--               `hl.animation`, `hl.monitor`, `hl.dispatch`).
--   "hyprlang"  the classic `.conf`. Changes are `keyword` commands sent in
--               one batch, and dispatchers by their plain names.
--
-- `flavour(cb)` finds out once, by evaluating a chunk that does nothing: a
-- Lua-configured Hyprland answers "ok". `set_flavour` pins it (tests).
--
-- Every change is first a *plan*, a list of steps
--   { eval = "hl.config(...)" }              one `/eval`
--   { keyword = "input:kb_layout", value = "us,de" }
--   { dispatch = "workspace", arg = "3" }
--   { request = "/setcursor name 24" }       a raw command
-- built by a pure function (`options_plan`, `animation_plan`,
-- `monitors_plan`, `rehome_plan`) that can be tested without a compositor,
-- and then `run(plan, cb)` sends it: evals one by one, everything else
-- batched on one connection. Values are validated before they reach a
-- command, so a setting file cannot become an arbitrary `eval`; a plan with
-- anything invalid in it is refused whole (`nil, why`).
--
-- Under anything but Hyprland `available()` is false and `run` answers
-- `cb(false, "unavailable")` without sending anything.

local morf = require("morf")

local M = {}

local hyprland = require("lib.hyprland")
local flavour = nil

--- Talk through another `lib.hyprland`-shaped table (a test double).
function M.use(lib)
  hyprland = lib
  flavour = nil
end

function M.available()
  return hyprland ~= nil and hyprland.available ~= nil and hyprland.available() == true
end

-- ------------------------------------------------------------- flavour --

local waiting = nil

--- `cb("lua" | "hyprlang" | nil)`: which language the running Hyprland is
--- configured in; nil when there is no Hyprland to ask.
function M.flavour(cb)
  if flavour then return cb(flavour) end
  if not M.available() then return cb(nil) end
  if waiting then waiting[#waiting + 1] = cb return end
  waiting = { cb }
  local tries = 0
  local function ask()
    hyprland.eval("local _ = 0", function(reply)
      if type(reply) ~= "string" and tries < 10 and M.available() then
        -- Asked before the library had started, or while the compositor
        -- was busy: ask again shortly rather than give up for good.
        tries = tries + 1
        morf.timer(300, ask, false)
        return
      end
      local list = waiting
      waiting = nil
      if type(reply) == "string" then
        flavour = reply:match("^%s*ok%s*$") and "lua" or "hyprlang"
      end
      for _, each in ipairs(list) do each(flavour) end
    end)
  end
  ask()
end

--- The flavour last found, or nil.
function M.known_flavour() return flavour end

--- Pins the flavour ("lua" or "hyprlang"), or forgets it (nil).
function M.set_flavour(name) flavour = name end

-- A compositor restarted may be configured differently.
if hyprland and hyprland.on then
  hyprland.on("connected", function() flavour = nil end)
end

-- ----------------------------------------------------------- validation --

local function trim_number(n)
  if n == math.floor(n) and math.abs(n) < 1e9 then return string.format("%d", n) end
  local s = string.format("%.4f", n):gsub("0+$", ""):gsub("%.$", "")
  return s
end
M.number = trim_number

-- What a string value may hold: layouts ("us,de"), xkb options
-- ("grp:alt_shift_toggle"), names. Nothing that ends a Lua string or a
-- batched command.
local PLAIN = "^[%w_,:+%-%.]*$"

--- A value checked against its kind, as `{ lua = <literal>, conf = <text> }`,
--- or nil when it does not pass. Kinds: bool, int, float, str, colour.
function M.value(kind, value)
  if kind == "bool" then
    if value == true or value == "true" or value == 1 or value == "1" then
      return { lua = "true", conf = "true" }
    end
    if value == false or value == "false" or value == 0 or value == "0" then
      return { lua = "false", conf = "false" }
    end
    return nil
  end
  if kind == "int" or kind == "float" then
    local n = tonumber(value)
    if not n or n ~= n or math.abs(n) > 1e6 then return nil end
    if kind == "int" then n = math.floor(n + 0.5) end
    local text = trim_number(n)
    return { lua = text, conf = text }
  end
  if kind == "str" then
    local text = tostring(value or "")
    if #text > 256 or not text:match(PLAIN) then return nil end
    return { lua = '"' .. text .. '"', conf = text }
  end
  if kind == "colour" then
    local text = tostring(value or "")
    if text:match("^rgba%(%x%x%x%x%x%x%x%x%)$") or text:match("^rgb%(%x%x%x%x%x%x%)$") then
      return { lua = '"' .. text .. '"', conf = text }
    end
    return nil
  end
  return nil
end

--- `decoration:blur:size` and `4` into `{ decoration = { blur = { size = 4 } } }`.
function M.nest(path, literal)
  local keys = {}
  for part in tostring(path):gmatch("[^:%.]+") do
    if not part:match("^[%a_][%w_]*$") then return nil end
    keys[#keys + 1] = part
  end
  if #keys == 0 then return nil end
  local body = literal
  for index = #keys, 1, -1 do body = "{ " .. keys[index] .. " = " .. body .. " }" end
  return body
end

-- --------------------------------------------------------------- options --

--- A plan setting options: `list` is `{ { option, kind, value } }`. One
--- eval of `hl.config` calls (lua), or one keyword each (hyprlang).
function M.options_plan(list, how)
  local chunks, steps = {}, {}
  for _, entry in ipairs(list or {}) do
    local option, kind, value = entry[1], entry[2], entry[3]
    local checked = M.value(kind, value)
    if not checked then return nil, "refusing " .. tostring(option) .. " = " .. tostring(value) end
    if how == "lua" then
      local nested = M.nest(option, checked.lua)
      if not nested then return nil, "not an option path: " .. tostring(option) end
      chunks[#chunks + 1] = "hl.config(" .. nested .. ")"
    else
      if not tostring(option):match("^[%w_:%.%-]+$") then return nil, "not an option path: " .. tostring(option) end
      steps[#steps + 1] = { keyword = option, value = checked.conf }
    end
  end
  if how == "lua" and #chunks > 0 then steps[1] = { eval = table.concat(chunks, " ") } end
  return steps
end

-- ------------------------------------------------------------ animations --

--- A plan for animations. `spec` is `{ enabled, curve = { name, points =
--- { x1, y1, x2, y2 } }, leaves = { { name, enabled, speed, style } } }`;
--- a leaf's speed is in deciseconds, as Hyprland counts it. One eval (lua),
--- so no window animates on a mix of old and new curves.
function M.animation_plan(spec, how)
  spec = spec or {}
  local enabled = spec.enabled ~= false
  local lines, steps = {}, {}
  local curve = spec.curve
  local name = curve and curve.name or "preset"
  if not tostring(name):match("^[%a_][%w_]*$") then return nil, "bad curve name" end
  local points = {}
  if curve then
    for index = 1, 4 do
      local n = tonumber(curve.points and curve.points[index])
      if not n then return nil, "bad curve" end
      points[index] = trim_number(n)
    end
  end
  if how == "lua" then
    lines[#lines + 1] = "hl.config({ animations = { enabled = " .. tostring(enabled) .. " } })"
  else
    steps[#steps + 1] = { keyword = "animations:enabled", value = enabled and "true" or "false" }
  end
  if enabled then
    if curve then
      if how == "lua" then
        lines[#lines + 1] = string.format('hl.curve("%s", { type = "bezier", points = { {%s, %s}, {%s, %s} } })',
          name, points[1], points[2], points[3], points[4])
      else
        steps[#steps + 1] = { keyword = "bezier", value = name .. "," .. table.concat(points, ",") }
      end
    end
    for _, leaf in ipairs(spec.leaves or {}) do
      if not tostring(leaf.name or ""):match("^[%a][%w]*$") then return nil, "bad leaf" end
      local style = leaf.style
      if style ~= nil and not tostring(style):match("^[%w %%]+$") then return nil, "bad style" end
      if leaf.enabled == false then
        if how == "lua" then
          lines[#lines + 1] = string.format('hl.animation({ leaf = "%s", enabled = false })', leaf.name)
        else
          steps[#steps + 1] = { keyword = "animation", value = leaf.name .. ",0" }
        end
      else
        local speed = tonumber(leaf.speed)
        if not speed then return nil, "bad speed" end
        if how == "lua" then
          lines[#lines + 1] = string.format('hl.animation({ leaf = "%s", enabled = true, speed = %s, bezier = "%s"%s })',
            leaf.name, trim_number(speed), name, style and (', style = "' .. style .. '"') or "")
        else
          steps[#steps + 1] = { keyword = "animation",
            value = table.concat({ leaf.name, "1", trim_number(speed), name, style }, ",") }
        end
      end
    end
  end
  if how == "lua" then return { { eval = table.concat(lines, " ") } } end
  return steps
end

-- -------------------------------------------------------------- monitors --

-- An output: a connector name or `desc:` and an EDID description, which may
-- carry spaces, dots and brackets.
local function output_ok(text, how)
  text = tostring(text or "")
  if #text == 0 or #text > 256 then return false end
  if not text:match("^[%w %._:%+%(%)%[%]/#%-,]+$") then return false end
  -- A comma ends a field of a hyprlang rule.
  if how ~= "lua" and text:find(",", 1, true) then return false end
  return true
end

local function mode_ok(text)
  text = tostring(text or "")
  if text == "preferred" or text == "highres" or text == "highrr" or text == "maxwidth" then return true end
  local w, h, rest = text:match("^(%d%d?%d?%d?%d?)x(%d%d?%d?%d?%d?)(.*)$")
  if not w then return false end
  return rest == "" or rest:match("^@%d+%.?%d*$") ~= nil
end

local function position_ok(text)
  text = tostring(text or "")
  if text:match("^auto") then
    return text == "auto" or text == "auto-left" or text == "auto-right" or text == "auto-up" or text == "auto-down"
  end
  return text:match("^%-?%d+x%-?%d+$") ~= nil
end

local function scale_of(value)
  if value == "auto" then return "auto" end
  local n = tonumber(value)
  if not n or n < 0.1 or n > 10 then return nil end
  return trim_number(n)
end

local ORDER = { "output", "disabled", "mode", "position", "scale", "transform", "mirror", "vrr" }
local KNOWN = {}
for _, field in ipairs(ORDER) do KNOWN[field] = true end

--- One rule as `hl.monitor{...}` (lua) or the text after `keyword monitor`
--- (hyprlang); nil and why when a field is unknown or invalid. All or
--- nothing: a rule with a field dropped would put a screen somewhere it was
--- not meant to go. A hyprlang rule needs mode, position and scale, or
--- `disabled`.
function M.monitor_rule(rule, how)
  if type(rule) ~= "table" or rule.output == nil then return nil, "a rule needs an output" end
  for field in pairs(rule) do
    if not KNOWN[field] then return nil, "unknown field " .. tostring(field) end
  end
  if not output_ok(rule.output, how) then return nil, "bad output" end
  if rule.mirror ~= nil and rule.mirror ~= "none" and not output_ok(rule.mirror, how) then return nil, "bad mirror" end
  if rule.mode ~= nil and not mode_ok(rule.mode) then return nil, "bad mode " .. tostring(rule.mode) end
  if rule.position ~= nil and not position_ok(rule.position) then return nil, "bad position" end
  if rule.scale ~= nil and not scale_of(rule.scale) then return nil, "bad scale" end
  local transform = rule.transform ~= nil and tonumber(rule.transform) or nil
  if rule.transform ~= nil and (not transform or transform ~= math.floor(transform) or transform < 0 or transform > 7) then
    return nil, "bad transform"
  end
  local vrr = rule.vrr ~= nil and tonumber(rule.vrr) or nil
  if rule.vrr ~= nil and (not vrr or vrr ~= math.floor(vrr) or vrr < 0 or vrr > 3) then return nil, "bad vrr" end
  if rule.disabled ~= nil and type(rule.disabled) ~= "boolean" then return nil, "bad disabled" end

  if how == "lua" then
    local pairs_out = {}
    for _, field in ipairs(ORDER) do
      local v = rule[field]
      if v ~= nil then
        local literal
        if field == "disabled" then literal = tostring(v)
        elseif field == "scale" then
          local s = scale_of(v)
          literal = s == "auto" and '"auto"' or s
        elseif field == "transform" then literal = tostring(transform)
        elseif field == "vrr" then literal = tostring(vrr)
        else literal = '"' .. tostring(v) .. '"' end
        pairs_out[#pairs_out + 1] = field .. " = " .. literal
      end
    end
    return "hl.monitor({ " .. table.concat(pairs_out, ", ") .. " })"
  end

  if rule.disabled == true then return rule.output .. ",disable" end
  if rule.mode == nil or rule.position == nil or rule.scale == nil then
    return nil, "a hyprlang rule needs mode, position and scale"
  end
  local parts = { rule.output, rule.mode, rule.position, scale_of(rule.scale) }
  if transform and transform ~= 0 then parts[#parts + 1] = "transform," .. transform end
  if rule.mirror ~= nil and rule.mirror ~= "none" then parts[#parts + 1] = "mirror," .. rule.mirror end
  if vrr then parts[#parts + 1] = "vrr," .. vrr end
  return table.concat(parts, ",")
end

--- A plan applying monitor rules together. Disabled outputs go first, so
--- the ones staying are never placed around one that is leaving (`auto`
--- resolves against what the previous rule left) -- unless the same plan
--- also lights one, which then goes first: Hyprland must never pass through
--- zero lit outputs.
function M.monitors_plan(rules, how)
  local lighting, darkening = false, false
  for _, rule in ipairs(rules or {}) do
    if type(rule) == "table" and rule.disabled == false then lighting = true end
    if type(rule) == "table" and rule.disabled == true then darkening = true end
  end
  local off_first = not (lighting and darkening)
  local ordered = {}
  for index, rule in ipairs(rules or {}) do ordered[#ordered + 1] = { rule = rule, index = index } end
  local function rank(entry)
    local off = type(entry.rule) == "table" and entry.rule.disabled == true
    if off_first then return off and 0 or 1 end
    return off and 1 or 0
  end
  table.sort(ordered, function(a, b)
    local ra, rb = rank(a), rank(b)
    if ra ~= rb then return ra < rb end
    return a.index < b.index
  end)
  local chunks, steps = {}, {}
  for _, entry in ipairs(ordered) do
    local made, why = M.monitor_rule(entry.rule, how)
    if not made then return nil, why end
    if how == "lua" then chunks[#chunks + 1] = made
    else steps[#steps + 1] = { keyword = "monitor", value = made } end
  end
  if how == "lua" then
    if #chunks == 0 then return {} end
    return { { eval = table.concat(chunks, " ") } }
  end
  return steps
end

-- How `availableModes` spells a mode: "2560x1440@144.00Hz".
local function parse_mode(text)
  local w, h, r = tostring(text):match("^%s*(%d+)x(%d+)@([%d%.]+)Hz%s*$")
  if not w then return nil end
  return tonumber(w), tonumber(h), math.floor(tonumber(r) * 100 + 0.5) / 100
end

--- The modes, deduplicated, largest and fastest first:
--- `{ { mode = "2560x1440@144.00", width, height, refresh } }`.
function M.parse_modes(list)
  local seen, out = {}, {}
  for _, text in ipairs(list or {}) do
    local w, h, r = parse_mode(text)
    if w then
      local name = string.format("%dx%d@%.2f", w, h, r)
      if not seen[name] then
        seen[name] = true
        out[#out + 1] = { mode = name, width = w, height = h, refresh = r }
      end
    end
  end
  table.sort(out, function(a, b)
    if a.width * a.height ~= b.width * b.height then return a.width * a.height > b.width * b.height end
    return a.refresh > b.refresh
  end)
  return out
end

--- The modes grouped by resolution: `{ { width, height, refreshes } }`,
--- refreshes fastest first.
function M.group_modes(modes)
  local groups, order = {}, {}
  for _, mode in ipairs(modes or {}) do
    local key = mode.width .. "x" .. mode.height
    if not groups[key] then
      groups[key] = { width = mode.width, height = mode.height, refreshes = {} }
      order[#order + 1] = groups[key]
    end
    table.insert(groups[key].refreshes, mode.refresh)
  end
  for _, group in ipairs(order) do table.sort(group.refreshes, function(a, b) return a > b end) end
  table.sort(order, function(a, b) return a.width * a.height > b.width * b.height end)
  return order
end

--- A `lib.hyprland` output row in the words rules are compared in: `mode`
--- as `availableModes` spells it, `position` "XxY", `scale` rounded (1.25
--- is reported as 1.2000000476837158), `mirror` the mirrored output's name
--- or "none", `vrr` 0/1, plus `modes` and `resolutions`.
function M.describe(row, names)
  local width, height = row.width or 0, row.height or 0
  local refresh = math.floor((row.refresh_rate or 0) * 100 + 0.5) / 100
  local modes = M.parse_modes(row.available_modes)
  local out = {}
  for key, value in pairs(row) do out[key] = value end
  out.refresh = refresh
  out.mode = (width > 0 and height > 0 and not row.disabled)
    and string.format("%dx%d@%.2f", width, height, refresh) or "preferred"
  out.position = string.format("%dx%d", row.x or 0, row.y or 0)
  out.scale = math.floor((row.scale or 1) * 1000 + 0.5) / 1000
  out.transform = row.transform or 0
  out.vrr = row.vrr and 1 or 0
  out.mirror = (names and names[row.mirror_of]) or "none"
  out.workspace = row.active_workspace or 0
  out.modes = modes
  out.resolutions = M.group_modes(modes)
  return out
end

--- Every output, described, from the library's current state.
function M.outputs()
  if not M.available() or not hyprland.outputs then return {} end
  local rows = hyprland.outputs()
  local names = {}
  for _, row in ipairs(rows) do if row.id and row.id >= 0 then names[row.id] = row.name end end
  local out = {}
  for index, row in ipairs(rows) do out[index] = M.describe(row, names) end
  return out
end

-- ------------------------------------------------------------ workspaces --

--- A plan bringing workspaces back where they can be seen. `lit` is the
--- lit outputs' `{ name, workspace }`, `workspaces` the rows `{ id, monitor,
--- windows }`, `home` where stranded ones go. Fixes two things: workspaces
--- with windows on an output that is gone or dark, and a lit output showing
--- an empty workspace while one of its others holds windows. Special
--- workspaces (negative ids) are never moved.
function M.rehome_plan(lit, workspaces, home, how)
  if #lit == 0 then return {} end
  local names = {}
  for _, m in ipairs(lit) do names[m.name] = true end
  if not names[home] then home = lit[1].name end
  local real = {}
  for _, row in ipairs(workspaces or {}) do if (row.id or 0) > 0 then real[#real + 1] = row end end
  table.sort(real, function(a, b) return a.id < b.id end)
  local stranded = {}
  for _, row in ipairs(real) do
    if (row.windows or 0) > 0 and not names[row.monitor] then stranded[#stranded + 1] = row end
  end
  local lua, steps = {}, {}
  local function move(id, monitor)
    if how == "lua" then
      lua[#lua + 1] = string.format('hl.dispatch(hl.dsp.workspace.move({ workspace = %d, monitor = "%s" }))', id, monitor)
    else
      steps[#steps + 1] = { dispatch = "moveworkspacetomonitor", arg = id .. " " .. monitor }
    end
  end
  local function show(monitor, id)
    -- Two calls: focusing a workspace acts on the focused monitor.
    if how == "lua" then
      lua[#lua + 1] = string.format('hl.dispatch(hl.dsp.focus({ monitor = "%s" }))', monitor)
      lua[#lua + 1] = string.format("hl.dispatch(hl.dsp.focus({ workspace = %d }))", id)
    else
      steps[#steps + 1] = { dispatch = "focusmonitor", arg = monitor }
      steps[#steps + 1] = { dispatch = "workspace", arg = tostring(id) }
    end
  end
  for _, row in ipairs(stranded) do
    if not output_ok(home, "hyprlang") then return nil, "bad monitor name" end
    move(row.id, home)
  end
  for _, m in ipairs(lit) do
    local active, carrying = nil, nil
    for _, row in ipairs(real) do
      if row.monitor == m.name then
        if row.id == m.workspace then active = row end
        if not carrying and (row.windows or 0) > 0 then carrying = row end
      end
    end
    if not (active and (active.windows or 0) > 0) then
      local wanted = (m.name == home and stranded[1]) or carrying
      if wanted and output_ok(m.name, "hyprlang") then show(m.name, wanted.id) end
    end
  end
  if how == "lua" then
    if #lua == 0 then return {} end
    return { { eval = table.concat(lua, " ") } }
  end
  return steps
end

-- ----------------------------------------------------------------- misc --

--- A plan setting the cursor theme and size (`hyprctl setcursor`), which
--- reaches clients as well as the compositor.
function M.cursor_plan(theme_name, size)
  theme_name = tostring(theme_name or "")
  local n = tonumber(size)
  if not theme_name:match("^[%w_%.%-]+$") or not n or n < 8 or n > 256 then return nil, "bad cursor" end
  return { { request = "/setcursor " .. theme_name .. " " .. math.floor(n) } }
end

--- The modifiers `modmask` carries, in the order binds are spelled.
M.MODIFIERS = { { bit = 64, name = "SUPER" }, { bit = 4, name = "CTRL" }, { bit = 8, name = "ALT" }, { bit = 1, name = "SHIFT" } }

--- A bind as `hl.bind` spells it: "SUPER + SHIFT + T".
function M.spell(bind)
  local parts = {}
  local mask = tonumber(bind.modmask) or 0
  for _, m in ipairs(M.MODIFIERS) do
    if mask & m.bit ~= 0 then parts[#parts + 1] = m.name end
  end
  local key = tostring(bind.key or "")
  parts[#parts + 1] = key ~= "" and key or ("code:" .. tostring(bind.keycode or 0))
  return table.concat(parts, " + ")
end

-- ---------------------------------------------------------------- sending --

-- Each command sent, for a test to read: `M.sent` keeps the last 200.
M.sent = {}
local function note(command)
  M.sent[#M.sent + 1] = command
  if #M.sent > 200 then table.remove(M.sent, 1) end
  morf.log("debug", "hyprland_config: sending " .. command:sub(1, 400))
end

local function payload(step)
  if step.eval then return "/eval " .. step.eval end
  if step.keyword then return "/keyword " .. step.keyword .. " " .. step.value end
  if step.dispatch then
    return "/dispatch " .. step.dispatch .. ((step.arg and step.arg ~= "") and (" " .. step.arg) or "")
  end
  return step.request
end
M.payload = payload

--- Sends a plan. `cb(ok, replies)` once every step is answered: ok when
--- each said "ok". Evals go alone (a chunk may hold `;`); the rest share a
--- batch.
function M.run(plan, cb)
  cb = cb or function(ok, replies)
    if not ok then morf.log("warn", "hyprland_config: " .. tostring(replies and table.concat(replies, " | ") or "failed")) end
  end
  if not M.available() then return cb(false, { "unavailable" }) end
  if type(plan) ~= "table" or #plan == 0 then return cb(true, {}) end
  local replies, all_ok, pending = {}, true, 0
  local batch = {}
  local function done()
    pending = pending - 1
    if pending == 0 then cb(all_ok, replies) end
  end
  local function take(reply)
    reply = tostring(reply or "no reply")
    replies[#replies + 1] = reply
    -- A batch answers "ok" per command, separated by blank lines.
    for line in reply:gmatch("[^\n]+") do
      if not line:match("^%s*ok%s*$") then all_ok = false end
    end
    if reply:match("^%s*$") then all_ok = false end
  end
  local sends = {}
  for _, step in ipairs(plan) do
    local command = payload(step)
    if step.eval then sends[#sends + 1] = { command }
    else batch[#batch + 1] = command end
  end
  if #batch == 1 then sends[#sends + 1] = { batch[1] }
  elseif #batch > 1 then sends[#sends + 1] = { "[[BATCH]]" .. table.concat(batch, ";") } end
  pending = #sends
  for _, send in ipairs(sends) do
    note(send[1])
    hyprland.request(send[1], function(reply, err)
      take(reply or err)
      done()
    end)
  end
end

--- Builds a plan for the running flavour and sends it:
--- `apply(function(how) return M.options_plan(list, how) end, cb)`.
function M.apply(build, cb)
  cb = cb or function() end
  M.flavour(function(how)
    if not how then return cb(false, { "unavailable" }) end
    local plan, why = build(how)
    if not plan then
      morf.log("warn", "hyprland_config: refused: " .. tostring(why))
      return cb(false, { tostring(why) })
    end
    M.run(plan, cb)
  end)
end

--- `cb(true|false)`: whether a plugin's option exists, i.e. the plugin is
--- loaded ("plugin:dynamic_cursors:shake:enabled").
function M.plugin_available(option, cb)
  if not M.available() then return cb(false) end
  hyprland.getoption(option, function(value) cb(type(value) == "table" and value.option ~= nil) end)
end

--- Reloads the configuration (dropping whatever was pushed at run time).
function M.reload(cb)
  if not M.available() then if cb then cb(false, "unavailable") end return end
  note("/reload")
  hyprland.reload(false, cb)
end

--- `fn()` after every reload of the configuration. Returns a handle with
--- `:off()`.
function M.on_reload(fn)
  if not hyprland or not hyprland.on then return { off = function() end } end
  return hyprland.on("configreloaded", fn)
end

return M
