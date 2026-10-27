-- AdwPasswordEntryRow: an entry row whose text is hidden, with a button
-- that shows it; the entry row's fields otherwise.
local entry_row = require("lib.kit.composites.rows.entry_row")

return function(spec)
  local s = {}
  for k, v in pairs(spec or {}) do s[k] = v end
  s.widget = "password_entry"
  return entry_row(s)
end
