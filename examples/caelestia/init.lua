-- caelestia: a clean-room reimplementation of caelestia-dots/shell's look
-- and behaviour on morf, written from watching the original run (its
-- screenshots and films in a sandbox), not from its source. MIT.
--
-- The signature: a thin frame round the whole screen in the surface colour,
-- with rounded inner corners; the workspaces as pills down its left side; and drawers that
-- grow out of the frame with concave fillets -- the launcher at the bottom,
-- the dashboard at the top.
--
--     morf examples/caelestia/init.lua
--     morf ipc call launcher          -- toggle; or `launcher open`, `launcher close`
--     morf ipc call dashboard         -- the same; `session` too
--     morf ipc call close             -- every drawer
--
-- The frame, the rail and the drawers are one fullscreen layer surface; only
-- what can be clicked takes the pointer (the engine derives the input
-- region from the MouseAreas), so the desk under the opening stays usable.
-- The wallpaper is a background layer of its own.

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
local osd = require("osd")
local notifs = require("notifs")
local utilities = require("utilities")
local sidebar = require("sidebar")

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
  ui.SdfShape {
    shape = "box",
    anchors = {
      fill = true,
      left_margin = theme.LEFT, top_margin = theme.BORDER,
      right_margin = theme.BORDER, bottom_margin = theme.BORDER,
    },
    radius = theme.ROUNDING,
    operation = "subtract",
  },
}
for _, d in ipairs(drawer.all) do field[#field + 1] = d.shape end
-- The rail's swell is the frame's too.
local rail_node = rail.build()
field[#field + 1] = rail.shape

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
  -- A click on the desk shuts the sidebar.
  require("sidebar").catcher(),
}
for _, d in ipairs(drawer.all) do panels[#panels + 1] = d.panel end

ui.Item {
  anchors = { fill = true },
  ui.Sdf(field),
  -- The workspaces down the left edge: a pill each, the active one popping
  -- out into a numbered bud when it changes.
  rail_node,
  ui.Item(panels),
  dashboard.edge_trigger(),
  -- The bottom edge under the launcher opens it; the middle of the right
  -- edge opens the sidebar (and the utilities under it).
  require("hover").edge {
    name = "launcher", drawer = launcher.drawer, edge = "bottom",
    length = launcher.width, setting = "launcher.hover",
  },
  require("hover").edge {
    name = "sidebar", drawer = sidebar.drawer, edge = "right",
    length = function() return math.floor((morf.screens[1] and morf.screens[1].height or 1080) / 3) end,
    panels = { sidebar.drawer.panel, require("utilities").drawer.panel },
    setting = "sidebar.hover",
  },
}

wallpaper.open_layer()

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

morf.ipc.launcher = verb(launcher.drawer)
morf.ipc.dashboard = verb(dashboard.drawer)
morf.ipc.session = verb(session.drawer)
morf.ipc.sidebar = verb(sidebar.drawer)
morf.ipc.utilities = verb(utilities.drawer)
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
