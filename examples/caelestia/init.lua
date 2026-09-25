-- caelestia: a clean-room reimplementation of caelestia-dots/shell's look
-- and behaviour on morf, written from watching the original run (its
-- screenshots and films in a sandbox), not from its source. MIT.
--
-- The signature: a thin frame round the whole screen in the bar's colour,
-- with rounded inner corners; the bar down its left side; and drawers that
-- grow out of the frame with concave fillets -- the launcher at the bottom,
-- the dashboard at the top.
--
--     morf examples/caelestia/init.lua
--     morf ipc call launcher          -- toggle; or `launcher open`, `launcher close`
--     morf ipc call dashboard         -- the same; `session` too
--     morf ipc call close             -- every drawer
--
-- The frame, the bar and the drawers are one fullscreen layer surface; only
-- what can be clicked takes the pointer (the engine derives the input
-- region from the MouseAreas), so the desk under the opening stays usable.
-- The wallpaper is a background layer of its own.

local morf = require("morf")
local ui = require("morf.ui")

local config = require("config")
local theme = require("theme")
local drawer = require("drawer")
local wallpaper = require("wallpaper")
local bar = require("bar")

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
-- Windows keep inside the frame: the bar's width on the left, the frame's
-- thickness everywhere else.
morf.surface.reserve = { left = theme.BAR, top = theme.BORDER, right = theme.BORDER, bottom = theme.BORDER }

-- ------------------------------------------------------------------ colour --

morf.effect("caelestia.scheme", function()
  theme.follow(config.get("theme.source"), wallpaper.current:get())
  -- The variant and mode are read inside `follow`; name them here so a
  -- change of either re-runs this.
  config.get("theme.variant")
  config.get("theme.mode")
end)

-- ---------------------------------------------------------------- drawers --

local launcher = require("launcher")
local dashboard = require("dashboard")
local session = require("session")
require("popouts")
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
      left_margin = theme.BAR, top_margin = theme.BORDER,
      right_margin = theme.BORDER, bottom_margin = theme.BORDER,
    },
    radius = theme.ROUNDING,
    operation = "subtract",
  },
}
for _, d in ipairs(drawer.all) do field[#field + 1] = d.shape end

local panels = {
  id = "opening",
  anchors = {
    fill = true,
    left_margin = theme.BAR, top_margin = theme.BORDER,
    right_margin = theme.BORDER, bottom_margin = theme.BORDER,
  },
  clip = true,
  -- The desk dims under the session menu.
  session.dim(),
}
for _, d in ipairs(drawer.all) do panels[#panels + 1] = d.panel end

ui.Item {
  anchors = { fill = true },
  ui.Sdf(field),
  bar.build(),
  ui.Item(panels),
  dashboard.edge_trigger(),
}

wallpaper.open_layer()

-- -------------------------------------------------------------------- ipc --

local function verb(d)
  return function(how)
    how = how or "toggle"
    if how == "open" then d.set(true)
    elseif how == "close" then d.set(false)
    elseif how == "toggle" then d.toggle()
    elseif how ~= "state" then error("`" .. tostring(how) .. "`: open, close, toggle or state") end
    return d.is_open()
  end
end

morf.ipc.launcher = verb(launcher.drawer)
morf.ipc.dashboard = verb(dashboard.drawer)
morf.ipc.session = verb(session.drawer)
morf.ipc.sidebar = verb(sidebar.drawer)
morf.ipc.utilities = verb(utilities.drawer)
morf.ipc.workspace = function(n)
  require("services").workspace.go(n)
  return require("services").workspace.active()
end
morf.ipc.osd = function() osd.flash() return true end
-- `notify SUMMARY [BODY [critical|normal [APP]]]` raises a notification of
-- the shell's own, as the reference's toaster does.
morf.ipc.notify = function(summary, body, urgency, app)
  return notifs.push { summary = summary, body = body, urgency = urgency == "critical" and 2 or 1, app = app }
end
-- `popout NAME` opens the bar's popout NAME (network, bluetooth, power);
-- `popout` alone shuts it.
morf.ipc.popout = function(name)
  local popouts = require("popouts")
local osd = require("osd")
local notifs = require("notifs")
  popouts.current:set(name or "")
  return popouts.current:get()
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
