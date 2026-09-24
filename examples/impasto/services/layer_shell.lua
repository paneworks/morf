-- Whether edge surfaces can be put at an edge.
--
-- `morf.capabilities.layer_surfaces` says a `morf.window.layer` lands where
-- its anchors, margins and size put it: through wlr-layer-shell, or without
-- it (a nested compositor, GNOME) as a subsurface of the shell's own window,
-- which the engine places the same way. Only with neither would an edge
-- surface -- the dock, a deck's tabs -- show up as a stray window over
-- everything else, so those wait for this to say yes. (`layer_shell` alone
-- names the real protocol, and an engine from before the fallback reports
-- only that.) The engine names its protocols once it is connected, after the
-- configuration ran, so the answer is read on the first tick.

local M = {}

M.available = morf.signal("impasto.layer_shell", false)
morf.timer(1, function()
  local caps = morf.capabilities or {}
  M.available:set(caps.layer_surfaces == true or caps.layer_shell == true)
end, false)

--- True once edge surfaces are known to land at their edge.
function M.ok() return M.available:get() end

return M
