-- The mesh VPNs a machine runs beside NetworkManager's own: NetBird,
-- Tailscale and ZeroTier, each through its own command -- what it says,
-- and up or down where its command lets a user say so.
--
--   local vpns = require("lib.vpns")
--   local mesh = vpns.watch()            -- a signal of rows, read while it is read
--   for _, v in ipairs(mesh:get()) do print(v.name, v.up, v.address, v.detail) end
--   vpns.set("netbird", true, function(ok, why) end)
--
-- A row: `{ id, name, installed, up, address, detail, can_toggle }`.
-- ZeroTier's own command wants root to list its networks, so without that
-- it is told by its interface (a `zt*` link) and is not switched from here.

local morf = require("morf")

local vpns = {}

local TOOLS = {
  { id = "netbird", name = "NetBird", command = "netbird" },
  { id = "tailscale", name = "Tailscale", command = "tailscale" },
  { id = "zerotier", name = "ZeroTier", command = "zerotier-cli" },
}

local function run(argv, done)
  local ok = pcall(morf.run, argv, { timeout_ms = 8000, max_output = 256 * 1024 }, function(result)
    done(result or {})
  end)
  if not ok then done({ ok = false }) end
end

local function installed(command)
  for dir in (morf.env("PATH") or "/usr/bin"):gmatch("[^:]+") do
    if morf.fs.exists(dir .. "/" .. command) then return true end
  end
  return morf.fs.exists(morf.fs.home() .. "/.local/bin/" .. command)
end

-- Links whose name starts with `prefix` and are up.
local function link_up(prefix)
  local ok, entries = pcall(morf.fs.list, "/sys/class/net")
  for _, e in ipairs(ok and entries or {}) do
    if e.name:sub(1, #prefix) == prefix then
      local okr, state = pcall(morf.fs.read, "/sys/class/net/" .. e.name .. "/operstate")
      local s = okr and type(state) == "string" and state:match("%a+") or ""
      if s == "up" or s == "unknown" then return e.name end
    end
  end
  return nil
end

--- The mesh VPNs that are up, by their links alone -- no command run:
--- cheap enough for a status icon to ask every time. `{ "NetBird", ... }`.
function vpns.links()
  local out = {}
  if link_up("wt") or link_up("netbird") then out[#out + 1] = "NetBird" end
  if link_up("tailscale") then out[#out + 1] = "Tailscale" end
  if link_up("zt") or link_up("zerotier") then out[#out + 1] = "ZeroTier" end
  return out
end

local READ = {}

function READ.netbird(done)
  run({ "netbird", "status" }, function(r)
    local out = tostring(r.stdout or "")
    local management = out:match("Management:%s*(%a+)") or ""
    local address = out:match("NetBird IP:%s*([%d%.]+)") or ""
    local peers = out:match("Peers count:%s*([%d/]+)") or ""
    local up = management == "Connected"
    done { up = up, address = address,
      detail = up and (peers ~= "" and ("%s peers"):format(peers) or "Connected") or (out:match("Daemon status:%s*(%a+)") or "Down") }
  end)
end

function READ.tailscale(done)
  run({ "tailscale", "status", "--json" }, function(r)
    local ok, data = pcall(morf.json.decode, tostring(r.stdout or ""))
    if not ok or type(data) ~= "table" then done { up = false, detail = "Not running" } return end
    local up = data.BackendState == "Running"
    local peers, online = 0, 0
    for _, p in pairs(type(data.Peer) == "table" and data.Peer or {}) do
      peers = peers + 1
      if p.Online then online = online + 1 end
    end
    local ips = type(data.TailscaleIPs) == "table" and data.TailscaleIPs or {}
    done { up = up, address = ips[1] or "",
      detail = up and ("%d/%d peers online"):format(online, peers) or tostring(data.BackendState or "Stopped") }
  end)
end

function READ.zerotier(done)
  local link = link_up("zt") or link_up("zerotier")
  done { up = link ~= nil, address = "", detail = link and ("Up on " .. link) or "Down", can_toggle = false }
end

local UP = {
  netbird = { { "netbird", "up" }, { "netbird", "down" } },
  tailscale = { { "tailscale", "up" }, { "tailscale", "down" } },
}

--- Reads every installed tool now; `done(rows)`.
function vpns.read(done)
  local rows, pending = {}, 0
  local list = {}
  for _, t in ipairs(TOOLS) do
    if installed(t.command) then list[#list + 1] = t end
  end
  if #list == 0 then done(rows) return end
  pending = #list
  for index, t in ipairs(list) do
    READ[t.id](function(info)
      rows[index] = {
        id = t.id, name = t.name, installed = true,
        up = info.up == true, address = info.address or "", detail = info.detail or "",
        can_toggle = info.can_toggle ~= false and UP[t.id] ~= nil,
      }
      pending = pending - 1
      if pending == 0 then done(rows) end
    end)
  end
end

-- Made as the module loads: a signal made inside a binding or an effect is
-- that binding's, and goes with it.
local signal = morf.signal("lib.vpns.rows", {})
local timer, readers = nil, 0

--- The rows, as a signal (what `watch` keeps current).
vpns.rows = signal

--- A signal of the rows, read again every `interval_ms` (10 s) while
--- anything holds it -- `vpns.release()` when done.
function vpns.watch(interval_ms)
  readers = readers + 1
  if not timer then
    local function again() vpns.read(function(rows) signal:set(rows) end) end
    again()
    timer = morf.timer(interval_ms or 10000, again, true)
  end
  return signal
end

function vpns.release()
  readers = math.max(0, readers - 1)
  if readers == 0 and timer then
    timer:cancel()
    timer = nil
  end
end

--- Brings `id` up or down; `done(ok, why)`, and the rows are read again.
function vpns.set(id, up, done)
  done = done or function() end
  local argv = UP[id] and UP[id][up and 1 or 2]
  if not argv then done(nil, "not switched from here") return end
  run(argv, function(r)
    done(r.ok and true or nil, r.stderr)
    if signal then vpns.read(function(rows) signal:set(rows) end) end
  end)
end

return vpns
