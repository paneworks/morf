-- Which layer the island is showing.
--
-- Port of IslandState.qml. Layers are ranked and the island shows the
-- highest that wants the screen:
--
--     panel         opened by the user      until dismissed
--     notification  something arrived       until it expires or is closed
--     osd           volume, brightness...   shown, then expires
--     summary       the pointer rests on it  until it leaves
--     modules       nothing happening       always
--
-- A notification outranks an OSD, which only confirms something the user
-- just did. The glance sits just above rest because it restates what was
-- already true. Adding a layer means giving it a rank here.

local M = {}

M.LAYERS = { "modules", "summary", "osd", "notification", "panel" }
M.TRANSIENT_MS = 1800

local s = {
  active = morf.signal("impasto.island.active", true),
  panel = morf.signal("impasto.island.panel", ""),
  summary = morf.signal("impasto.island.summary", false),
  notification = morf.signal("impasto.island.notification", false),
  osd = morf.signal("impasto.island.osd", false),
  osd_icon = morf.signal("impasto.island.osd.icon", ""),
  osd_label = morf.signal("impasto.island.osd.label", ""),
  osd_progress = morf.signal("impasto.island.osd.progress", -1),
}
M.signals = s

--- The layer on top now; a binding that calls it follows it.
function M.layer()
  if not s.active:get() then return "modules" end
  if s.panel:get() ~= "" then return "panel" end
  if s.notification:get() then return "notification" end
  if s.osd:get() then return "osd" end
  if s.summary:get() then return "summary" end
  return "modules"
end

function M.open_panel() return s.panel:get() end
function M.expanded() return s.panel:get() ~= "" end

-- Called when a panel opens, so the notification it replaces is set aside
-- (not answered) by whoever owns notifications.
M.on_panel_opened = nil

local osd_generation = 0

--- A transient event: an icon, a label and, when there is one, a fraction
--- 0..1. Dropped while a panel is open -- shown later it would be stale.
function M.flash(icon, label, progress)
  if not s.active:get() or s.panel:get() ~= "" then return end
  s.osd_icon:set(icon or "")
  s.osd_label:set(label or "")
  s.osd_progress:set(progress or -1)
  s.osd:set(true)
  osd_generation = osd_generation + 1
  local mine = osd_generation
  morf.timer(M.TRANSIENT_MS, function()
    if mine == osd_generation then s.osd:set(false) end
  end, false)
end

function M.open(panel)
  if not s.active:get() then return end
  -- A panel takes over at once; whatever was flashing is now noise.
  s.osd:set(false)
  osd_generation = osd_generation + 1
  s.summary:set(false)
  s.panel:set(panel)
  if M.on_panel_opened then M.on_panel_opened(panel) end
end

function M.close() s.panel:set("") end

--- The same key opens and closes.
function M.toggle(panel)
  if s.panel:get() == panel then M.close() else M.open(panel) end
end

--- Handing the island to another screen puts this one back to rest.
function M.set_active(active)
  s.active:set(active)
  if not active then
    s.panel:set("")
    s.summary:set(false)
    s.osd:set(false)
  end
end

function M.set_summary(on) s.summary:set(on and true or false) end
function M.set_notification(on) s.notification:set(on and true or false) end

return M
