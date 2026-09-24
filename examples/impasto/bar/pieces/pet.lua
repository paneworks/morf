-- The pet on the bar: a module piece, placed with `settings.barLeft` or
-- `barRight` like any other ("pet" in either list).
--
-- The pet is not on the bar by default, as in impasto, whose defaults leave
-- it for the user to add. Being on the bar is also what keeps it company:
-- the service's trickle of experience runs only while it is.

local bar = require("bar.bar")
local module = require("bar.modules.pet")

bar.register("pet", { build = module.chip })
