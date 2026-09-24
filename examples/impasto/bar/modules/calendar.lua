-- Calendar: the day as a figure ("Thu 24"), and the control centre's month
-- as its detail, bare on the island (DetailFace.qml reuses CalendarCard
-- rather than drawing a second month).
--
-- Port of CalendarModule.qml's chip.

local modules = require("services.modules")
local card = require("bar.controls.calendar_card")

modules.define("calendar", {
  glyph = function() return "󰃭" end,
  -- Only changes at midnight.
  value = function()
    card.today:get()
    return morf.time.format("%a %-d")
  end,
  has = function() return true end,
  detail = function()
    local w, h = modules.open_size("calendar")
    return card.build { bare = true, padding = 12, width = w - 8, height = h - 8 }
  end,
})
