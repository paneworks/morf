-- Emoji by name: every one lib/emoji_data.lua knows (Unicode's own names),
-- searched fuzzily.
--
--   local emoji = require("lib.emoji")
--   for _, hit in ipairs(emoji.search("party", 20)) do print(hit.char, hit.name) end

local morf = require("morf")

local emoji = {}

local all -- { char, name } rows, read on first use

local function load()
  if all then return all end
  all = {}
  for i, row in ipairs(require("lib.emoji_data")) do
    all[i] = { char = row[1], name = row[2] }
  end
  return all
end

--- The emoji whose names match `term`, best first, at most `limit`.
function emoji.search(term, limit)
  local list = load()
  local out = {}
  if (term or "") == "" then
    for i = 1, math.min(limit or #list, #list) do out[i] = list[i] end
    return out
  end
  for _, hit in ipairs(morf.text.fuzzy(term, list, { key = "name" })) do
    out[#out + 1] = hit.item
    if limit and #out >= limit then break end
  end
  return out
end

return emoji
