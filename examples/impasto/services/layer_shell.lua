-- Whether edge surfaces can be put at an edge.
--
-- Without wlr-layer-shell (a nested compositor, GNOME) a `morf.window.layer`
-- cannot be anchored, and an edge surface -- the dock, a deck's tabs --
-- would show up as a stray square over everything else, so those wait for
-- this to say yes. The engine names its protocols once it is connected,
-- after the configuration ran, so the answer is read on the first tick.

local M = {}

M.available = morf.signal("impasto.layer_shell", false)
morf.timer(1, function()
  M.available:set((morf.capabilities or {}).layer_shell == true)
end, false)

--- True once the compositor is known to have layer-shell.
function M.ok() return M.available:get() end

return M
