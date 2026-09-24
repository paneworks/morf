-- The GitHub contribution wall, for the desk's GitHub faces.
--
-- Port of GithubService.qml over `lib.github`, which lands in lua-stdlib
-- separately. Until it does, or with no `githubUser` set, the wall is
-- unavailable and the faces say why. The reading is taken as the library
-- gives it: `weeks` (one list of seven levels 0..4 per week, Sunday first,
-- -1 or nil for days outside the range), `total`, `streak`, `today`.

local settings = require("services.settings")

local M = {}

local ok, lib = pcall(require, "lib.github")
if not ok then lib = nil end

local handle, handle_user = nil, nil
local EMPTY = { available = false, weeks = {}, total = 0, streak = 0, today = 0 }

local function user()
  return (settings.githubUser or ""):match("^%s*(.-)%s*$")
end

local function source()
  if not lib then return nil end
  local name = user()
  if name == "" then return nil end
  if handle == nil or handle_user ~= name then
    handle_user = name
    local okn, made = pcall(function()
      if lib.new then return lib.new { user = name } end
      if lib.contributions then return lib.contributions(name) end
      return nil
    end)
    handle = okn and made or false
  end
  return handle or nil
end

--- The reading; a binding follows it.
function M.now()
  local s = source()
  if not s then return EMPTY end
  local okr, value = pcall(function()
    if type(s) == "table" and s.get then return s:get() end
    return s
  end)
  if not okr or type(value) ~= "table" then return EMPTY end
  if value.available == nil then value.available = type(value.weeks) == "table" and #value.weeks > 0 end
  return value
end

function M.available() return M.now().available == true end
function M.weeks() return M.now().weeks or {} end
function M.total() return tonumber(M.now().total) or 0 end
function M.streak() return tonumber(M.now().streak) or 0 end
function M.user_set() return user() ~= "" end

--- "3689" as GitHub writes it, "3,689".
function M.grouped(n)
  local s = tostring(math.floor(tonumber(n) or 0))
  local out = s:reverse():gsub("(%d%d%d)", "%1,"):reverse()
  return (out:gsub("^,", ""))
end

--- Why the wall is empty.
function M.reason()
  if not M.user_set() then return "No GitHub user set" end
  return "GitHub is out of reach"
end

return M
