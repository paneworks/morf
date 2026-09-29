-- The VPNs a machine runs beside NetworkManager's own, each through its
-- own command -- what it says, and up or down where its command lets a
-- user say so. Two kinds:
--
--   mesh     a private network of one's own machines: NetBird, Tailscale,
--            ZeroTier
--   tunnel   the way out to the internet through somewhere else: Mullvad,
--            Proton VPN (its command-line client; its app's connections
--            are NetworkManager's)
--
--   local vpns = require("lib.vpns")
--   vpns.watch("mesh")                   -- read now and every so often ...
--   vpns.rows.mesh:get()                 -- ... into this signal
--   vpns.loaded.mesh:get()               -- true after the first completed scan
--   vpns.release("mesh")
--   vpns.set("netbird", true, function(ok, why) end)
--   vpns.links("tunnel")                 -- up by their links: cheap, no command
--
-- A row: `{ id, name, kind, installed, up, address, detail, can_toggle }`.
-- ZeroTier's own command wants root to list its networks, so without that
-- it is told by its interface (a `zt*` link) and is not switched from here.

local morf = require("morf")

local vpns = {}

local TOOLS = {
  { id = "netbird", name = "NetBird", command = "netbird", kind = "mesh" },
  { id = "tailscale", name = "Tailscale", command = "tailscale", kind = "mesh" },
  { id = "zerotier", name = "ZeroTier", command = "zerotier-cli", kind = "mesh" },
  { id = "mullvad", name = "Mullvad", command = "mullvad", kind = "tunnel" },
  { id = "protonvpn", name = "Proton VPN", command = "protonvpn-cli", kind = "tunnel" },
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

--- The VPNs of `kind` ("mesh" or "tunnel") that are up, by their links
--- alone -- no command run: cheap enough for a status icon to ask every
--- time. `{ "NetBird", ... }`.
function vpns.links(kind)
  local out = {}
  if kind ~= "tunnel" then
    if link_up("wt") or link_up("netbird") then out[#out + 1] = "NetBird" end
    if link_up("tailscale") then out[#out + 1] = "Tailscale" end
    if link_up("zt") or link_up("zerotier") then out[#out + 1] = "ZeroTier" end
  end
  if kind ~= "mesh" then
    if link_up("wg0-mullvad") or link_up("wg-mullvad") then out[#out + 1] = "Mullvad" end
    if link_up("proton") or link_up("pvpn") then out[#out + 1] = "Proton VPN" end
  end
  return out
end

--- Whether a NetworkManager connection is a mesh VPN's own tunnel (the
--- mesh daemons' interfaces show up there too).
function vpns.is_mesh_link(name)
  name = tostring(name or "")
  return name:match("^netbird") ~= nil or name:match("^wt%d") ~= nil
    or name:match("^tailscale") ~= nil or name:match("^zt") ~= nil or name:match("^zerotier") ~= nil
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

function READ.mullvad(done)
  run({ "mullvad", "status", "--json" }, function(r)
    local ok, data = pcall(morf.json.decode, tostring(r.stdout or ""))
    if not ok or type(data) ~= "table" then done { up = false, detail = "Not running" } return end
    local st = tostring(data.state or "")
    local loc = type(data.details) == "table" and data.details.location or {}
    local where = table.concat({ loc.city or "", loc.country or "" }, ", "):gsub("^, ", ""):gsub(", $", "")
    local words = { connected = "Connected", connecting = "Connecting", disconnected = "Off", disconnecting = "Disconnecting", error = "Error" }
    done { up = st == "connected", address = st == "connected" and (loc.ipv4 or "") or "",
      detail = (words[st] or st) .. (st == "connected" and where ~= "" and (" · " .. where) or "") }
  end)
end

function READ.protonvpn(done)
  run({ "protonvpn-cli", "status" }, function(r)
    local out = tostring(r.stdout or "")
    local server = out:match("Server:%s*([^\n]+)")
    local up = server ~= nil or out:match("Connected") ~= nil and not out:match("No active")
    done { up = up == true, address = out:match("IP:%s*([%d%.]+)") or "",
      detail = up and ("Connected" .. (server and (" · " .. server) or "")) or "Off" }
  end)
end

local UP = {
  mullvad = { { "mullvad", "connect" }, { "mullvad", "disconnect" } },
  protonvpn = { { "protonvpn-cli", "connect", "--fastest" }, { "protonvpn-cli", "disconnect" } },
  netbird = { { "netbird", "up" }, { "netbird", "down" } },
  tailscale = { { "tailscale", "up" }, { "tailscale", "down" } },
}

--- Reads every installed tool of `kind` now; `done(rows)`.
function vpns.read(kind, done)
  local rows, pending = {}, 0
  local list = {}
  for _, t in ipairs(TOOLS) do
    if (kind == nil or t.kind == kind) and installed(t.command) then list[#list + 1] = t end
  end
  if #list == 0 then done(rows) return end
  pending = #list
  for index, t in ipairs(list) do
    READ[t.id](function(info)
      rows[index] = {
        id = t.id, name = t.name, kind = t.kind, installed = true,
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
vpns.rows = {
  mesh = morf.signal("lib.vpns.mesh", {}),
  tunnel = morf.signal("lib.vpns.tunnel", {}),
}
-- Distinguish an empty completed scan from a scan that has not returned yet.
vpns.loaded = {
  mesh = morf.signal("lib.vpns.mesh.loaded", false),
  tunnel = morf.signal("lib.vpns.tunnel.loaded", false),
}
local timers, readers = {}, { mesh = 0, tunnel = 0 }

local function refresh(kind)
  vpns.read(kind, function(rows)
    vpns.rows[kind]:set(rows)
    vpns.loaded[kind]:set(true)
  end)
end

--- Reads `kind` now and every `interval_ms` (10 s) while anything holds
--- it -- `vpns.release(kind)` when done.
function vpns.watch(kind, interval_ms)
  readers[kind] = readers[kind] + 1
  if not timers[kind] then
    refresh(kind)
    timers[kind] = morf.timer(interval_ms or 10000, function() refresh(kind) end, true)
  end
  return vpns.rows[kind]
end

function vpns.release(kind)
  readers[kind] = math.max(0, readers[kind] - 1)
  if readers[kind] == 0 and timers[kind] then
    timers[kind]:cancel()
    timers[kind] = nil
  end
end

--- Brings `id` up or down; `done(ok, why)`, and the rows are read again.
function vpns.set(id, up, done)
  done = done or function() end
  local argv = UP[id] and UP[id][up and 1 or 2]
  if not argv then done(nil, "not switched from here") return end
  run(argv, function(r)
    done(r.ok and true or nil, r.stderr)
    for _, t in ipairs(TOOLS) do
      if t.id == id then refresh(t.kind) end
    end
  end)
end

return vpns
