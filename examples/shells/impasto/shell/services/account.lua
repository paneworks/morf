-- The account: whose session this is, by name and by picture.
--
-- Port of AccountService.qml and scripts/account.py, without the script.
-- The user is the process's own uid (/proc/self/status) looked up in
-- /etc/passwd, not $USER, which anything can set. The name is the GECOS
-- field (the part before the first comma); the picture the first of the four
-- places the login screen can also reach. A name or picture chosen in
-- Settings and kept only there wins over both (the older way, still
-- honoured).
--
-- Changing them changes the account itself, so the lock and the login
-- screen never disagree: the name through AccountsService's SetRealName
-- (which lets the active user change their own without a password), the
-- picture made square and 256 pixels with `morf.image.process` and handed,
-- as PNG on stdin, to `/usr/lib/impasto/avatar set` under pkexec -- the
-- part only root can do, from upstream's `./setup system`. Where neither is
-- there, the choice is kept in Settings as before. Both go through
-- services/act.lua: a dry run changes nothing.

local settings = require("services.settings")

local M = {}

local fs = morf.fs

M.HELPER = "/usr/lib/impasto/avatar"
M.POLICY = "/usr/share/polkit-1/actions/org.impasto.avatar.policy"
M.SIDE = 256

local function uid()
  local status = fs.read("/proc/self/status") or ""
  return tonumber(status:match("\nUid:%s*(%d+)"))
end

--- `{ name, gecos, home }` for a uid, from /etc/passwd.
local function passwd_by_uid(id)
  local text = fs.read("/etc/passwd") or ""
  for line in text:gmatch("[^\n]+") do
    local name, _, found, _, gecos, home = line:match("^([^:]*):([^:]*):(%d+):([^:]*):([^:]*):([^:]*):")
    if name and tonumber(found) == id then return { name = name, gecos = gecos or "", home = home or "" } end
  end
  return nil
end

local record = passwd_by_uid(uid() or -1)
local user = record and record.name or (morf.env("USER") or morf.env("LOGNAME") or "")
local full = ((record and record.gecos or ""):match("^([^,]*)") or ""):match("^%s*(.-)%s*$")

local s = {
  full = morf.signal("impasto.account.full", full),
  avatar = morf.signal("impasto.account.avatar", ""),
  accounts = morf.signal("impasto.account.accounts", false),
  busy = morf.signal("impasto.account.busy", ""),
  failure = morf.signal("impasto.account.failure", ""),
  revision = morf.signal("impasto.account.revision", 0),
}
M.signals = s

-- The greeter's faces directory first: it is the one place both the lock and
-- a login screen running as another user can read.
local function system_avatar()
  local home = (record and record.home ~= "" and record.home) or fs.home() or ""
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

local function read()
  if user ~= "" then s.avatar:set(system_avatar()) end
  local again = passwd_by_uid(uid() or -1)
  if again then
    s.full:set(((again.gecos or ""):match("^([^,]*)") or ""):match("^%s*(.-)%s*$"))
  end
end
read()

M.user = user
M.full_name = full

--- The account's own full name, as it is now ("" when none was set).
function M.full() return s.full:get() end

--- The name shown: Settings' choice, the full name, or the login name.
function M.name()
  local chosen = settings.userName
  if chosen ~= "" then return chosen end
  local now = s.full:get()
  return now ~= "" and now or user
end

--- The picture shown, a path, or "" for initials.
function M.avatar()
  local chosen = settings.userAvatar
  if chosen ~= "" then return chosen end
  s.revision:get()
  return s.avatar:get()
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

-- ------------------------------------------------------------ changing --

--- AccountsService is running: the name can be changed on the account.
function M.accounts() return s.accounts:get() end
--- The picture helper and its policy are there (upstream's ./setup system).
function M.shared() return fs.exists(M.HELPER) and fs.exists(M.POLICY) end
--- "name" or "picture" while a change is on its way.
function M.busy() return s.busy:get() end
--- Why the last change did not happen, or "".
function M.failure() return s.failure:get() end

local client
local account_path

local function bus()
  if client then return client end
  local ok, dbus_client = pcall(require, "lib.services.dbus_client")
  if ok then client = dbus_client.new { bus = "system" } end
  return client
end

-- AccountsService's object for this user, found once, without waiting.
local function find_account(done)
  if account_path then return done(account_path) end
  local c = bus()
  if not c or user == "" then return done(nil) end
  local sent = c.call1_async("org.freedesktop.Accounts", "/org/freedesktop/Accounts",
    "org.freedesktop.Accounts", "FindUserByName", { user }, 5000, function(path)
      if type(path) == "string" and path ~= "" then account_path = path end
      done(account_path)
    end)
  if not sent then done(nil) end
end
morf.timer(800, function()
  find_account(function(path) s.accounts:set(path ~= nil) end)
end, false)

-- The copy kept in Settings goes once the account has the change: a refused
-- write leaves the lock showing what it showed before.
local function settle(kind, why)
  s.busy:set("")
  s.failure:set(why or "")
  if not why then
    if kind == "picture" then settings.set("userAvatar", "") else settings.set("userName", "") end
    s.revision:set(s.revision:get() + 1)
  end
  read()
end

local name_timer
--- The account's full name. Waits for the typing to stop: one change, not
--- one a key. Without AccountsService it is kept in Settings.
function M.set_name(text)
  text = tostring(text or ""):match("^%s*(.-)%s*$")
  if not s.accounts:get() then
    settings.set("userName", text)
    return
  end
  if name_timer then name_timer:cancel() end
  name_timer = morf.timer(800, function()
    name_timer = nil
    s.busy:set("name")
    local act = require("services.act")
    if act.dry then
      morf.log("info", "impasto: dry run, not setting the account's name to " .. text)
      settings.set("userName", text)
      s.busy:set("")
      return
    end
    find_account(function(path)
      if not path then return settle("name", "AccountsService is not running") end
      local sent = bus().call_async("org.freedesktop.Accounts", path, "org.freedesktop.Accounts.User",
        "SetRealName", { text }, 25000, function(reply, err)
          if reply == nil then settle("name", tostring(err or "not changed")) else settle("name", nil) end
        end)
      if not sent then settle("name", "not changed") end
    end)
  end, false)
end

--- The picture: squared to 256 pixels here, as the user, and only the PNG
--- that comes out goes to the helper under pkexec. Without the helper it is
--- kept in Settings.
function M.set_picture(path)
  path = tostring(path or "")
  if path == "" then return M.clear_picture() end
  if not M.shared() then
    settings.set("userAvatar", path)
    return
  end
  if s.busy:get() ~= "" then return end
  s.busy:set("picture")
  local square = fs.join(fs.dir("runtime") or "/tmp", "impasto-avatar-" .. morf.time.now_ms() .. ".png")
  local ok, queued = pcall(morf.image.process, {
    source = path, output = square, format = "png",
    ops = { { "square" }, { "resize", M.SIDE, M.SIDE, "exact" } },
    on_done = function(done)
      if not done then
        fs.remove(square)
        return settle("picture", "could not read the picture")
      end
      local bytes = fs.read(square, 8 * 1024 * 1024)
      fs.remove(square)
      if not bytes then return settle("picture", "could not read the picture") end
      require("services.act").collect("setting the account's picture", "pkexec", { M.HELPER, "set" },
        function(_, done_ok)
          settle("picture", done_ok and nil or "not changed")
        end, { mutates = true, stdin = bytes, timeout_ms = 60000 })
    end,
  })
  if not ok or not queued then settle("picture", "could not read the picture") end
end

--- No picture at all.
function M.clear_picture()
  if not M.shared() then
    settings.set("userAvatar", "")
    return
  end
  if s.busy:get() ~= "" then return end
  s.busy:set("picture")
  require("services.act").collect("clearing the account's picture", "pkexec", { M.HELPER, "clear" },
    function(_, done_ok) settle("picture", done_ok and nil or "not changed") end,
    { mutates = true, timeout_ms = 60000 })
end

return M
