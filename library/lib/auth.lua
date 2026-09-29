-- The two doors a login screen has, behind one interface: a lock's, asked
-- through PAM, and a greeter's, asked through greetd.
--
--   local auth = require("lib.auth")
--   local door = auth.lock {                -- or auth.greeter { ... }
--     user = "trim",
--     on_busy = function(busy) end,         -- a verdict is being waited on
--     on_info = function(text, bad) end,    -- what the stack says ("touch the reader")
--     on_failed = function(why) end,        -- a wrong answer
--     on_open = function() end,             -- in: lift the lock, or the session starts
--   }
--   door:submit(password)
--   door.session = sessions.list()[1]      -- the greeter: what to start
--   door:switch("someone")                 -- the greeter: another account
--   door:stop()
--
-- The password is an argument, never a signal: signals are named, observable
-- and kept, and a secret wants none of that.
--
-- A lock has two stacks. The password goes to the password stack --
-- `morf-lock` when the machine has one, else `system-auth`, else `login` --
-- and gets its verdict in the time a hash takes. The interactive stack,
-- `login`, is where a machine puts a fingerprint reader or a face: that one is
-- listened to the whole time the lock is up, in a conversation of its own, and
-- whatever it says is passed on. One stack for both would put the reader
-- ahead of the password and wait for a finger before hearing a word.

local morf = require("morf")

local auth = {}

local function noop() end

local function handlers(options)
  return {
    busy = options.on_busy or noop,
    info = options.on_info or noop,
    failed = options.on_failed or noop,
    open = options.on_open or noop,
  }
end

--- The PAM services a lock uses: the password's, and the reader's (or nil).
---
--- The reader -- a fingerprint, a face -- is listened to on a stack of its
--- own, `/etc/pam.d/morf-lock-reader`, and only when there is one. A stack
--- that goes on to a password after the reader (`login`, `system-auth`)
--- cannot be listened to: when the reader gives up it asks for the password,
--- the listener has none to give, and pam_faillock counts that as a failed
--- login -- a lock listening in a loop locked its own person out in
--- seconds. The reader's stack is the module and nothing to fall through
--- to, e.g.
---
---     auth  sufficient  pam_fprintd.so      (or pam_gaze.so, pam_howdy.so)
---     auth  required    pam_deny.so
---     account include   system-auth
--- The readers a lock can listen to, those this machine has a stack for,
--- in order: `morf-lock-finger` (pam_fprintd), `morf-lock-face` (pam_gaze,
--- pam_howdy) and `morf-lock-reader` (anything else). Each is the module
--- and `pam_deny` after it, as above, so a miss is never a failed login.
function auth.lock_readers()
  local out = {}
  for _, service in ipairs { "morf-lock-finger", "morf-lock-face", "morf-lock-reader" } do
    if morf.fs.exists("/etc/pam.d/" .. service) then out[#out + 1] = service end
  end
  return out
end

function auth.lock_services()
  local password = "login"
  for _, service in ipairs { "morf-lock", "system-auth" } do
    if morf.fs.exists("/etc/pam.d/" .. service) then
      password = service
      break
    end
  end
  local reader = morf.fs.exists("/etc/pam.d/morf-lock-reader") and "morf-lock-reader" or nil
  return password, reader
end

-- ------------------------------------------------------------------ lock --

local Lock = {}
Lock.__index = Lock

--- A lock's door: PAM, for `options.user`. The reader is listened to from
--- the start unless `options.listen` is false; then `door:listen()` and
--- `door:stop()` say when (a lock listens while its way in is open, and a
--- camera is not kept on while the clock is looked at).
function auth.lock(options)
  local password_service = auth.lock_services()
  local readers = options.reader == false and {} or (options.readers or auth.lock_readers())
  local door = setmetatable({
    user = options.user,
    password_service = options.service or password_service,
    readers = readers,
    listening = {}, wanted = {},
    on = handlers(options),
    working = false,
    done = false,
  }, Lock)
  if options.listen ~= false then door:listen() end
  return door
end

function Lock:submit(password)
  if self.working or self.done or not self.user then return end
  self.working = true
  self.on.busy(true)
  morf.pam.authenticate(self.password_service, self.user, password or "", function(ok, why)
    self.working = false
    self.on.busy(false)
    if self.done then return end
    if ok then
      self:finish()
    else
      self.on.failed(why or "Wrong password")
    end
  end)
end

-- The readers' stacks, each heard for as long as it is wanted and the door
-- is shut, all at once: a finger and a face race, and the first yes opens
-- the door. A no starts that reader listening again, since a reader that
-- gave up is not a reader that refused -- after a moment, or after a while
-- when it gave up at once (a reader it cannot reach, a camera in use).
--
-- `door:listen(service)` and `door:stop(service)`; without one, all of them.
local function wants(self, service, fn)
  for _, name in ipairs(self.readers) do
    if service == nil or service == name then fn(name) end
  end
end

function Lock:listen(service)
  wants(self, service, function(name)
    self.wanted[name] = true
    self:hear(name)
  end)
end

function Lock:hear(name)
  if self.listening[name] or self.done or not self.user or not self.wanted[name] then return end
  local ok, session = pcall(morf.pam.session, name, self.user)
  if not ok or not session then return end
  local started = morf.time.now_ms()
  self.listening[name] = session
  session:on_message(function(m)
    if self.listening[name] ~= session then return end
    if m.kind == "info" or m.kind == "error" then
      self.on.info(m.text, m.kind == "error")
    elseif m.kind == "prompt" then
      -- It fell through to asking for a password: that has its own stack.
      session:cancel()
    elseif m.kind == "finished" then
      self.listening[name] = nil
      if m.ok then
        self:finish()
      elseif not self.done and self.wanted[name] then
        local quick = (morf.time.now_ms() - started) < 1000
        morf.timer(quick and 10000 or 1500, function()
          if self.wanted[name] then self:hear(name) end
        end, false)
      end
    end
  end)
end

function Lock:finish()
  if self.done then return end
  self.done = true
  self:stop()
  self.on.open()
end

function Lock:stop(service)
  wants(self, service, function(name)
    self.wanted[name] = false
    local listening = self.listening[name]
    self.listening[name] = nil
    if listening then pcall(function() listening:cancel() end) end
  end)
end

-- --------------------------------------------------------------- greeter --

local Greeter = {}
Greeter.__index = Greeter

--- A greeter's door: greetd, for `options.user`; `options.session` (from
--- lib.sessions) is what a yes starts. Nested, with no greetd to ask,
--- `door.available` is false and a submit says so.
function auth.greeter(options)
  local door = setmetatable({
    user = options.user,
    session = options.session,
    on = handlers(options),
    available = options.enabled ~= false and (morf.env("GREETD_SOCK") or "") ~= "",
    working = false,
  }, Greeter)
  return door
end

-- Start only for an explicit submission. Opening a greeter, selecting an
-- account or receiving an error must never consume another PAM attempt.
function Greeter:begin()
  if self.login or not self.user or not self.available then return end
  local ok, login = pcall(morf.greetd.converse, self.user)
  if not ok or not login then
    self.available = false
    return
  end
  self.login, self.wanting, self.held, self.started = login, false, nil, false
  login:on_message(function(m)
    if self.login ~= login then return end
    if m.kind == "auth" then
      if m.auth_type == "secret" or m.auth_type == "visible" then
        if self.held then
          local answer = self.held
          self.held = nil
          login:respond(answer)
        else
          self.wanting = true
        end
      else
        self.on.info(m.text, m.auth_type == "error")
        login:respond(nil)
      end
    elseif m.kind == "success" then
      if self.started then
        self.on.open()
        return
      end
      local session = self.session
      if not session then
        self:fail("No session to start")
        return
      end
      self.started = true
      login:start(session.command, session.environment)
    elseif m.kind == "error" or m.kind == "failed" then
      self:fail(m.text ~= "" and m.text or "Authentication connection failed")
    end
  end)
end

function Greeter:fail(why)
  self:stop()
  self.on.failed(why)
end

function Greeter:submit(password)
  if self.working or not self.user or not password or password == "" then return end
  if not self.available then
    self.on.failed("No greetd to ask (not started by greetd)")
    return
  end
  if not self.login then self:begin() end
  if not self.login then return end
  self.working = true
  self.on.busy(true)
  if self.wanting then
    self.wanting = false
    self.login:respond(password or "")
  else
    self.held = password or ""
  end
end

--- Another account: cancel the old conversation; wait for a submission.
function Greeter:switch(user)
  if user == self.user then return end
  self:stop()
  self.user = user
end

function Greeter:stop()
  local login = self.login
  -- Detach first: late callbacks from cancellation cannot start a session.
  self.login, self.wanting, self.held, self.started = nil, false, nil, false
  self.working = false
  if login then pcall(function() login:cancel() end) end
  self.on.busy(false)
end

-- ------------------------------------------------------------------ power --

--- Asks logind to "Suspend", "Reboot" or "PowerOff". Not interactive: a
--- login screen has nobody to answer a polkit prompt. True, or nil and why.
function auth.power(method)
  local io = require("morf.io")
  local ok, manager = pcall(io.dbus.proxy, "system", "org.freedesktop.login1",
    "/org/freedesktop/login1", "org.freedesktop.login1.Manager")
  if not (ok and manager) then return nil, "cannot reach logind" end
  local called, err = pcall(manager.call_with, manager, method, false)
  if not called then return nil, tostring(err) end
  return true
end

return auth
