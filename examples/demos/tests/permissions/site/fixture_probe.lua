-- An imported module that runs a program while it loads: `true`, which
-- does nothing.
local child, why = morf.spawn { command = { "true" } }
return { ran = child ~= nil, why = why }
