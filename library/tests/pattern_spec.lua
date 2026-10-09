local validate = require("lib.util.pattern").validate
local function dots(s)
  local out = {}
  for digit in s:gmatch(".") do out[#out+1] = tonumber(digit) end
  return out
end
for _, pattern in ipairs {"123654", "14785", "1598", "123698745"} do
  assert(validate(dots(pattern)), "adjacent path rejected")
end
for _, pattern in ipairs {"123456", "1357", "1232", "123", "0123", "1236547891"} do
  assert(not validate(dots(pattern)), "invalid path accepted")
end
assert(not validate({1, 2, 3, 4.5}))
assert(not validate({1, 2, 3, "6"}))
print("Pattern UI validation passed")
