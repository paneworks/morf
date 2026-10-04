-- Tor, as a switch: the system's tor service started and stopped -- never
-- enabled, so it is off again after a reboot -- and whether it is up.
--
--   local tor = require("lib.integrations.tor")
--   local t = tor.new { unit = "tor.service", socks = 9050 }
--   t.up()                   -- its SOCKS port is listening (cheap: /proc)
--   t.start(done) / t.stop(done)
--   t.bootstrap(done)        -- done(percent, summary) from its journal, or nil
--
-- Starting a system service takes root: systemctl asks polkit, and the
-- desk's polkit agent asks the person. Whether it is running is told by its
-- SOCKS port listening, which needs no command and no permission.

local morf = require("morf")

local tor = {}

local function listening(port)
  local hex = (":%04X"):format(port)
  for _, file in ipairs { "/proc/net/tcp", "/proc/net/tcp6" } do
    local ok, text = pcall(morf.fs.read, file)
    if ok and type(text) == "string" then
      for line in text:gmatch("[^\n]+") do
        -- local_address is the second column; state 0A is LISTEN.
        local addr, state = line:match("^%s*%d+:%s+(%S+)%s+%S+%s+(%x%x)")
        if addr and state == "0A" and addr:sub(-5):upper() == hex then return true end
      end
    end
  end
  return false
end

function tor.new(options)
  options = options or {}
  local unit = options.unit or "tor.service"
  local port = options.socks or 9050
  local t = { unit = unit, socks = port }

  function t.up() return listening(port) end

  local function systemctl(verb, done)
    local ok = pcall(morf.run, { "systemctl", verb, unit }, { timeout_ms = 60000 }, function(r)
      if done then done(r and r.ok and true or nil, r and r.stderr) end
    end)
    if not ok and done then done(nil, "could not run systemctl") end
  end
  function t.start(done) systemctl("start", done) end
  function t.stop(done) systemctl("stop", done) end

  --- How far it has come: its journal's last "Bootstrapped N%" line.
  --- `done(percent, summary)`, or `done(nil)` when the journal will not say
  --- (reading a system unit's journal wants the systemd-journal group).
  function t.bootstrap(done)
    local ok = pcall(morf.run, { "journalctl", "-u", unit, "-n", "60", "--no-pager", "-o", "cat" },
      { timeout_ms = 5000 }, function(r)
        local last, summary
        for pct, what in tostring(r and r.stdout or ""):gmatch("Bootstrapped (%d+)%%[^:]*:%s*([^\n]*)") do
          last, summary = tonumber(pct), what
        end
        done(last, summary)
      end)
    if not ok then done(nil) end
  end

  return t
end

return tor
