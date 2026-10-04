-- A configuration that loads the default kit as `require("kit")` and draws
-- nothing, for `morf check --kit`. The variant comes from MORF_KIT_VARIANT
-- ("dark", "light", "high_contrast"; unset: the desktop's preference).
local variant = morf.env("MORF_KIT_VARIANT")
if variant == false or variant == "" then variant = nil end
package.loaded.kit = require("lib.kit.skins.default").make { variant = variant }
-- The one root a configuration must have: empty.
require("morf.ui").Item { width = 1, height = 1 }
