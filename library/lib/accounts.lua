-- The machine's people: the accounts a login screen lists, and the picture
-- each keeps of itself.
--
--   local accounts = require("lib.accounts")
--   for _, a in ipairs(accounts.list()) do print(a.name, a.label, a.face) end
--   local me = accounts.me()          -- whoever runs this, listed or not
--
-- Read straight out of /etc/passwd: the file is world-readable and the
-- authority, and a login screen that cannot list anybody because a daemon is
-- not running is worse than one that reads a file. Below uid 1000 is the
-- system's own, 65534 is `nobody`, and a shell that cannot be logged into is
-- an account that cannot log in.

local morf = require("morf")

local accounts = {}

--- The picture an account keeps of itself, if it keeps one: what
--- AccountsService holds for it, else the `.face` in its home.
function accounts.face(name, home)
  home = home or ("/home/" .. name)
  for _, path in ipairs {
    "/var/lib/AccountsService/icons/" .. name,
    home .. "/.face",
    home .. "/.face.icon",
  } do
    if morf.fs.exists(path) then return path end
  end
  return nil
end

local function entry(name, gecos, home)
  local label = (gecos and gecos ~= "" and gecos:match("^[^,]*")) or name
  if label == "" then label = name end
  return {
    name = name,
    label = label,
    initial = label:sub(1, 1):upper(),
    home = home or ("/home/" .. name),
    face = accounts.face(name, home),
  }
end

--- Ordinary human accounts, by name: `{ name, label, initial, home, face }`.
function accounts.list(path)
  local found = {}
  local ok, text = pcall(morf.fs.read, path or "/etc/passwd")
  if not ok or type(text) ~= "string" then return found end
  for line in text:gmatch("[^\n]+") do
    local name, uid, gecos, home, shell = line:match("^([^:]+):[^:]*:(%d+):[^:]*:([^:]*):([^:]*):([^:]*)$")
    local id = tonumber(uid or "")
    if name and id and id >= 1000 and id < 65534
      and not (shell:match("nologin$") or shell:match("/false$")) then
      found[#found + 1] = entry(name, gecos, home)
    end
  end
  table.sort(found, function(a, b) return a.name < b.name end)
  return found
end

--- Whoever runs this: from the list when there, made up from `$USER`
--- otherwise -- a lock asks exactly this person.
function accounts.me()
  local me = morf.env("USER") or morf.env("LOGNAME") or ""
  for _, a in ipairs(accounts.list()) do
    if a.name == me then return a end
  end
  if me == "" then return nil end
  return entry(me, nil, morf.fs.home())
end

return accounts
