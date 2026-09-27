-- The polkit dialog: when a program asks for something only root may do
-- (starting a service, mounting a disk, pkexec), a panel drops out of the
-- frame's top edge asking for the password, and goes back up with the
-- verdict. The shell is the session's polkit agent (lib/polkit_agent.lua);
-- this is what it draws.
--
-- A session has one agent. While another holds the job (hyprpolkitagent,
-- polkit-gnome) the authority refuses this one, and it asks again every
-- minute, so it takes over as soon as the other is gone. `polkit.agent`
-- "off" leaves the job to the other for good.
--
-- Every screen runs the shell, and one of them (`morf.primary()`) is the
-- agent; the dialog opens on the screen in use. The agent tells every
-- screen how each request stands (`morf.broadcast("polkit", "view", ...)`),
-- the focused one draws it, and the answer goes back the same way to the
-- one holding the request -- inside this process, through the shell's own
-- IPC socket, never on a command line.
--
-- The badge says where it is: a slowly turning cookie while it asks, the
-- loading shapes while the password is checked, a burst in the error
-- colour for a wrong one (and the field shakes), a circle with a tick on
-- the way out. A wrong password is another try at the same request, not
-- the program refused.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local drawer = require("drawer")

local C = theme.color
local M = {}

local W, PAD = 452, 28
local INNER = W - 2 * PAD
local FIELD_H, DOT, MAX_DOTS = 52, 11, 18
local RETRIES = 5

-- What is being asked, without the secret: `{ message, action, user,
-- prompt, echo }`, or false with nothing open.
M.request = morf.signal("caelestia.polkit.request", false)
-- "asking", "checking", "wrong", "done" or "refused".
M.phase = morf.signal("caelestia.polkit.phase", "asking")
-- What PAM had to say ("touch the sensor", "wrong PIN"), or "".
M.info = morf.signal("caelestia.polkit.info", "")
local typed = morf.signal("caelestia.polkit.typed", 0)
local shake = morf.signal("caelestia.polkit.shake", 0)
M.registered = morf.signal("caelestia.polkit.registered", false)

-- The request this screen is showing, by id, and the password: plain
-- locals, never signals.
local viewing
local password = ""
local seen_failures = 0

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end

-- ------------------------------------------------------------ the field --

--- A kick sideways, and the spring brings it home, wobbling.
function M.shake()
  shake:set(1)
  morf.timer(70, function() shake:set(0) end, false)
end

local keys
local function clear()
  password = ""
  typed:set(0)
  if keys then keys.text = "" end
end

-- To every screen, or to this one alone where there is no shell socket to
-- broadcast through (a headless test).
local send

function M.submit()
  if not viewing or M.phase:get() == "checking" or M.phase:get() == "done" then return end
  if password == "" then return end
  local answer = password
  clear()
  M.phase:set("checking")
  M.info:set("")
  send("answer", viewing, answer)
end

function M.cancel()
  if viewing then send("cancel", viewing) end
end

keys = ui.TextInput {
  id = "polkit-keys",
  width = 1, height = 1, opacity = 0,
  password = true,
  on_text_changed = function(text)
    password = text or ""
    typed:set(math.min(#password, MAX_DOTS))
    if M.phase:get() == "wrong" and #password > 0 then M.phase:set("asking") end
  end,
  on_accepted = function() M.submit() end,
  on_escape = function() M.cancel() end,
}

local dots = {}
for i = 1, MAX_DOTS do
  dots[i] = ui.Rect {
    width = DOT, height = DOT, radius = DOT / 2,
    color = function() return C.primary end,
    visible = function() return typed:get() >= i end,
    scale = function() return typed:get() >= i and 1 or 0 end,
    behavior = { scale = { duration = 260, easing = "out_back" } },
  }
end

local submit_area
submit_area = ui.MouseArea {
  id = "polkit-submit",
  anchors = { right = true, right_margin = 7, vertical_center = true },
  width = FIELD_H - 14, height = FIELD_H - 14, cursor = "pointer",
  on_clicked = function() M.submit() end,
  ui.Rect {
    anchors = { fill = true }, radius = (FIELD_H - 14) / 2,
    color = function() return typed:get() > 0 and C.primary or C.surfaceContainerHigh end,
    behavior = { color = { duration = theme.duration.small } },
  },
  kit.icon("arrow_forward", 22, function() return typed:get() > 0 and C.onPrimary or C.onSurfaceVariant end,
    { anchors = { center_in = true } }),
}

local field = ui.MouseArea {
  id = "polkit-field",
  width = INNER, height = FIELD_H, cursor = "text",
  on_clicked = function() keys.focus = true end,
  translate_x = function() return (shake:get() % 2 == 1) and 10 or 0 end,
  behavior = { translate_x = ui.spring { stiffness = 900, damping = 9 } },
  ui.Rect {
    anchors = { fill = true }, radius = FIELD_H / 2,
    color = function() return C.surfaceContainerHighest end,
    border_width = function() return M.phase:get() == "wrong" and 2 or 0 end,
    border_color = function() return C.error end,
  },
  kit.icon(function() return M.phase:get() == "checking" and "hourglass" or "key" end, 22,
    function() return C.onSurfaceVariant end, { x = 20, anchors = { vertical_center = true } }),
  kit.text {
    anchors = { vertical_center = true }, x = 56, width = INNER - 120, elide = "right",
    text = function()
      local r = M.request:get()
      local p = r and r.prompt or ""
      p = p:gsub(":%s*$", "")
      return p ~= "" and p or "Password"
    end,
    color = function() return C.onSurfaceVariant end,
    visible = function() return typed:get() == 0 end,
  },
  ui.Row { x = 56, anchors = { vertical_center = true }, gap = 7, table.unpack(dots) },
  submit_area,
}

-- ------------------------------------------------------------ the badge --

local LOADING = { "soft_burst", "cookie9", "pentagon", "pill", "sunny", "cookie4", "oval", "flower" }
local step = morf.signal("caelestia.polkit.step", 1)
local stepper
morf.effect("caelestia.polkit.stepper", function()
  local checking = M.phase:get() == "checking"
  morf.timer(1, function()
    if checking and not stepper then
      stepper = morf.timer(650, function() step:set(step:get() % #LOADING + 1) end, true)
    elseif not checking and stepper then
      stepper:cancel()
      stepper = nil
    end
  end, false)
end)

local function badge_colour()
  local p = M.phase:get()
  if p == "wrong" or p == "refused" then return C.errorContainer end
  if p == "done" then return C.primary end
  return C.primaryContainer
end
local function badge_ink()
  local p = M.phase:get()
  if p == "wrong" or p == "refused" then return C.onErrorContainer end
  if p == "done" then return C.onPrimary end
  return C.onPrimaryContainer
end

local badge = ui.Item {
  width = 60, height = 60,
  kit.shape {
    id = "polkit-badge", anchors = { fill = true },
    shape = function()
      local p = M.phase:get()
      if p == "checking" then return LOADING[step:get()] end
      if p == "wrong" or p == "refused" then return "sunny" end
      if p == "done" then return "circle" end
      return "cookie9"
    end,
    color = badge_colour,
    loop = function()
      local p = M.phase:get()
      if not M.request:get() or p == "done" then return nil end
      return { rotation = { to = 360, duration = p == "checking" and 2600 or 14000, hold = true } }
    end,
  },
  kit.icon(function()
    local p = M.phase:get()
    if p == "done" then return "check" end
    if p == "wrong" or p == "refused" then return "priority_high" end
    if p == "checking" then return "more_horiz" end
    return "shield_lock"
  end, 28, badge_ink, { anchors = { center_in = true }, fill = true }),
}

-- ------------------------------------------------------------- the panel --

local function button(id, label, filled, on_clicked)
  local area
  area = ui.MouseArea {
    id = id, height = 40, width = filled and 148 or 104, cursor = "pointer",
    on_clicked = on_clicked,
    scale = function() return (area and area.pressed) and 0.95 or 1 end,
    behavior = { scale = kit.spring(700, 20) },
    ui.Rect {
      anchors = { fill = true }, radius = 20,
      color = function()
        local base = filled and C.primary or C.onSurface:alpha(0)
        if area and area.hovered then return filled and base:mix(C.onPrimary, 0.08) or C.onSurface:alpha(0.08) end
        return base
      end,
      behavior = { color = { duration = theme.duration.small } },
    },
    kit.text {
      anchors = { center_in = true }, text = label, font_weight = 600,
      color = function() return filled and C.onPrimary or C.primary end,
    },
  }
  return area
end

local column = ui.Column {
  x = PAD, y = PAD, width = INNER, gap = 14,
  ui.Row {
    gap = 16, align = "center",
    badge,
    ui.Column {
      gap = 2,
      kit.text { text = "Authentication required", font_size = theme.size.large, font_weight = 600,
        width = INNER - 76, elide = "right" },
      kit.text {
        width = INNER - 76, elide = "right", font_size = theme.size.small,
        color = function() return C.onSurfaceVariant end,
        text = function()
          local r = M.request:get()
          return r and ("as " .. tostring(r.user)) or ""
        end,
      },
    },
  },
  kit.text {
    id = "polkit-message", width = INNER, wrap = true, max_lines = 4, line_height = 1.35,
    text = function() local r = M.request:get() return r and r.message or "" end,
  },
  kit.text {
    width = INNER, elide = "middle", font_size = theme.size.small - 2,
    color = function() return C.outline end,
    text = function() local r = M.request:get() return r and r.action or "" end,
  },
  field,
  kit.text {
    id = "polkit-info", width = INNER, wrap = true, max_lines = 2, font_size = theme.size.small,
    visible = function() return M.info:get() ~= "" end,
    color = function()
      local p = M.phase:get()
      return (p == "wrong" or p == "refused") and C.error or C.onSurfaceVariant
    end,
    text = function() return M.info:get() end,
  },
  ui.Item {
    width = INNER, height = 40,
    ui.Row {
      anchors = { right = true }, gap = 8,
      button("polkit-cancel", "Cancel", false, function() M.cancel() end),
      button("polkit-ok", "Authenticate", true, function() M.submit() end),
    },
  },
}

local content = ui.Item {
  id = "polkit",
  anchors = { fill = true },
  column,
  keys,
}

M.drawer = drawer.new {
  name = "polkit",
  edge = "top",
  width = W,
  height = function() return (column.layout_height or 330) + 2 * PAD end,
  content = content,
}

-- Open, it has the keyboard; shut, the keyboard goes back.
morf.effect("caelestia.polkit.open", function()
  local open = M.drawer.open:get()
  if open then
    keys.focus = true
  else
    keys.focus = false
  end
end)

-- --------------------------------------------------------------- the view --

local here = require("services").here

--- How a request stands, from the screen that holds it. The screen in use
--- opens the dialog on a request it has not seen; the rest leave it be.
local function view(id, message, action, user, prompt, phase, info, failures)
  failures = tonumber(failures) or 0
  local final = phase == "done" or phase == "refused"
  if viewing ~= id then
    if final or not here() then return end
    viewing = id
    seen_failures = 0
    clear()
    -- The dashboard hangs from the same edge.
    local ok, dashboard = pcall(require, "dashboard")
    if ok and dashboard.drawer then dashboard.drawer.set(false) end
    M.drawer.set(true)
  end
  M.request:set({
    message = (message and message ~= "") and message or "An application wants to do something that needs your password.",
    action = action or "", user = user or "", prompt = prompt or "",
  })
  M.phase:set(phase or "asking")
  M.info:set(info or "")
  if failures > seen_failures then
    seen_failures = failures
    M.shake()
  end
  if final then
    viewing = nil
    clear()
    morf.timer(phase == "done" and 650 or 900, function()
      if not viewing then
        M.drawer.set(false)
        M.request:set(false)
        M.info:set("")
      end
    end, false)
  end
end

-- ------------------------------------------------------------ the holder --

-- The requests this screen holds -- the agent's, or a demo's -- by id:
-- `{ request, phase, info, failures }`.
local held = {}
local count = 0

local function publish(id)
  local entry = held[id]
  if not entry then return end
  local r = entry.request
  send("view", id, r.message or "", r.action_id or "", r.user or "", r.prompt or "",
    entry.phase, entry.info, entry.failures)
  if entry.phase == "done" or entry.phase == "refused" then held[id] = nil end
end

--- Takes a request in and says how it stands; `request.id` is its id here.
local function hold(request)
  local id = request.id
  if not id then
    count = count + 1
    id = tostring(morf.screens and morf.screens[1] and morf.screens[1].name or "") .. "." .. count
    request.id = id
  end
  local entry = held[id]
  if not entry then
    entry = { request = request, phase = "asking", info = "", failures = 0 }
    held[id] = entry
  end
  -- What PAM said: a prompt, or a line to show ("look at the camera").
  if request.info and request.info ~= "" then
    entry.info = request.info
    request.info = nil
  end
  if request.prompt and entry.phase == "checking" then entry.phase = "asking" end
  publish(id)
end

local function wrong(request)
  local entry = held[request.id]
  if not entry then return end
  entry.phase, entry.info, entry.failures = "wrong", "Wrong password, try again", entry.failures + 1
  publish(request.id)
end

local function finished(request, ok, why)
  local entry = held[request.id]
  if not entry then return end
  entry.phase = ok and "done" or "refused"
  entry.info = (not ok and why and not tostring(why):find("cancel")) and tostring(why) or ""
  publish(request.id)
end

local function answer(id, pw)
  local entry = held[id]
  if not entry then return end
  entry.phase, entry.info = "checking", ""
  publish(id)
  entry.request.answer(pw or "")
end

local function cancel(id)
  local entry = held[id]
  if entry then entry.request.cancel() end
end

--- Every screen's `polkit` verb, for what the screens say to each other.
function M.message(kind, ...)
  if kind == "view" then view(...)
  elseif kind == "answer" then answer(...)
  elseif kind == "cancel" then cancel(...) end
end

function send(kind, ...)
  if morf.broadcast and morf.broadcast("polkit", kind, ...) then return end
  M.message(kind, ...)
end

--- A request as polkit would send one, for trying the dialog out
--- (`morf ipc call polkit demo`): any password but "wrong" is taken, and
--- it goes nowhere.
function M.demo()
  local fake = { action_id = "org.freedesktop.systemd1.manage-units", user = morf.env("USER") or "you",
    message = "Authentication is required to start 'tor.service'.", prompt = "Password: " }
  function fake.answer(pw)
    morf.timer(900, function()
      if pw == "wrong" then wrong(fake) else finished(fake, true) end
    end, false)
  end
  function fake.cancel() finished(fake, false, "cancelled") end
  hold(fake)
end

-- -------------------------------------------------------------- the agent --

local agent
local function register()
  if agent or config.get("polkit.agent") == "off" or dry_run() then return true end
  -- One screen is the agent; the rest only ever draw.
  if morf.primary and not morf.primary() then return false, "not the primary screen" end
  local ok, lib = pcall(require, "lib.polkit_agent")
  if not ok then return true end
  local served, a, why = pcall(lib.serve, {
    retries = RETRIES,
    on_request = hold,
    on_failure = wrong,
    on_done = function(request, ok2, reason) finished(request, ok2, reason) end,
  })
  if served and a then
    agent = a
    M.registered:set(true)
    morf.log("info", "caelestia: the polkit agent")
    return true
  end
  return false, served and why or a
end

-- A moment after the shell is up, and then every minute while another
-- agent has the job (or this screen becomes the primary one).
local retry
morf.timer(1500, function()
  local ok, why = register()
  if ok then return end
  morf.log("info", "caelestia: not the polkit agent yet: " .. tostring(why))
  retry = morf.timer(60000, function()
    if register() and retry then retry:cancel() retry = nil end
  end, true)
end, false)

return M
