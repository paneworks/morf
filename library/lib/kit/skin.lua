-- Skins: how a theme draws the archetypes (crates/morf-kit).
--
-- A theme defines skins by widget name -- `Button`, `Slider`, or the bare
-- archetype's name, `Control` -- each either a function of the control's
-- live state returning its slots, or a table of one function per slot:
--
--     skin.define("material", {
--       skins = {
--         Button = function(t, spec, node) return { background = ..., label = ... } end,
--         Switch = { indicator = function(t, spec) ... end },
--       },
--       defaults = { Press = { background = function(t, spec) ... end } },
--     }, { extends = "base" })
--
-- A slot comes from the first theme along the `extends` chain that fills
-- it for the widget, then from the first that fills it for the archetype,
-- then from the chain's `defaults` for the archetype -- built only when no
-- skin filled it. A skin may give a slot as a function instead of a node:
-- it is built the first time the control is entered, pressed or focused --
-- for decoration most controls never show (a hover wash, a focus ring). `skin.use(name)` switches theme: every live control
-- rebuilds its slots in place.
local M = {}

local themes = {}
local current = morf.signal("kit.skin.theme", "")
-- Every live control, so a theme switch can reach it.
local live = setmetatable({}, { __mode = "k" })

--- Defines (or replaces) a theme's skins.
function M.define(name, theme, options)
  theme = theme or {}
  themes[name] = {
    skins = theme.skins or {},
    defaults = theme.defaults or {},
    extends = options and options.extends or theme.extends,
  }
end

--- The theme whose skins draw now ("" before any `use`).
function M.current() return current:get() end

--- Whether a theme is defined.
function M.has(name) return themes[name] ~= nil end

--- The theme and those it extends, nearest first.
local function chain(name)
  local out, seen = {}, {}
  while name and themes[name] and not seen[name] do
    seen[name] = true
    out[#out + 1] = themes[name]
    name = themes[name].extends
  end
  return out
end

--- Builds the slots of a control: `widget` (its name), `archetype`,
--- `slots` (the archetype's slot names), `t` (its live state), `spec`
--- (what the configuration gave it), `node` (the control itself, for a
--- skin that hangs feedback on it), `send` (sends the control's archetype
--- an event: a clear button's `"clear"`). Returns `{ [slot] = node }`.
function M.build(widget, archetype, slots, t, spec, theme, node, send)
  local themes_chain = chain(theme or current:get())
  local built, made = {}, {}
  -- A whole-function skin is called once and gives several slots at once.
  local function from(skin, slot, key)
    if type(skin) == "table" then
      local fill = skin[slot]
      return fill and fill(t, spec, node, send) or nil
    elseif type(skin) == "function" then
      if made[key] == nil then made[key] = skin(t, spec, node, send) or false end
      return made[key] and made[key][slot] or nil
    end
  end
  for _, slot in ipairs(slots) do
    local filled
    for depth, theme_entry in ipairs(themes_chain) do
      filled = from(theme_entry.skins[widget], slot, "w" .. depth)
        or (widget ~= archetype and from(theme_entry.skins[archetype], slot, "a" .. depth)) or nil
      if filled then break end
    end
    if not filled then
      for _, theme_entry in ipairs(themes_chain) do
        local defaults = theme_entry.defaults[archetype]
        if defaults and defaults[slot] then filled = defaults[slot](t, spec, node, send) break end
      end
    end
    built[slot] = filled
  end
  -- Slots a whole-function skin built that a nearer theme filled instead.
  local kept = {}
  for _, node in pairs(built) do kept[node] = true end
  for _, result in pairs(made) do
    if result then
      for _, node in pairs(result) do
        if not kept[node] and type(node) ~= "function" then require("morf.ui").destroy(node, true) end
      end
    end
  end
  return built
end

--- Registers a live control: `rebuild()` is called on a theme switch.
function M.track(control, rebuild) live[control] = rebuild end
function M.untrack(control) live[control] = nil end

--- Switches theme: every live control rebuilds its slots.
function M.use(name)
  if themes[name] == nil then error("no skins defined for theme `" .. tostring(name) .. "`", 2) end
  if current:get() == name then return end
  current:set(name)
  local rebuilds = {}
  for control, rebuild in pairs(live) do rebuilds[#rebuilds + 1] = rebuild end
  for _, rebuild in ipairs(rebuilds) do rebuild() end
end

return M
