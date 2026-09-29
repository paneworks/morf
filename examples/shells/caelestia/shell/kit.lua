-- Visual components supplied by the selected theme. Services and state live
-- outside theme packages; both themes receive the same wallpaper palette.
local theme = require("theme")
return require(theme.appearance.components)(theme)
