-- The screens, and the arrangement kept for each set of them. Port of
-- MonitorService.qml and monitors.py.
--
-- Every output Hyprland has, lit or not, comes from lib/hyprland's
-- `monitors all` (described by lib/hyprland_config). The arrangement lives
-- in impasto's settings (`displays`), keyed by the set of monitors present,
-- so the right one comes back by itself on hotplug; a shut lid is the same
-- set with one screen disabled. Monitors are known by description (make,
-- model, serial), which is what Hyprland's `desc:` matches: connector names
-- change between ports. A set never arranged is left to Hyprland's own
-- rules, so on a fresh install nothing is pushed.
--
-- Rules go out as one plan (lib/hyprland_config's `monitors_plan`), and
-- only those that disagree with what the compositor reports, which is also
-- the loop guard: lighting a screen fires `monitoradded`, which re-reads,
-- which then finds nothing left to change. The configuration files are
-- never written; a reload drops the rules and the next read pushes them
-- again. Never leaves the machine with no lit screen (`recover`), and moves
-- workspaces stranded by a hotplug back where they can be seen (`rescue`).
--
-- Under any other compositor `available()` is false: the settings page
-- shows the screens morf knows, read-only.

local settings = require("services.settings")
local act = require("services.act")
local live = require("services.live")

local M = {}

local s = {
  revision = morf.signal("impasto.displays.revision", 0),
}
M.signals = s

local config, hyprland
local monitors = {}

function M.available()
  s.revision:get()
  return config ~= nil and config ~= false and config.available()
end

--- Every screen plugged in, lit or not, described (see
--- lib/hyprland_config `describe`). Tracks in a binding.
function M.monitors()
  s.revision:get()
  return monitors
end
function M.revision() return s.revision:get() end

--- A monitor's key: its description, or its name when it reports none.
function M.key(m) return (m.description ~= nil and m.description ~= "") and m.description or m.name end

local function output_of(m)
  return (m.description ~= nil and m.description ~= "") and ("desc:" .. m.description) or m.name
end

function M.find(key)
  for _, m in ipairs(monitors) do if M.key(m) == key then return m end end
  return nil
end

function M.by_name(name)
  for _, m in ipairs(monitors) do if m.name == name then return m end end
  return nil
end

-- ------------------------------------------------------------ the store --

--- The set's key: descriptions sorted, so enumeration order does not
--- matter.
function M.profile()
  local keys = {}
  for _, m in ipairs(M.monitors()) do keys[#keys + 1] = M.key(m) end
  table.sort(keys)
  return table.concat(keys, " · ")
end

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = copy(v) end
  return out
end

--- This set's arrangement, or nil. `peek` reads it without tracking.
function M.arrangement(peek)
  local store = (peek and settings.peek("displays") or settings.displays) or {}
  local kept = store[M.profile()]
  return type(kept) == "table" and kept or nil
end

function M.arranged() return M.arrangement() ~= nil end
function M.mirroring() local a = M.arrangement() return a ~= nil and a.mirror == true end

--- The shell's main screen (the one mirroring copies), by name: the
--- chosen one while lit, else the focused or first lit screen.
function M.primary_name(peek)
  local a = M.arrangement(peek)
  local wanted = a and a.primary and M.find(a.primary)
  if wanted and not wanted.disabled then return wanted.name end
  local first
  for _, m in ipairs(M.monitors()) do
    if not m.disabled then
      if m.focused then return m.name end
      first = first or m
    end
  end
  return first and first.name or ""
end

-- Writes into this set's arrangement.
local function change(fn)
  local store = copy(settings.displays or {})
  local profile = M.profile()
  if profile == "" then return end
  local kept = type(store[profile]) == "table" and store[profile] or { primary = "", mirror = false, monitors = {} }
  kept.monitors = type(kept.monitors) == "table" and kept.monitors or {}
  fn(kept)
  store[profile] = kept
  settings.set("displays", store)
end

--- Whether `fields` could be pushed (lib/hyprland_config's checks).
function M.valid(fields)
  if not config then return false end
  local rule = { output = "check" }
  for field, value in pairs(fields) do rule[field] = value end
  return config.monitor_rule(rule, "lua") ~= nil
end

--- Merges `fields` into one screen's rule; refused when they could not be
--- pushed, so a bad value never blocks the rest of the arrangement.
function M.remember(key, fields)
  if not M.valid(fields) then
    morf.log("warn", "impasto: not keeping an invalid screen rule for " .. tostring(key))
    return false
  end
  change(function(kept)
    local rule = copy(kept.monitors[key] or {})
    for field, value in pairs(fields) do rule[field] = value end
    kept.monitors[key] = rule
  end)
end

--- Every screen's position at once: `{ [key] = { x, y } }`.
function M.remember_positions(places)
  change(function(kept)
    for key, at in pairs(places) do
      local rule = copy(kept.monitors[key] or {})
      rule.position = string.format("%dx%d", math.floor(at.x + 0.5), math.floor(at.y + 0.5))
      kept.monitors[key] = rule
    end
  end)
end

function M.remember_mirror(on) change(function(kept) kept.mirror = on == true end) end
function M.remember_primary(key) change(function(kept) kept.primary = key end) end

--- Hands this set back to Hyprland's own rules; reloads, since Hyprland
--- keeps whatever was last pushed.
function M.forget()
  local store = copy(settings.displays or {})
  store[M.profile()] = nil
  settings.set("displays", store)
  if M.available() and live.here() then
    act.run("reload Hyprland to forget the arrangement", function() config.reload() return true end)
  end
end

-- ------------------------------------------------------------- applying --

--- The kept rule, with mirroring: a mirrored screen gets no position.
function M.effective_rule(key)
  local a = M.arrangement()
  local kept = a and a.monitors and a.monitors[key]
  if type(kept) ~= "table" then return nil end
  local rule = copy(kept)
  local found = M.find(key)
  local primary = M.primary_name()
  if a.mirror == true and primary ~= "" and found and found.name ~= primary then
    rule.mirror = primary
    rule.position = nil
  else
    rule.mirror = "none"
  end
  return rule
end

--- Whether a rule disagrees with what the compositor reports.
function M.differs(key, rule)
  local found = M.find(key)
  if not found then return false end
  -- Only when the rule sets it: a rule without `disabled` must not light
  -- the laptop panel right after its lid closed.
  if rule.disabled ~= nil and rule.disabled ~= found.disabled then return true end
  if found.disabled then return false end
  return (rule.mode ~= nil and rule.mode ~= found.mode)
    or (rule.position ~= nil and rule.position ~= found.position)
    or (rule.scale ~= nil and math.abs((tonumber(rule.scale) or 0) - found.scale) > 0.001)
    or (rule.transform ~= nil and tonumber(rule.transform) ~= found.transform)
    or (rule.mirror ~= nil and rule.mirror ~= found.mirror)
    or (rule.vrr ~= nil and tonumber(rule.vrr) ~= found.vrr)
end

--- A full rule for `m` from a partial one: what it leaves out is what the
--- screen has now, so a hyprlang rule (which needs all three) keeps them.
local function complete(m, rule)
  local out = { output = output_of(m) }
  for field, value in pairs(rule) do out[field] = value end
  if out.disabled == true then
    return { output = out.output, disabled = true }
  end
  if out.mode == nil then out.mode = m.disabled and "preferred" or m.mode end
  if out.position == nil and out.mirror == "none" then out.position = m.disabled and "auto" or m.position end
  if out.position == nil then out.position = "auto" end
  if out.scale == nil then out.scale = m.scale end
  if out.mirror == "none" then out.mirror = nil end
  return out
end
M.complete = complete

local settling = nil
local disturbed = false
local rescue

local warned_all_off = nil

--- The rules this set needs now, or {}.
function M.pending_rules()
  local a = M.arrangement()
  if not a or type(a.monitors) ~= "table" then return {} end
  local keys = {}
  for key in pairs(a.monitors) do keys[#keys + 1] = key end
  table.sort(keys)
  local rules = {}
  for _, key in ipairs(keys) do
    local rule = M.effective_rule(key)
    local found = M.find(key)
    if rule and found and M.differs(key, rule) then
      -- Mirroring back off needs `mirror = none` said out loud only when
      -- the screen mirrors now.
      if rule.mirror == "none" and found.mirror ~= "none" then
        local out = complete(found, rule)
        out.mirror = "none"
        rules[#rules + 1] = out
      else
        rules[#rules + 1] = complete(found, rule)
      end
    end
  end
  -- An arrangement that would leave nothing lit is not pushed as it is:
  -- its switching off is dropped, so it cannot fight `recover` forever.
  local lit = 0
  for _, m in ipairs(monitors) do
    local wanted = not m.disabled
    for _, rule in ipairs(rules) do
      if rule.output == output_of(m) and rule.disabled ~= nil then wanted = not rule.disabled end
    end
    if wanted then lit = lit + 1 end
  end
  if lit == 0 and #monitors > 0 then
    if warned_all_off ~= M.profile() then
      warned_all_off = M.profile()
      morf.log("warn", "impasto: the arrangement would switch every screen off; keeping them on")
    end
    -- The main screen stays on (or the first being switched off).
    local primary = M.find(((M.arrangement() or {}).primary) or "")
    local spare
    for index, rule in ipairs(rules) do
      if rule.disabled == true and (not spare or (primary and rule.output == output_of(primary))) then spare = index end
    end
    if spare then table.remove(rules, spare) end
  end
  return rules
end

local function send(what, rules)
  if #rules == 0 then return end
  -- Bringing an output up is a burst of events; nothing more is pushed
  -- until they settle, and the read after sees where things landed.
  if settling then settling:cancel() end
  settling = morf.timer(600, function()
    settling = nil
    disturbed = true
    if hyprland and hyprland.refresh then hyprland.refresh() end
  end, false)
  act.run(what, function()
    config.apply(function(how) return config.monitors_plan(rules, how) end, function(ok, replies)
      if not ok then
        morf.log("warn", "impasto: Hyprland did not take the monitor rules: "
          .. table.concat(replies or {}, " | "):sub(1, 200))
      end
    end)
    return true
  end)
end

function M.apply_profile()
  if not M.available() or not live.here() or settling then return end
  send("push the screen arrangement to Hyprland", M.pending_rules())
end

--- Never leave the machine with no lit screen: when nothing present is
--- lit, light everything. A dark screen beside a lit one is left alone,
--- since the lid does exactly that.
function M.recovery_rules()
  if #monitors == 0 then return nil end
  for _, m in ipairs(monitors) do if not m.disabled then return nil end end
  local rules = {}
  for _, m in ipairs(monitors) do
    rules[#rules + 1] = { output = output_of(m), disabled = false, mode = "preferred", position = "auto", scale = 1 }
  end
  return rules
end

local function recover()
  local rules = M.recovery_rules()
  if not rules then return false end
  morf.log("warn", "impasto: no screen is lit; putting them all back on")
  send("light every screen", rules)
  return true
end

rescue = function()
  if not hyprland or not live.here() then return end
  local lit = {}
  for _, m in ipairs(monitors) do
    if not m.disabled then lit[#lit + 1] = { name = m.name, workspace = m.workspace } end
  end
  local workspaces = (hyprland.snapshot().workspaces) or {}
  act.run("bring workspaces back to lit screens", function()
    config.apply(function(how) return config.rehome_plan(lit, workspaces, M.primary_name(), how) end)
    return true
  end)
end

-- What a view of the screens shows; the revision moves only when it does,
-- not on every workspace switch (which refetches the monitors too).
local function signature(list)
  local parts = {}
  for _, m in ipairs(list) do
    parts[#parts + 1] = table.concat({ m.name, M.key(m), m.mode, m.position, tostring(m.scale),
      tostring(m.transform), tostring(m.vrr), m.mirror, tostring(m.disabled), tostring(m.dpms),
      tostring(#m.modes) }, "|")
  end
  return table.concat(parts, "\n")
end
local seen_signature = nil

local function read()
  monitors = config.outputs()
  local now = signature(monitors)
  if now ~= seen_signature then
    seen_signature = now
    s.revision:set(s.revision:get() + 1)
  end
  if not live.here() or settling then return end
  if recover() then return end
  M.apply_profile()
  if disturbed then
    disturbed = false
    -- The workspaces answer follows the monitors'.
    morf.timer(300, rescue, false)
  end
end

-- ------------------------------------------------------------------ lid --

function M.internal()
  for _, m in ipairs(M.monitors()) do
    if m.name:match("^eDP") or m.name:match("^LVDS") or m.name:match("^DSI") then return m end
  end
  return nil
end

--- The lid, through the arrangement, so later pushes agree with it rather
--- than undo it. Returns whether it was handled here (Hyprland present).
function M.lid(closed)
  if not M.available() then return false end
  local panel = M.internal()
  if not panel or settings.lidPolicy == "system" then return true end
  local others = 0
  for _, m in ipairs(monitors) do if m ~= panel then others = others + 1 end end
  if others == 0 then return true end
  if closed and settings.lidPolicy ~= "off" then return true end
  -- Opening always lights the panel, whatever the policy.
  M.remember(M.key(panel), { disabled = closed })
  return true
end

-- ---------------------------------------------------------------- start --

function M.start()
  if config ~= nil then return end
  local ok, lib = pcall(require, "lib.hyprland_config")
  config = ok and lib or false
  if not config or not config.available() then return end
  hyprland = require("lib.hyprland")
  hyprland.on("refreshed", function(kind) if kind == "monitors" then read() end end)
  for _, name in ipairs { "configreloaded", "monitoradded", "monitoraddedv2", "monitorremoved", "monitorremovedv2" } do
    hyprland.on(name, function() disturbed = true end)
  end
  -- Canvas drags and the settings file arriving are one push once they
  -- rest.
  local pending = nil
  local first = true
  morf.effect("impasto.displays.store", function()
    local _ = settings.displays
    if first then first = false return end
    if pending then pending:cancel() end
    -- Long enough that a slider dragged is one push when it rests.
    pending = morf.timer(400, function() pending = nil M.apply_profile() end, false)
  end)
  monitors = config.outputs()
  seen_signature = signature(monitors)
  s.revision:set(s.revision:get() + 1)
end

return M
