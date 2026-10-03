-- Text fields (the TextField archetype) around the engine's text input.
--
--     local node, input = text_field.make("entry", {
--       id = "search", width = 300, height = 40, placeholder = "Search",
--       on_accepted = run, validator = "number", clear = true,
--     })
--
-- `node` is the control, the one to place; `input` the `ui.TextInput`
-- inside it, which keeps the spec's `id` and every text input property
-- (`placeholder`, `font_size`, `on_text_changed`, ...). The archetype
-- validates (`validator`, `minimum`, `maximum`, `required`,
-- `max_length`), reverts on Escape when asked (`revert_on_escape`), and
-- reveals a password; the skin draws the field around the input
-- (`background`, `placeholder`, `trailing` -- a clear or reveal press --,
-- `counter`, `error`). `inset` (px, or `{ l, t, r, b }`) keeps the input
-- clear of the skin's edges. `on_edited(text)`, `on_accepted(text)` --
-- only when acceptable -- and `on_invalid(text)` follow it.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

-- What belongs to the control, not the input.
local CONTROL = { x = true, y = true, z = true, anchors = true, visible = true, opacity = true, layout = true,
  width = true, height = true }
-- The archetype's settings and handlers, which the input does not take.
local SETTINGS = { text = false, placeholder = false, echo = true, read_only = false, max_length = false,
  validator = true, minimum = true, maximum = true, required = true, revert_on_escape = true, inset = true,
  clear = true, reveal = true, widget = true, on_accepted = true, on_escape = true, on_edited = true,
  on_invalid = true, on_text_changed = true, on_focus_changed = true }

local function sides(v)
  if type(v) == "table" then return v[1] or 0, v[2] or 0, v[3] or 0, v[4] or 0 end
  v = v or 0
  return v, v, v, v
end

-- What each widget is unless its spec says otherwise.
local DEFAULTS = {
  password = { echo = "password", reveal = true },
  url = { validator = "url" }, email = { validator = "email" },
  numeric_entry = { validator = "number" }, search = { clear = true },
}

function M.make(widget, spec)
  local given = spec or {}
  spec = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do spec[k] = v end
  for k, v in pairs(given) do spec[k] = v end
  local input_props, control_props = {}, {}
  for k, v in pairs(spec) do
    if type(k) == "number" then input_props[k] = v
    elseif CONTROL[k] then control_props[k] = v
    elseif SETTINGS[k] == nil or SETTINGS[k] == false then input_props[k] = v end
  end
  local l, t_, r, b = sides(spec.inset)
  input_props.anchors = { fill = true, left_margin = l, top_margin = t_, right_margin = r, bottom_margin = b }
  if spec.echo == "password" then input_props.password = true end
  local root, t, ctl
  local input
  input_props.on_text_changed = function(text)
    if ctl then ctl.send("edited", text) end
    if spec.on_text_changed then spec.on_text_changed(text) end
  end
  input_props.on_accepted = function(text)
    if ctl then ctl.send("accepted") elseif spec.on_accepted then spec.on_accepted(text) end
  end
  input_props.on_escape = function(...)
    if ctl then ctl.send("escape") end
    if spec.on_escape then spec.on_escape(...) end
  end
  input_props.on_focus_changed = function(on)
    if ctl then ctl.send("focus", on, input and input.visual_focus or false) end
    if spec.on_focus_changed then spec.on_focus_changed(on) end
  end
  input = ui.TextInput(input_props)
  local settings = { widget = widget }
  for k, v in pairs(spec) do if SETTINGS[k] ~= nil and type(k) == "string" then settings[k] = v end end
  settings.id = spec.id and (spec.id .. "-field") or nil
  -- What the archetype raises that the input or the configuration acts on.
  settings.on_set_text = function(text) input.text = text end
  settings.on_accepted = function(text) if spec.on_accepted then spec.on_accepted(text) end end
  settings.on_edited = spec.on_edited
  settings.on_invalid = spec.on_invalid
  settings.input = input
  control_props.focus_policy = "none"
  root, t, ctl = control.make("TextField", widget, settings, { children = { input }, props = control_props })
  -- A revealed password shows its text.
  morf.effect("kit.text_field.echo." .. ctl.id, function()
    if spec.echo == "password" then input.password = t.echo == "password" end
  end, { owner = root })
  return root, input, t, ctl
end

return M
