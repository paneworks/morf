-- caelestia: a clean-room reimplementation of caelestia-dots/shell's look
-- and behaviour on morf, written from watching the original run (its
-- screenshots and films in a sandbox), not from its source. MIT.
--
-- The signature: a thin frame round the whole screen in the surface colour,
-- with rounded inner corners; the workspaces as pills down its left side; and drawers that
-- grow out of the frame with concave fillets -- the launcher at the bottom,
-- the dashboard at the top.
--
--     morf examples/shells/caelestia/shell/init.lua
--     morf ipc call launcher          -- toggle; or `launcher open`, `launcher close`
--     morf ipc call dashboard         -- the same; `session` too
--     morf ipc call close             -- every drawer
--
-- The frame, the rail and the drawers are one fullscreen layer surface; only
-- what can be clicked takes the pointer (the engine derives the input
-- region from the MouseAreas), so the desk under the opening stays usable.
-- The wallpaper, when the shell paints it (`wallpaper.draw`), is a
-- background layer of its own.

local morf = require("morf")
local ui = require("morf.ui")

local config = require("config")
local theme = require("theme")
local drawer = require("drawer")
local wallpaper = require("wallpaper")
local rail = require("rail")

-- Qt mixes translucent colours in sRGB; so does the original.
morf.surface.blend = "srgb"
morf.surface.namespace = "caelestia-drawers"
morf.surface.anchors = { top = true, bottom = true, left = true, right = true }
morf.surface.width = 0
morf.surface.height = 0
morf.surface.layer = "top"
morf.surface.keyboard_focus = "none"
-- The whole output, whatever other surfaces reserve (our own reservers
-- below among them).
morf.surface.exclusive_zone = -1
-- Windows keep inside the frame.
morf.surface.reserve = { left = theme.LEFT, top = theme.BORDER, right = theme.BORDER, bottom = theme.BORDER }

-- ------------------------------------------------------------------ colour --

-- Two palettes: `theme.color`, the Material scheme, built from
-- `theme.source` -- "lule" (the colour tool's accent: lule, pywal; see
-- terminal_colors.lua), "wallpaper", a colour, or "auto" (lule when it has
-- set anything, else the wallpaper) -- and `theme.lule`, the tool's own
-- colours as they are, for whatever wants them.
local terminal_colors = require("terminal_colors")
morf.effect("caelestia.lule", function() theme.apply_lule(terminal_colors.current:get()) end)
morf.effect("caelestia.scheme", function()
  local source = config.get("theme.source")
  local tool = terminal_colors.current:get()
  if (source == "auto" or source == "lule") and tool then
    theme.follow(tool.accent, nil, tool.mode)
  else
    theme.follow(source == "auto" and "wallpaper" or source, wallpaper.current:get())
  end
  -- The variant and mode are read inside `follow`; name them here so a
  -- change of either re-runs this.
  config.get("theme.variant")
  config.get("theme.mode")
end)

-- ---------------------------------------------------------------- drawers --

local launcher = require("launcher")
local dashboard = require("dashboard")
local session = require("session")
local polkit = require("polkit")
local authsteps = require("authsteps")
local osd = require("osd")
local notifs = require("notifs")
local sidebar = require("sidebar")
local leftbar = require("leftbar")
local capture = require("capture")
local bottom = require("bottom")
-- These two drawers occupy the same edge; only one can be open.
for _, pair in ipairs { { capture.drawer, bottom.drawer }, { bottom.drawer, capture.drawer } } do
  local own, other = pair[1], pair[2]
  function own.set(on)
    if on then other.open:set(false) end
    own.open:set(on and true or false)
  end
  function own.toggle() own.set(not own.open:get()) end
end
local keyboard = require("keyboard")
local bar = require("bar")

-- One policy for the shared surface. Closing a launcher or auth dialog
-- must not disable typing in the task editor that is still open.
morf.effect("caelestia.keyboard.focus", function()
  local exclusive = launcher.drawer.open:get() or session.drawer.open:get() or polkit.drawer.open:get()
  local planner_open = leftbar.drawer.open:get()
  morf.surface.keyboard_focus = exclusive and "exclusive" or planner_open and "on_demand" or "none"
end)

-- ------------------------------------------------------------------- frame --

-- The frame: the whole screen as one box, less the rounded opening inside
-- it, and every drawer's background joined to it by a circular seam.
local field = {
  id = "frame",
  anchors = { fill = true },
  fill_color = function() return theme.color.surface end,
  blend = theme.SEAM,
  blend_profile = "circular",
  -- A soft shadow the frame throws into the opening, as the reference's.
  shadow_color = "#000000a0",
  shadow_blur = 6,
  ui.SdfShape { shape = "box", anchors = { fill = true } },
  -- The opening, drawn back on the bar's side when it is up.
  ui.SdfShape {
    shape = "box",
    x = function() local x = bar.desk() return x + theme.LEFT end,
    y = function() local _, y = bar.desk() return y + theme.BORDER end,
    width = function() local _, _, w = bar.desk() return w - theme.LEFT - theme.BORDER end,
    height = function() local _, _, _, h = bar.desk() return h - 2 * theme.BORDER end,
    radius = theme.ROUNDING,
    operation = "subtract",
  },
}
for _, d in ipairs(drawer.all) do field[#field + 1] = d.shape end
-- The rail's swell is the frame's too.
local rail_node = rail.build()
field[#field + 1] = rail.shape
-- And the levels' swell, down the right edge.
local levels = require("levels")
local levels_node = levels.build()
field[#field + 1] = levels.shape

local panels = {
  id = "opening",
  anchors = {
    fill = true,
    left_margin = theme.LEFT, top_margin = theme.BORDER,
    right_margin = theme.BORDER, bottom_margin = theme.BORDER,
  },
  clip = true,
  -- The desk dims under the session menu.
  session.dim(),
  -- A click on the desk shuts the sidebar, and the launcher.
  require("sidebar").catcher(),
  -- And the dashboard, however it was opened.
  dashboard.catcher(),
  ui.MouseArea {
    id = "bottom-catcher", anchors = { fill = true },
    visible = function() return bottom.drawer.open:get() end,
    on_clicked = function()
      -- Empty space inside a tab is still part of the panel. Only clicks
      -- on the surrounding desktop dismiss this workspace.
      if not bottom.drawer.panel.contains_pointer then bottom.drawer.set(false) end
    end,
  },
  ui.MouseArea {
    id = "capture-catcher", anchors = { fill = true },
    visible = function() return capture.drawer.open:get() end,
    on_clicked = function() capture.drawer.set(false) end,
  },
  ui.MouseArea {
    id = "launcher-catcher",
    anchors = { fill = true },
    visible = function() return launcher.drawer.open:get() end,
    on_clicked = function() launcher.drawer.set(false) end,
  },
}
for _, d in ipairs(drawer.all) do panels[#panels + 1] = d.panel end

ui.Item {
  anchors = { fill = true },
  ui.Sdf(field),
  -- The bar, in the frame's edge, when it is up.
  bar.build(),
  -- The desk: the screen less the bar. Everything that hangs off the
  -- frame's edges lives in it, so it stays on the opening's edge.
  ui.Item {
  id = "desk",
  x = function() local x = bar.desk() return x end,
  y = function() local _, y = bar.desk() return y end,
  width = function() local _, _, w = bar.desk() return w end,
  height = function() local _, _, _, h = bar.desk() return h end,
  -- The workspaces down the left edge: a pill each, the active one popping
  -- out into a numbered bud when it changes.
  rail_node,
  levels_node,
  ui.Item(panels),
  dashboard.edge_trigger(),
  -- The bottom edge opens the tabbed assistant workspace.
  require("hover").edge {
    name = "bottom", drawer = bottom.drawer, edge = "bottom",
    length = function() return bottom.WIDTH end, setting = "bottom.hover",
    enabled = function()
      local phase = capture.phase:get()
      return not capture.drawer.open:get() and (phase == "ready" or phase == "error")
    end,
  },
  -- Near the right edge, anywhere down it, the sidebar opens; near the
  -- left edge (the rail's pills with it), the left panel.
  require("hover").edge {
    name = "sidebar", drawer = sidebar.drawer, edge = "right",
    from = function() return theme.BORDER + theme.ROUNDING end,
    length = function()
      local _, _, _, h = bar.desk()
      return h - 2 * (theme.BORDER + theme.ROUNDING)
    end,
    setting = "sidebar.hover",
  },
  require("hover").edge {
    name = "leftbar", drawer = leftbar.drawer, edge = "left",
    from = function() return theme.BORDER + theme.ROUNDING end,
    length = function()
      local _, _, _, h = bar.desk()
      return h - 2 * (theme.BORDER + theme.ROUNDING)
    end,
    setting = "leftbar.hover",
  },
  },
}

-- Windows keep inside the opening: the frame, and the bar when it is up.
morf.effect("caelestia.bar.reserve", function()
  local i = bar.insets()
  morf.surface.reserve = {
    left = theme.LEFT + i.left, top = theme.BORDER + i.top,
    right = theme.BORDER + i.right, bottom = theme.BORDER + i.bottom,
  }
end)

if config.get("wallpaper.draw") then wallpaper.open_layer() end

-- -------------------------------------------------------------------- ipc --

-- Every screen runs the shell and hears every verb. What opens appears on
-- the focused screen only (`services.here()`); a close shuts it wherever it
-- is. A screen that does nothing answers nothing, so the reply is the one
-- that acted.
local here = require("services").here
local function verb(d)
  return function(how)
    how = how or "toggle"
    if how == "close" then
      d.set(false)
      if here() then return d.is_open() end
      return nil
    end
    if how ~= "open" and how ~= "toggle" and how ~= "state" then
      error("`" .. tostring(how) .. "`: open, close, toggle or state")
    end
    if not here() then
      -- Opened elsewhere now: shut here, so one screen has it at a time.
      if how ~= "state" then d.set(false) end
      return nil
    end
    if how == "open" then d.set(true)
    elseif how == "toggle" then d.toggle() end
    return d.is_open()
  end
end

-- `launcher [how]`, or `launcher apps` / `launcher web`: the launcher on
-- one of the author's own menus (menus.lua: appy's apps, browsy's web).
morf.ipc.launcher = function(how)
  if how == "apps" or how == "web" then
    if not here() then return nil end
    require("menus").open(how)
    launcher.drawer.set(true)
    return true
  end
  return verb(launcher.drawer)(how)
end
morf.ipc.dashboard = verb(dashboard.drawer)
morf.ipc["dashboard-history"] = function(output)
  local name = (morf.screens[1] or {}).name
  if output and output ~= name then return end
  local status = require("dashboard_state").history_status()
  status.output = name
  return status
end
morf.ipc.session = verb(session.drawer)
-- `polkit` says whether this screen is the agent and what it is asking;
-- `polkit demo` opens the dialog on a made-up request (any password but
-- "wrong" is taken, and it goes nowhere). `view`, `answer` and `cancel`
-- are the screens talking to each other (polkit.lua).
morf.ipc.polkit = function(how, ...)
  if how == "demo" then
    if not here() then return nil end
    polkit.demo()
    return true
  end
  if how == "view" or how == "answer" or how == "cancel" then
    polkit.message(how, ...)
    return nil
  end
  -- The state, from the agent's screen.
  if not polkit.registered:get() then return nil end
  local r = polkit.request:get()
  return { agent = true, open = polkit.drawer.open:get(), phase = polkit.phase:get(), action = r and r.action or "" }
end

-- `auth-step STEP [SERVICE]`: the markers in a PAM stack (tools/pam)
-- saying where sudo has got to: face, finger, password, ok.
morf.ipc["auth-step"] = function(step, service) return authsteps.steps.mark(step, service) end

-- `sidebar [how [TAB]]`: TAB is settings or notifications. `utilities`
-- is the sidebar on its settings; `settings PAGE` opens one of their pages
-- (network, bluetooth, sound).
morf.ipc.sidebar = function(how, tab)
  if tab and here() then
    if not sidebar.select(tab) then error("`" .. tostring(tab) .. "`: no such tab") end
  end
  return verb(sidebar.drawer)(how)
end
morf.ipc.utilities = function(how)
  if here() and how ~= "close" then sidebar.select("settings") end
  return verb(sidebar.drawer)(how)
end
morf.ipc.settings = function(page)
  if not here() then return nil end
  sidebar.select("settings")
  require("utilities").detail:set(page or "")
  sidebar.drawer.set(true)
  return page or ""
end
morf.ipc.leftbar = function(how, tab)
  if tab and here() and not leftbar.panel.select(tab) then error("No such left panel tab: " .. tostring(tab)) end
  return verb(leftbar.drawer)(how)
end
for _, tab in ipairs { "tasks", "calendar" } do
  morf.ipc[tab] = function(how)
    if here() and how ~= "close" then leftbar.panel.select(tab) end
    return verb(leftbar.drawer)(how or "open")
  end
end
-- `capture [how]` opens the capture drawer. `screenshot [WHAT]` and
-- `record [WHAT]` (again: stop) take one at once, of
-- region, window or screen (the chosen one by default).
morf.ipc.capture = verb(capture.drawer)
morf.ipc.bottom = function(how, tab)
  if tab and here() and not bottom.panel.select(tab) then error("No such bottom panel tab: " .. tostring(tab)) end
  return verb(bottom.drawer)(how)
end
morf.ipc.assistant = function(how)
  if here() and how ~= "close" then bottom.panel.select("assistant") end
  return verb(bottom.drawer)(how or "open")
end
-- `keyboard [how]`: the on-screen keyboard, for a key to bind. `how` is
-- open, close, toggle or state, or a mode to open it in: full, dev,
-- letters, numbers, phone or pattern.
do
  local open_close = verb(keyboard.drawer)
  local MODES = { full = true, dev = true, letters = true, numbers = true, phone = true, pattern = true }
  morf.ipc.keyboard = function(how)
    if MODES[how] then
      if not here() then return nil end
      keyboard.show(how)
      return true
    end
    return open_close(how)
  end
end
morf.ipc.screenshot = function(what)
  if not here() then return nil end
  return capture.shoot(what)
end
morf.ipc.record = function(what)
  if not here() then return nil end
  return capture.record(what)
end
morf.ipc.workspace = function(n)
  if not here() then return nil end
  require("services").workspace.go(n)
  return require("services").workspace.active()
end
-- `lule`: the terminal the colour tool writes to, and the accents in use.
morf.ipc.lule = function()
  local tty = require("terminal_colors").tty
  return tty and tty.path or "", theme.lule.accent:hex(), theme.color.primary:hex()
end
morf.ipc.osd = function()
  if not here() then return nil end
  osd.flash()
  return true
end
-- `notify SUMMARY [BODY [critical|normal [APP]]]` raises a notification of
-- the shell's own, as the reference's toaster does.
morf.ipc.notify = function(summary, body, urgency, app)
  return notifs.push { summary = summary, body = body, urgency = urgency == "critical" and 2 or 1, app = app }
end
morf.ipc.close = function()
  drawer.close_all()
  return true
end
-- `drawers` lists the open drawers; `drawers toggle NAME` (open, close)
-- acts on one by name, as the reference's IPC does.
morf.ipc.drawers = function(how, name)
  if how ~= nil and how ~= "list" then
    local d = drawer[name or ""]
    if not d then error("`" .. tostring(name) .. "`: no such drawer") end
    return verb(d)(how)
  end
  local open = {}
  for _, d in ipairs(drawer.all) do
    if d.is_open() then open[#open + 1] = d.name end
  end
  return table.concat(open, " ")
end
