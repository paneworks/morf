-- Starts the notification daemon with the shell, in the primary runtime
-- only: the bus name has one owner, and every screen's runtime asking for
-- it was a race one of them always lost ("the notification name is taken").
-- When the primary's screen goes away the engine gives the name back before
-- the next primary hears it is one, so that one finds it free.
local notifications = require("services.notifications")

local function follow(primary)
  if primary then notifications.start() end
end
follow(morf.primary())
morf.on_primary(follow)
