-- A desktop shell's on-screen display (the shell group's `osd`): the volume
-- or the brightness shown for a moment as it changes -- an icon and a level
-- on the dark osd ground, at the bottom centre of a surface (a Popup over
-- a progress bar).
--
--     local node, osd = composites.osd { id = "volume", root = surface_root,
--       icon = function() return muted:get() and "volume_off" or "volume_up" end,
--       value = function() return level:get() end }    -- 0..1
--     osd.show()                                         -- for `timeout` ms (1500)
--
-- The level is a progress bar in its osd variant; `width` (280). Ids:
-- `<id>`, `<id>-level`.
local ui = require("morf.ui")
local popup = require("lib.kit.archetypes.popup")
local widgets = require("lib.kit.widgets")

return function(spec)
  spec = spec or {}
  local W = spec.width or 280
  local content = ui.Row { gap = 12, align = "center", z = 1,
    widgets.image { icon = spec.icon or "volume_up", pixel_size = 22, color = "#ffffff" },
    widgets.progress_bar { id = spec.id and (spec.id .. "-level") or nil, width = W - 64, variant = "osd",
      fraction = spec.value } }
  local shown = popup.make("toast", { id = spec.id, content = content, padding = 14, root = spec.root,
    anchor = spec.anchor, placement = spec.placement or "center" })
  local timer
  local handle = { popup = shown }
  function handle.show()
    shown.open(spec.anchor)
    if timer then timer:cancel() end
    timer = morf.timer(spec.timeout or 1500, function() timer = nil shown.close("timeout") end, false)
  end
  function handle.hide() if timer then timer:cancel() timer = nil end shown.close("closed") end
  function handle.is_open() return shown.is_open() end
  return nil, handle
end
