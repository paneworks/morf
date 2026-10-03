-- Gallery samples for the Form archetype's widgets (samples/init.lua):
-- each a form as an application would make it -- real kit fields
-- (widgets.entry, password, email, numeric_entry) with their validators,
-- a send -- caught in a state worth seeing: a sign-up tried with two
-- fields wrong, a sign-in the server turned down, a settings page that
-- saved itself, a subscribe row sending.
local ui = require("morf.ui")

local S = {}
S.span = { form = { 2, 2 }, login_form = { 1, 2 }, settings_form = { 2, 1 }, inline_form = { 2, 1 } }

local CELL_W, CELL_H = 320, 260

local function theme_of(kit)
  if kit.theme then return kit.theme end
  local ok, theme = pcall(require, "theme")
  return ok and type(theme) == "table" and theme or {}
end

--- A kit field of `name` in the kit's ink and face, packed as a form
--- takes it: `{ node, input, state, handle }`.
local function field(kit, widgets, name, spec, width)
  local theme = theme_of(kit)
  local props = {}
  for k, v in pairs(spec) do props[k] = v end
  -- (Its width given up front: an outline that is cut for its label is
  -- drawn to it.)
  props.width = props.width or width
  props.height = props.height or 56
  props.inset = props.inset or { 16, 24, 16, 6 }
  props.font_family = props.font_family or theme.font
  props.font_size = props.font_size or (theme.size and theme.size.normal) or 15
  props.color = props.color or kit.ink("hi")
  props.placeholder_color = props.placeholder_color or kit.ink("lo")
  props.caret_color = props.caret_color or kit.ink("accent")
  props.selection_color = props.selection_color or function() return kit.ink("accent")():alpha(0.3) end
  return { widgets[name](props) }
end

--- `node` centred across a cell `columns` wide.
local function cell(node, columns, rows, width)
  local w, h = columns * CELL_W - 40, rows * CELL_H - 40
  node.x = math.floor((w - width) / 2)
  return ui.Item { width = w, height = h, node }
end

--- A sign-up form, tried with the address unfinished and no password: the
--- banner lists both, each field says what it wants.
function S.form(kit, widgets)
  local W = 360
  local node, form = widgets.form { id = "sample-form", width = W, submit_label = "Create account",
    on_submit = function(_, done) done(true) end }
  form.field("name", { label = "Name", control = field(kit, widgets, "entry",
    { id = "sample-form-name", label = "Full name", text = "Ana Lindqvist", required = true }, W) })
  form.field("email", { label = "Email", message = "Enter an address like ana@example.com",
    control = field(kit, widgets, "email", { id = "sample-form-email", label = "Email", text = "ana@",
      icon = "mail", inset = { 48, 24, 44, 6 } }, W) })
  form.field("password", { label = "Password", message = "Choose a password of eight or more",
    valid = function(v) return #(v or "") >= 8 end,
    control = field(kit, widgets, "password", { id = "sample-form-password", label = "Password", text = "",
      icon = "lock", inset = { 48, 24, 44, 6 } }, W) })
  form.submit()
  return cell(node, 2, 2, W)
end

--- A sign-in the server turned down: the failure in the banner, the
--- fields as they were typed.
function S.login_form(kit, widgets)
  local W = 264
  local node, form = widgets.login_form { id = "sample-login", width = W,
    on_submit = function(_, done) done(false, "Wrong password") end }
  form.field("user", { label = "Username", control = field(kit, widgets, "entry",
    { id = "sample-login-user", label = "Username", text = "ana", required = true }, W) })
  form.field("password", { label = "Password", control = field(kit, widgets, "password",
    { id = "sample-login-password", label = "Password", text = "hunter22", required = true, icon = "lock",
      inset = { 48, 24, 44, 6 } }, W) })
  form.submit()
  return cell(node, 1, 2, W)
end

--- A settings page: a change saves itself once it is valid; the status
--- line says where it stands.
function S.settings_form(kit, widgets)
  local W = 340
  local node, form = widgets.settings_form { id = "sample-settings", width = W,
    on_submit = function(_, done) done(true) end }
  form.field("device", { label = "Device name", control = field(kit, widgets, "entry",
    { id = "sample-settings-device", label = "Device name", text = "Studio desk", required = true }, W) })
  form.field("port", { label = "Port", message = "A port from 1 to 65535",
    control = field(kit, widgets, "numeric_entry", { id = "sample-settings-port", label = "Port", text = "8080",
      minimum = 1, maximum = 65535, unit = "tcp", inset = { 16, 24, 52, 6 } }, W) })
  return cell(node, 2, 1, W)
end

--- A subscribe row caught sending: the button's spinner in its place.
function S.inline_form(kit, widgets)
  local W = 440
  local node, form = widgets.inline_form { id = "sample-inline", width = W, submit_label = "Subscribe",
    button_width = 128, button_height = 48, on_submit = function() end }
  form.field("email", { label = "Email", message = "Enter your address",
    control = field(kit, widgets, "email", { id = "sample-inline-email", placeholder = "Email address",
      text = "ana@morf.dev", icon = "mail", height = 48, inset = { 48, 0, 44, 0 } }, W - 128 - 10) })
  form.submit()
  return cell(node, 2, 1, W)
end

return S
