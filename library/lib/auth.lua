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
  local password_service, reader_service = auth.lock_services()
  local door = setmetatable({
    user = options.user,
    password_service = options.service or password_service,
    reader_service = options.reader == false and nil or (options.reader or reader_service),
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

-- The reader's stack, heard for as long as the door is shut. Its yes opens
-- the door; a no starts it listening again, since a reader that gave up is
-- not a reader that refused.
function Lock:listen()
  self.wanted = true
  if not self.reader_service or self.listening or self.done or not self.user then return end
  local ok, session = pcall(morf.pam.session, self.reader_service, self.user)
  if not ok or not session then return end
  self.listening = session
  session:on_message(function(m)
    if self.listening ~= session then return end
    if m.kind == "info" or m.kind == "error" then
      self.on.info(m.text, m.kind == "error")
    elseif m.kind == "prompt" then
      -- It fell through to asking for a password: that has its own stack.
      session:cancel()
    elseif m.kind == "finished" then
      self.listening = nil
      if m.ok then
        self:finish()
      elseif not self.done and self.wanted then
        morf.timer(1500, function() if self.wanted then self:listen() end end, false)
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

function Lock:stop()
  self.wanted = false
  local listening = self.listening
  self.listening = nil
  if listening then pcall(function() listening:cancel() end) end
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
    available = (morf.env("GREETD_SOCK") or "") ~= "",
    working = false,
  }, Greeter)
  door:begin()
  return door
end

-- The conversation is opened as soon as there is an account, so whatever
-- its stack wants first (a finger) is asked for before a password is typed.
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
    elseif m.kind == "error" then
      self:fail(m.text ~= "" and m.text or "Wrong password")
    end
  end)
end

function Greeter:fail(why)
  if self.login then pcall(function() self.login:cancel() end) end
  self.login = nil
  self.working = false
  self.on.busy(false)
  self.on.failed(why)
  -- The next try, and the reader, start listening again.
  self:begin()
end

function Greeter:submit(password)
  if self.working or not self.user then return end
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

--- Another account: the conversation starts over for it.
function Greeter:switch(user)
  if user == self.user then return end
  self:stop()
  self.user = user
  self.working = false
  self:begin()
end

function Greeter:stop()
  if self.login then pcall(function() self.login:cancel() end) end
  self.login, self.wanting, self.held = nil, false, nil
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
