-- The wallpaper and the palette that follows it start with the shell.
--
-- The wallpaper gets a background layer of its own. A nested compositor
-- without layer-shell (cage, used to test the shell) has nowhere to put it;
-- `IMPASTO_INLINE_WALLPAPER=1` draws it inside the bar's surface instead,
-- which init.lua does when it sees the flag.
require("services.theme")
local screen = (morf.screens or {})[1] or {}
if (morf.env and morf.env("IMPASTO_INLINE_WALLPAPER") or "") == "" then
  require("desktop.wallpaper").open_layer(tonumber(screen.width) or 1920, tonumber(screen.height) or 1080)
end
