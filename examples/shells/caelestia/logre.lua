-- logre: caelestia's greeter and lock as one file -- the entry a bundle of
-- both runs, so one binary in /usr/bin serves greetd and the lock key.
--
--   logre                          the greeter (greetd runs it in cage)
--   logre -- lock                  the lock, held under ext-session-lock
--   logre -- lock window [preview] the lock in a window, holding nothing
--
-- Bundled with its two parts and the library beside it:
--
--   morf bundle examples/shells/caelestia/logre.lua -o logre \
--     --with examples/shells/caelestia/greet --with examples/shells/caelestia/lock \
--     --with library/lib
--
-- Each part reads its own arguments from the first, so the word that chose
-- it is taken off before it runs.

local morf = require("morf")

if morf.operands[1] == "lock" then
  table.remove(morf.operands, 1)
  require("lock.init")
else
  require("greet.init")
end
