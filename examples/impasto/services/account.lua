-- The account: whose session this is, by name and by picture.
--
-- Port of the reading half of AccountService.qml. The original asked a Python
-- helper, which read the password database and looked for a picture in the
-- places the login screen can also reach; here the same is read directly:
-- the name from /etc/passwd's GECOS field (the part before the first comma),
-- the picture from the first of the same four places that exists. A name or
-- picture chosen in Settings wins over both. Changing them on the account
-- itself (AccountsService, the shared faces directory) stays with Settings'
-- port.

local settings = require("services.settings")

local M = {}

local fs = morf.fs

local function passwd_entry(user)
  local text = fs.read("/etc/passwd") or ""
  for line in text:gmatch("[^\n]+") do
    local name, _, _, _, gecos = line:match("^([^:]*):([^:]*):([^:]*):([^:]*):([^:]*):")
    if name == user then return gecos or "" end
  end
  return ""
end

local user = morf.env("USER") or morf.env("LOGNAME") or ""
local full = (passwd_entry(user):match("^([^,]*)") or ""):match("^%s*(.-)%s*$")

-- The greeter's faces directory first: it is the one place both the lock and
-- a login screen running as another user can read.
local function system_avatar()
  local home = fs.home() or ""
  for _, candidate in ipairs {
    "/var/lib/impasto/faces/" .. user .. ".face.icon",
    home .. "/.face",
    home .. "/.face.icon",
    "/var/lib/AccountsService/icons/" .. user,
  } do
    local info = fs.stat(candidate)
    if info and info.is_file then return candidate end
  end
  return ""
end
local avatar_path = user ~= "" and system_avatar() or ""

M.user = user
M.full_name = full

--- The name shown: Settings' choice, the full name, or the login name.
function M.name()
  local chosen = settings.userName
  if chosen ~= "" then return chosen end
  return full ~= "" and full or user
end

--- The picture shown, a path, or "" for initials.
function M.avatar()
  local chosen = settings.userAvatar
  if chosen ~= "" then return chosen end
  return avatar_path
end

--- At most two initials, for when there is no picture.
function M.initials()
  local words = {}
  for word in M.name():gmatch("%S+") do words[#words + 1] = word end
  if #words == 0 then return "?" end
  -- The first character, not the first byte: names are not all ASCII.
  local function initial(word) return (word:match("^[\1-\127\194-\244][\128-\191]*") or ""):upper() end
  if #words == 1 then return initial(words[1]) end
  return initial(words[1]) .. initial(words[#words])
end

return M
