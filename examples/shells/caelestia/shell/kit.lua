-- Visual components supplied by the selected theme. Services and state live
-- outside theme packages; both themes receive the same wallpaper palette.
local theme = require("theme")
local kit = require(theme.appearance.components)(theme)
-- The theme's skins draw the kit's archetypes (lib.kit.skin).
if kit.skins then
  local skin = require("lib.kit.skin")
  skin.define(theme.appearance.id, { skins = kit.skins, defaults = kit.skin_defaults })
  skin.use(theme.appearance.id)
end
return kit
