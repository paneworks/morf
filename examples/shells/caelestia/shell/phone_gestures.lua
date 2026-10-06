-- Touch gestures belong to the whole frame, including touches that start
-- on its bar or workspace rail. Only the edge strips claim new input area;
-- the middle of the desktop continues to belong to application windows.
local ui = require("morf.ui")
local M = {}
local attached = {}
local EDGE = 20 -- Matches the runtime's on_edge_swiped recognition zone.

local function phone() return require("responsive").portrait() end

local function blocked()
  for _, name in ipairs { "polkit", "keyring", "authsteps", "session" } do
    local controller = package.loaded[name]
    if controller and controller.drawer and controller.drawer.open:get() then return true end
  end
  local capture = package.loaded.capture
  return capture and capture.editor and capture.editor.active:get()
end

local function close_other_panels(keep)
  for _, drawer in ipairs(require("drawer").all) do
    if drawer ~= keep then drawer.set(false) end
  end
end

function M.swipe(edge)
  if not phone() or blocked() then return end
  if edge == "top" then
    local sidebar = require("sidebar")
    -- The first pull shows notifications; another pull opens quick settings.
    local settings = sidebar.drawer.open:get() and sidebar.showing("notifications")
    close_other_panels(sidebar.drawer)
    sidebar.select(settings and "settings" or "notifications")
    sidebar.drawer.set(true)
  elseif edge == "bottom" then
    local dashboard = require("dashboard").drawer
    close_other_panels(dashboard)
    dashboard.set(true)
  elseif edge == "left" or edge == "right" then
    close_other_panels()
    require("services").workspace.step(edge == "left" and -1 or 1)
  end
end

function M.attach(root)
  if attached[root] or not phone() then return end
  attached[root] = true
  root.on_edge_swiped = M.swipe
  -- Under the existing controls: tapping the bar/rail still reaches them.
  -- An Item, rather than a fullscreen MouseArea, leaves apps interactive.
  ui.reparent(ui.Item {
    id = "phone-gesture-edges", anchors = { fill = true }, z = -1,
    ui.MouseArea { id = "phone-gesture-top", height = EDGE,
      anchors = { top = true, left = true, right = true } },
    ui.MouseArea { id = "phone-gesture-bottom", height = EDGE,
      anchors = { bottom = true, left = true, right = true } },
    ui.MouseArea { id = "phone-gesture-left", width = EDGE,
      anchors = { left = true, top = true, bottom = true, top_margin = EDGE, bottom_margin = EDGE } },
    ui.MouseArea { id = "phone-gesture-right", width = EDGE,
      anchors = { right = true, top = true, bottom = true, top_margin = EDGE, bottom_margin = EDGE } },
  }, root)
end

return M
