-- Notifications: a bell (crossed out in Do not disturb) with the count
-- kept. The detail is the control centre's list, bare on the island.
--
-- Port of NotificationsModule.qml.

local settings = require("services.settings")
local modules = require("services.modules")
local list = require("bar.controls.notification_list")

modules.define("notifications", {
  glyph = function() return settings.doNotDisturb and "󰂛" or "󰂚" end,
  value = function() return tostring(list.count:get()) end,
  has = function() return true end,
  -- An empty list is short.
  size = function()
    local item = modules.entry("notifications")
    if list.count:get() == 0 then return { item.width, 124 } end
    return { item.width, item.height }
  end,
  detail = function()
    local w, h = modules.open_size("notifications")
    return list.build { bare = true, width = w - 8, height = h - 8 }
  end,
})
