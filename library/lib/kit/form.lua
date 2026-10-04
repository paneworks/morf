-- Forms (the Form archetype): what a group of fields adds up to -- whether
-- it can be sent, whether anything changed, what is wrong and where -- and
-- the sending.
--
--     local node, form = form.make("login_form", {
--       width = 320, submit_label = "Sign in",
--       on_submit = function(values, done) sign_in(values.user, values.password, done) end,
--     })
--     form.field("user", { label = "Username", control = { widgets.entry { label = "Username", required = true } },
--       message = "Enter your username" })
--
-- `make` returns the control and a handle:
--
-- * `field(name, opts)` adds a field. `opts.control` is what was made for
--   it: a node, or the returns of a kit control packed in a table
--   (`{ widgets.entry { ... } }`: the control, the input that holds focus,
--   its live state, its handle). `valid`, `dirty`, `message` and `value`
--   are bindings (a message may be a string); a kit text field gives its
--   own (`acceptable`, its text, and what it was made with) when they are
--   left out. `validate(value, done)` checks out of line -- a name still
--   free on the server --: the field is pending until `done(ok, message)`.
--   `label` names it in the summary; `place = false` leaves the node where
--   the configuration put it (bind `error(name)` to show its message);
--   `reset(value)` puts a control that is not a text field back.
-- * `error(name)`: the field's message while the policy shows it, else "";
--   `shown(name)` whether it shows.
-- * `submit()`, `reset()`, `remove(name)`, `focus(name)`, and `t`, the
--   form's live state (`valid`, `dirty`, `pending`, `submitting`, `tried`,
--   `error_count`, `first_invalid`).
--
-- `spec.on_submit(values, done)` sends: `done(ok, message)` finishes it; a
-- failure's message shows in the summary. `spec.flick`, the Flickable the
-- form scrolls in, is scrolled to a field the form sends the keyboard to.
--
-- The widget lays the fields out: `form` a column under its summary with
-- the submit button at the end, `login_form` the same with a full-width
-- button, `settings_form` a column that saves itself once what changed is
-- valid (no button; its summary says unsaved, saving, saved), and
-- `inline_form` one row, the field and its button. The skin draws the
-- `summary` (errors, a failed send, the save status) and, through its
-- `message` builder (name, message, shown, width, id), each field's message
-- under it; the submit button is a kit Press drawn as `form_submit` (the
-- theme's suggested action, its loading indicator while sending). What
-- the policy shows, when the form may send and Return, are the
-- archetype's.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

local SUBMIT = { form = "Submit", login_form = "Sign in", inline_form = "Send" }
local GAP = { form = 14, login_form = 14, settings_form = 12, inline_form = 10 }
-- How long a settings page waits after the last change before it saves.
local SAVE_DELAY = 450
local serial = 0

local function get(v) if type(v) == "function" then return v() end return v end

-- Reads a field of a kit control's live state, nil when it has none (a
-- morf.state raises on a field it does not keep).
local function probe(state, field)
  if type(state) ~= "table" and type(state) ~= "userdata" then return nil end
  local ok, v = pcall(function() return state[field] end)
  if ok then return v end
  return nil
end

function M.make(widget, spec)
  spec = spec or {}
  serial = serial + 1
  local key = "kit.form." .. serial
  local W = spec.width or 320
  local gap = spec.gap or GAP[widget] or 14
  local inline = widget == "inline_form"
  local has_button = widget ~= "settings_form" and spec.submit ~= false
  local button_w = spec.button_width or (inline and 104 or (widget == "login_form" and W or 132))
  local button_h = spec.button_height or (widget == "login_form" and 44 or 40)
  local field_w = inline and (W - button_w - gap) or W

  local root, t, ctl
  local fields, order = {}, {}
  -- Bumped as a message shows or hides: the summary's list follows it.
  local listed = morf.signal(key .. ".errors", 0)
  local failure = morf.signal(key .. ".failure", "")
  local saved_once = morf.signal(key .. ".saved", false)

  local function send(event, ...)
    if ctl then return ctl.send(event, ...) end
  end

  -- ------------------------------------------------------- focusing --

  local function reveal(f)
    local flick = spec.flick
    local place = f.node
    if not flick or not place then return end
    local top = (place.layout_y or 0) - (flick.layout_y or 0)
    local bottom = top + (place.layout_height or 0) + 32
    local view = flick.layout_height or 0
    local y = flick.content_y or 0
    local to
    if top < 16 then to = y + top - 16 elseif bottom > view then to = y + bottom - view end
    if to then
      local room = math.max(0, (probe(flick, "content_height") or 0) - view)
      if room > 0 then to = math.min(room, to) end
      to = math.max(0, to)
      morf.animation.play { { node = flick, property = "content_y", to = to, duration = 260, easing = "out_cubic" } }
    end
  end

  local function focus(name)
    local f = fields[name]
    if not f then return end
    if f.focus then morf.focus.set(f.focus, true) end
    reveal(f)
  end

  -- ------------------------------------------------- what the skin reads --

  --- The fields whose message shows, in their order: `{ name, label, message }`.
  local function errors()
    listed:get()
    local out = {}
    for _, name in ipairs(order) do
      local f = fields[name]
      if f and f.shown:get() then out[#out + 1] = { name = name, label = f.label, message = f.text:get() } end
    end
    return out
  end

  local api = {
    errors = errors,
    focus = focus,
    failure = function() return failure:get() end,
    gap = gap,
    width = W,
    id = spec.id,
    --- A settings page's standing: "saving", "unsaved", "invalid" or "saved".
    --- (`st`, the form's live state: the skin's, which is there before the
    --- glue's own is.)
    status = function(st)
      st = st or t
      if not st then return "saved" end
      if st.submitting then return "saving" end
      if st.dirty then return (st.valid and not st.pending) and "unsaved" or "invalid" end
      return saved_once:get() and "saved" or "idle"
    end,
  }

  -- ---------------------------------------------------------- sending --

  local function values()
    local out = {}
    for _, name in ipairs(order) do
      local f = fields[name]
      if f then out[name] = f.value() end
    end
    return out
  end

  local function submitted()
    failure:set("")
    local finished = false
    local function done(ok, message)
      if finished then return end
      finished = true
      ok = ok ~= false
      send("done", ok, message or "")
      if ok then
        for _, name in ipairs(order) do
          local f = fields[name]
          if f then f.baseline:set(f.value()) end
        end
        saved_once:set(true)
      else
        failure:set(message or "")
      end
      if spec.on_done then spec.on_done(ok, message) end
    end
    if spec.on_submit then spec.on_submit(values(), done) else done(true) end
  end

  local function reset_fields()
    failure:set("")
    for _, name in ipairs(order) do
      local f = fields[name]
      if f then
        local was = f.baseline:get()
        if f.reset then f.reset(was)
        elseif f.input_text then
          f.focus.text = was or ""
          if f.ctl then f.ctl.send("edited", was or "") end
        end
      end
    end
  end

  -- ------------------------------------------------------- the control --

  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget, full.form = widget, api
  -- (The glue's own, not handlers for the node.)
  full.fields, full.flick, full.on_submit, full.on_done, full.on_key_pressed = nil, nil, nil, nil, nil
  full.on_submitted = function(...)
    submitted()
    if spec.on_submitted then spec.on_submitted(...) end
  end
  full.on_invalid = function(name, ...)
    focus(name)
    if spec.on_invalid then spec.on_invalid(name, ...) end
  end
  full.on_show_error = function(name, message)
    local f = fields[name]
    if f then
      f.text:set(message or "")
      f.shown:set(true)
      listed:set(listed:get() + 1)
    end
    if spec.on_show_error then spec.on_show_error(name, message) end
  end
  full.on_hide_error = function(name)
    local f = fields[name]
    if f then
      f.shown:set(false)
      listed:set(listed:get() + 1)
    end
    if spec.on_hide_error then spec.on_hide_error(name) end
  end
  full.on_reset = function(...)
    reset_fields()
    if spec.on_reset then spec.on_reset(...) end
  end

  -- The layout: the summary, the fields, the button.
  local summary_box = morf.signal(key .. ".summary", 0)
  local summary_node
  local summary_holder = ui.Item { width = W,
    height = function()
      summary_box:get()
      return summary_node and (summary_node.layout_height or 0) or 0
    end }
  local list = inline and ui.Row { gap = gap } or ui.Column { width = W, gap = gap }
  -- (The button is made once the form's state is there for it to follow.)
  local button
  local button_slot = has_button and ui.Item { x = inline and 0 or (W - button_w), y = inline and 0 or gap,
    width = button_w, height = button_h } or nil
  local column
  if inline then
    if button_slot then ui.reparent(button_slot, list) end
    column = ui.Column { width = W, gap = 0, list, summary_holder }
  else
    local foot = has_button and ui.Item { width = W, height = button_h + gap, button_slot } or nil
    column = ui.Column { width = W, gap = 0, summary_holder, list, foot }
  end

  -- A message under each placed field, from the skin's builder.
  local function build_message(f, builders)
    if f.message_node then ui.destroy(f.message_node, true) f.message_node = nil end
    local make = builders and builders.message
    if type(make) == "function" and f.message_holder then
      local name = f.name
      f.message_node = make(name, function() return f.text:get() end, function() return f.shown:get() end, field_w,
        spec.id and (spec.id .. "-" .. name .. "-message") or nil)
      if f.message_node then ui.reparent(f.message_node, f.message_holder) end
    end
    f.message_gen:set(f.message_gen:get() + 1)
  end

  local function place_summary()
    if not ctl then return end
    local node = (ctl.slots() or {}).summary
    summary_node = node
    if node then ui.reparent(node, summary_holder) end
    summary_box:set(summary_box:get() + 1)
  end

  -- Return from a control that has no use for it (a field's own Return is
  -- wired where the field is added).
  local function on_key(keysym, text, modifiers, repeat_, name)
    if spec.on_key_pressed and spec.on_key_pressed(keysym, text, modifiers, repeat_, name) then return true end
    local effects = send("key", name or "", modifiers or "")
    if type(effects) == "table" then return effects.handled == true end
    return (name == "Return" or name == "KP_Enter") and t.enabled ~= false
  end

  local props = { width = W, height = function() return column.layout_height or 0 end, focus_policy = "none",
    on_key_pressed = on_key }
  root, t, ctl = control.make("Form", widget, full, {
    children = { column }, props = props, builders = { message = true },
    on_rebuild = function(builders)
      for _, name in ipairs(order) do build_message(fields[name], builders) end
      place_summary()
    end })
  place_summary()
  if has_button then
    button = control.make("Press", "form_submit", {
      widget = "form_submit",
      id = spec.id and (spec.id .. "-submit") or nil,
      label = spec.submit_label or SUBMIT[widget] or "Submit",
      icon = spec.submit_icon,
      width = button_w, height = button_h,
      busy = function() return t.submitting end,
      enabled = function() return not ((t.tried and not t.valid) or t.pending or t.submitting) end,
      on_clicked = function() send("submit") end,
    })
    ui.reparent(button, button_slot)
  end

  -- A settings page saves itself once what changed is valid, a moment
  -- after the last change.
  if widget == "settings_form" then
    local timer
    morf.effect(key .. ".autosave", function()
      local ready = t.dirty and t.valid and not t.pending and not t.submitting
      if timer then timer:cancel() timer = nil end
      if ready then
        timer = morf.timer(spec.save_delay or SAVE_DELAY, function()
          timer = nil
          if t.dirty and t.valid and not t.pending and not t.submitting then send("submit") end
        end, false)
      end
    end, { owner = root })
  end

  -- --------------------------------------------------------- the handle --

  local handle = { t = t, node = root }

  function handle.field(name, opts)
    opts = opts or {}
    if fields[name] then handle.remove(name) end
    local c = opts.control
    local node, focus_node, state, field_ctl
    if type(c) == "table" then
      node = c[1]
      if type(c[2]) == "userdata" then focus_node, state, field_ctl = c[2], c[3], c[4]
      else focus_node, state, field_ctl = c[1], c[2], c[3] end
    else
      node, focus_node = c, c
    end
    node = opts.node or node
    focus_node = opts.focus or focus_node
    local text_state = probe(state, "acceptable") ~= nil
    local f = { name = name, label = opts.label or name, node = node, focus = focus_node, ctl = field_ctl,
      reset = opts.reset, input_text = text_state and focus_node ~= nil and focus_node ~= node }
    local fkey = key .. "." .. name
    f.text = morf.signal(fkey .. ".text", "")
    f.shown = morf.signal(fkey .. ".shown", false)
    f.async = morf.signal(fkey .. ".async", false)
    f.message_gen = morf.signal(fkey .. ".message", 0)
    -- What it holds, what makes it valid, what it says when it is not.
    f.value = opts.value or (text_state and function() return state.text end)
      or (state and function() return probe(state, "value") or probe(state, "checked") end)
      or function() return nil end
    local valid = opts.valid or (text_state and function() return state.acceptable end) or function() return true end
    f.baseline = morf.signal(fkey .. ".baseline", (function() local v = f.value() return v end)())
    local dirty = opts.dirty or function(v) return tostring(v) ~= tostring(f.baseline:get()) end
    local message = opts.message
    local function said(v, ok)
      local m = get(message)
      if type(message) == "function" then m = message(v, ok) end
      if m ~= nil and m ~= "" then return tostring(m) end
      if v == nil or v == "" then return f.label .. " is required" end
      return "Check " .. f.label:lower()
    end
    fields[name] = f
    order[#order + 1] = name

    -- Placed: the field and its message under it, in the form's flow.
    if node and opts.place ~= false then
      if opts.stretch ~= false then node.width = field_w end
      f.message_holder = ui.Item { width = field_w,
        height = function()
          f.message_gen:get()
          return f.message_node and (f.message_node.layout_height or 0) or 0
        end }
      f.wrapper = ui.Column { width = field_w, gap = 0, node, f.message_holder }
      ui.reparent(f.wrapper, list)
      if button_slot and inline then ui.reparent(button_slot, list) end
      build_message(f, ctl.builders())
    end

    -- Its standing, told to the archetype as it changes.
    morf.effect(fkey .. ".standing", function()
      local v = f.value()
      local ok = valid(v) ~= false
      local m = ok and "" or said(v, ok)
      local checked = f.async:get()
      if ok and checked and checked.value == v and not checked.ok then
        ok, m = false, checked.message or said(v, false)
      end
      send("field", name, ok, dirty(v) == true, m)
    end, { owner = root })

    -- A check out of line: pending until it answers; an answer for a value
    -- since changed is dropped.
    if opts.validate then
      local turn = 0
      morf.effect(fkey .. ".check", function()
        local v = f.value()
        local ok = valid(v) ~= false
        turn = turn + 1
        local mine = turn
        if not ok then send("pending", name, false) return end
        send("pending", name, true)
        morf.timer(1, function()
          if mine ~= turn then return end
          opts.validate(v, function(good, why)
            if mine ~= turn then return end
            f.async:set({ value = v, ok = good ~= false, message = why })
            send("pending", name, false)
          end)
        end, false)
      end, { owner = root })
    end

    -- Touched once the keyboard leaves it.
    if focus_node then
      local had = false
      morf.effect(fkey .. ".touched", function()
        local on = probe(focus_node, "focused") == true
        if had and not on then send("touched", name) end
        had = on
      end, { owner = root })
    end

    -- Return in a text field sends: the input takes the key itself, so its
    -- accept is where the form hears it (and the field's control still
    -- hears it first).
    if f.input_text and field_ctl then
      local multiline = probe(focus_node, "multiline") == true
      focus_node.on_accepted = function()
        field_ctl.send("accepted")
        send("key", "Return", multiline and "ctrl" or "")
      end
    end
    return f
  end

  function handle.remove(name)
    local f = fields[name]
    if not f then return end
    send("remove", name)
    fields[name] = nil
    for i, n in ipairs(order) do if n == name then table.remove(order, i) break end end
    if f.wrapper then
      if f.node then ui.reparent(f.node, root) f.node.visible = false end
      ui.destroy(f.wrapper, true)
    end
    listed:set(listed:get() + 1)
  end

  function handle.error(name)
    return function()
      local f = fields[name]
      if f and f.shown:get() then return f.text:get() end
      return ""
    end
  end

  function handle.shown(name)
    return function() local f = fields[name] return f ~= nil and f.shown:get() end
  end

  function handle.submit() send("submit") end
  function handle.reset() send("reset") end
  handle.focus = focus
  handle.values = values

  for _, entry in ipairs(spec.fields or {}) do handle.field(entry.name, entry) end
  return root, handle
end

-- ------------------------------------------------------------- the looks --
--
-- What every theme draws for a form, laid out once here so the themes
-- share it and differ only in style. `look(style)` returns the slot
-- functions; `style` gives the theme's ways and tones:
--
--   text(props), icon(name, size, color, props), loading(size, color, props)
--   spring() (a behaviour: the reveal), quick() (a behaviour: tones)
--   body, small (font sizes), weight (a heading's), font (a face, or nil)
--   radius (the banner's corners)
--   tones -- functions of nothing returning a colour: `error_ground`,
--     `error_edge`, `error_ink` (a banner's and a message's), `ink`,
--     `ink_dim`, `hover` (a summary row under the pointer), `saved`,
--     `unsaved`, `status_ground`; and, for a banner on a ground of its own,
--     `banner_ink`, `banner_label`, `banner_dim` (the error ink, the ink and
--     the dim ink when left out)
--   decorate(banner, height) (optional): adds the theme's marks to a
--     banner (`height` a binding)
--   caps (optional): headings in capitals.

-- The summary's measures, shared by every theme.
local PAD, HEAD, ROW, ICON = 12, 22, 28, 18
local MESSAGE = 26
local STATUS = 34
-- Closed: an Item whose height is 0 takes its children's, so a closed one
-- is a hair high instead.
local SHUT = 0.01

function M.look(style)
  local L = {}
  local tones = style.tones
  local function caps(s) if style.caps then return s:upper() end return s end
  local banner_ink = tones.banner_ink or tones.error_ink
  local banner_label = tones.banner_label or tones.ink
  local banner_dim = tones.banner_dim or tones.ink_dim

  --- The error summary: a banner that slides down from under the fields'
  --- top once a send was tried and something is wrong (or a send failed),
  --- its heading and a row per field that shows a message, each sending
  --- the keyboard to its field.
  function L.summary(t, spec)
    local api = spec.form
    if not api then return nil end
    local W = api.width or 320
    local gap = api.gap or 12
    local count = morf.signal("kit.form.look.count." .. tostring(api), 0)
    local function shown()
      local list = api.errors()
      return (t.tried and #list > 0) or api.failure() ~= ""
    end
    local function inner()
      local n = count:get()
      return PAD * 2 + HEAD + (n > 0 and (6 + n * ROW) or 0)
    end
    local head = style.text { id = api.id and (api.id .. "-summary-head") or nil, x = PAD + ICON + 10, y = PAD, width = W - PAD * 2 - ICON - 10, height = HEAD,
      vertical_alignment = "center", elide = "right", font_size = style.body, font_weight = style.weight or 700,
      font_family = style.font, color = banner_ink,
      text = function()
        local fail = api.failure()
        if fail ~= "" then return caps(fail) end
        local n = count:get()
        return caps(n == 1 and "One field needs attention" or (n .. " fields need attention"))
      end }
    local rows = ui.Column { x = PAD + ICON + 10, y = PAD + HEAD + 6, width = W - PAD * 2 - ICON - 10, gap = 0 }
    local banner = ui.Rect { width = W, height = inner, radius = style.radius or 0,
      color = tones.error_ground, border_width = tones.error_edge and 1 or 0, border_color = tones.error_edge,
      translate_y = function() return shown() and 0 or -inner() end,
      behavior = { translate_y = style.spring(), height = style.spring(), color = style.quick() },
      style.icon("error", ICON, banner_ink, { x = PAD, y = PAD + (HEAD - ICON) / 2 }),
      head, rows }
    if style.decorate then style.decorate(banner, inner) end
    local node = ui.Item { id = api.id and (api.id .. "-summary") or nil, width = W, clip = true,
      height = function() return shown() and (inner() + gap) or SHUT end,
      opacity = function() return shown() and 1 or 0 end,
      behavior = { height = style.spring(), opacity = style.spring() },
      banner }
    -- The rows, made again only as the list changes (a send, a fix): while
    -- it hides, the last ones stay for it to slide away with.
    local seen, made = "", {}
    morf.effect("kit.form.look.rows." .. tostring(node), function()
      local list = api.errors()
      if #list == 0 and not shown() then return end
      local sig = {}
      for i, e in ipairs(list) do sig[i] = e.name .. "\1" .. e.message end
      local now = table.concat(sig, "\2")
      if now == seen then return end
      seen = now
      for _, old in ipairs(made) do ui.destroy(old, true) end
      made = {}
      for _, e in ipairs(list) do
        local name = e.name
        local row_w = W - PAD * 2 - ICON - 10
        local label = style.text { text = caps(e.label), font_size = style.small, font_weight = style.weight or 700,
          font_family = style.font, color = banner_label }
        local area
        local function hot() return area ~= nil and area.hovered == true end
        area = ui.MouseArea { id = api.id and (api.id .. "-summary-" .. name) or nil,
          width = W - PAD * 2 - ICON - 10, height = ROW, cursor = "pointer",
          accessible_role = "link", accessible_name = e.label .. ": " .. e.message,
          on_clicked = function() api.focus(name) end,
          ui.Rect { anchors = { fill = true, left_margin = -6, right_margin = -6 }, radius = 4,
            color = function() return hot() and tones.hover() or tones.hover():alpha(0) end,
            behavior = { color = style.quick() } },
          ui.Row { anchors = { left = true, vertical_center = true }, gap = 8, align = "center",
            label,
            style.text { text = e.message, font_size = style.small, font_family = style.font, color = banner_dim,
              width = function() return math.max(40, row_w - (label.layout_width or 0) - 8) end,
              elide = "right" } } }
        ui.reparent(area, rows)
        made[#made + 1] = area
      end
      count:set(#list)
    end, { owner = node })
    return node
  end

  --- A field's message: the error tone with its glyph, revealed by a
  --- height and an opacity spring.
  function L.message(name, message, shown, width, id)
    return ui.Item { id = id, width = width, clip = true,
      height = function() return shown() and MESSAGE or SHUT end,
      opacity = function() return shown() and 1 or 0 end,
      behavior = { height = style.spring(), opacity = style.spring() },
      ui.Row { x = 4, y = 5, gap = 6, align = "center",
        style.icon("error", 16, tones.error_ink),
        style.text { text = message, font_size = style.small, font_family = style.font, color = tones.error_ink,
          width = width - 30, elide = "right" } } }
  end

  --- A settings page's standing: saved, unsaved, saving, or why it cannot.
  function L.status(t, spec)
    local api = spec.form
    if not api then return nil end
    local W = api.width or 320
    local gap = api.gap or 12
    local function state()
      if api.failure() ~= "" then return "failed" end
      return api.status(t)
    end
    local WORDS = { saved = "All changes saved", idle = "All changes saved", unsaved = "Unsaved changes",
      saving = "Saving…", invalid = "Fix the fields marked to save" }
    local GLYPH = { saved = "check_circle", idle = "check_circle", unsaved = "edit", invalid = "error",
      failed = "error" }
    local function tone()
      local s = state()
      if s == "failed" or s == "invalid" then return tones.error_ink() end
      if s == "saved" then return tones.saved() end
      if s == "unsaved" or s == "saving" then return tones.unsaved() end
      return tones.ink_dim()
    end
    local spinner = ui.Item { width = 18, height = 18, opacity = function() return state() == "saving" and 1 or 0 end,
      behavior = { opacity = style.quick() },
      style.loading(18, tones.unsaved, { active = function() return state() == "saving" end }) }
    local glyph = ui.Item { width = 18, height = 18, opacity = function() return state() == "saving" and 0 or 1 end,
      behavior = { opacity = style.quick() },
      style.icon(function() return GLYPH[state()] or "check_circle" end, 18, tone) }
    local plate = ui.Rect { width = W, height = STATUS, radius = style.radius or 0, color = tones.status_ground,
      ui.Item { x = PAD, y = (STATUS - 18) / 2, width = 18, height = 18, glyph, spinner },
      style.text { x = PAD + 28, width = W - PAD * 2 - 28, height = STATUS, vertical_alignment = "center",
        elide = "right", font_size = style.small, font_weight = style.weight or 700, font_family = style.font,
        color = tone, behavior = { color = style.quick() },
        text = function()
          local s = state()
          if s == "failed" then return caps("Not saved: " .. api.failure()) end
          return caps(WORDS[s] or "")
        end } }
    if style.decorate then style.decorate(plate, function() return STATUS end) end
    return ui.Item { width = W, height = STATUS + gap, plate }
  end

  --- An inline form's failure: one line under the row, revealed as a
  --- field's message is.
  function L.failure(t, spec)
    local api = spec.form
    if not api then return nil end
    local W = api.width or 320
    return L.message("", api.failure, function() return api.failure() ~= "" end, W,
      api.id and (api.id .. "-failure") or nil)
  end

  --- The submit button: the theme's suggested action (`slots`, its skin's),
  --- its label giving way to the theme's loading indicator while sending.
  function L.submit(slots, t, spec, ink)
    local content = slots.content
    if type(content) == "function" then content = content() end
    local function busy() return get(spec.busy) == true end
    local label = ui.Item { anchors = { fill = true }, opacity = function() return busy() and 0 or 1 end,
      scale = function() return busy() and 0.92 or 1 end,
      behavior = { opacity = style.quick(), scale = style.spring() }, content }
    local spin = ui.Item { anchors = { center_in = true }, width = 22, height = 22,
      opacity = function() return busy() and 1 or 0 end, behavior = { opacity = style.quick() },
      style.loading(22, ink, { active = busy }) }
    slots.content = ui.Item { anchors = { fill = true }, label, spin }
    return slots
  end

  return L
end

return M
