-- Mobile broadband details and actions through ModemManager's and
-- NetworkManager's command lines: `mmcli -J` for the modem, its SIM, bearers
-- and messages, `nmcli` for saved profiles. Parsing is pure; `run` is the
-- only side effect, so tests stub the commands.
local morf = require("morf")
local M = {}

local function known(v) return v ~= nil and v ~= "" and v ~= "--" end
local function list(v) return type(v) == "table" and v or {} end

--- `argv` asynchronously; `done(ok, stdout, err)`.
function M.run(argv, done, timeout)
  done = done or function() end
  local ok, child = pcall(morf.run, argv, { timeout_ms = timeout or 30000, max_output = 262144 }, function(result)
    result = result or {}
    local err = (result.stderr or ""):gsub("^%s+", ""):gsub("%s+$", "")
    if err == "" then err = result.error or (result.timed_out and "Timed out") or ("exit " .. tostring(result.code)) end
    done(result.ok == true, result.stdout or "", err)
  end)
  if not ok or not child then done(false, "", tostring(child or "Could not start " .. argv[1])) end
end

--- A command's JSON reply, or nil.
function M.json(text)
  local ok, value = pcall(morf.json.decode, text or "")
  if ok and type(value) == "table" then return value end
end

-- ------------------------------------------------------------- modes --

local GENERATIONS = { "2g", "3g", "4g", "5g" }

--- "allowed: 2g, 3g, 4g; preferred: 4g" as { allowed = {...}, preferred = "4g" }.
function M.mode(text)
  local allowed, preferred = (text or ""):match("allowed:%s*([^;]*);%s*preferred:%s*(%S+)")
  if not allowed then return nil end
  local set = {}
  for g in allowed:gmatch("%d+g") do set[#set + 1] = g end
  return { allowed = set, preferred = preferred ~= "none" and preferred or nil }
end

local function same(a, b)
  if not a or not b or #a.allowed ~= #b.allowed or a.preferred ~= b.preferred then return false end
  for i, g in ipairs(a.allowed) do if b.allowed[i] ~= g then return false end end
  return true
end

-- The choices a person makes, best first; each needs the modem to list it.
local CHOICES = {
  { id = "5g", name = "5G", allowed = { "2g", "3g", "4g", "5g" }, preferred = "5g" },
  { id = "4g", name = "4G", allowed = { "2g", "3g", "4g" }, preferred = "4g" },
  { id = "4g-only", name = "4G only", allowed = { "4g" } },
  { id = "3g", name = "3G", allowed = { "2g", "3g" }, preferred = "3g" },
  { id = "2g", name = "2G only", allowed = { "2g" } },
}

--- The network-mode choices this modem supports, and the current one's id.
function M.modes(supported, current)
  local have, out = {}, {}
  for _, text in ipairs(list(supported)) do have[#have + 1] = M.mode(text) end
  local now = M.mode(current)
  local current_id
  for _, choice in ipairs(CHOICES) do
    for _, mode in ipairs(have) do
      if same(mode, choice) then
        out[#out + 1] = choice
        if same(now, choice) then current_id = choice.id end
        break
      end
    end
  end
  return out, current_id
end

--- `mmcli` arguments selecting a choice from `M.modes`.
function M.mode_args(choice)
  local args = { "--set-allowed-modes=" .. table.concat(choice.allowed, "|") }
  if choice.preferred then args[#args + 1] = "--set-preferred-mode=" .. choice.preferred end
  return args
end
M.GENERATIONS = GENERATIONS

-- ------------------------------------------------------------ modem --

local function retries(entries)
  local out = {}
  for _, text in ipairs(list(entries)) do
    local lock, count = text:match("^(%S+)%s*%((%d+)%)")
    if lock then out[lock] = tonumber(count) end
  end
  return out
end

--- What `mmcli -J -m any` says, flattened.
function M.modem(reply)
  local m = (reply or {}).modem
  if type(m) ~= "table" then return nil end
  local g, gpp = m.generic or {}, m["3gpp"] or {}
  local modes, mode = M.modes(g["supported-modes"], g["current-modes"])
  local lock = g["unlock-required"]
  return {
    path = m["dbus-path"],
    state = g.state or "",
    failed = known(g["state-failed-reason"]) and g["state-failed-reason"] or nil,
    model = table.concat({ known(g.manufacturer) and g.manufacturer or nil, known(g.model) and g.model ~= "0" and g.model or nil }, " "),
    sim = known(g.sim) and g.sim ~= "/" and g.sim or nil,
    own_numbers = list(g["own-numbers"]),
    -- PIN2 guards fixed dialling only; it never locks the SIM.
    locked = (lock == "sim-pin" or lock == "sim-puk") and lock or nil,
    retries = retries(g["unlock-retries"]),
    operator = known(gpp["operator-name"]) and gpp["operator-name"] or "",
    operator_code = known(gpp["operator-code"]) and gpp["operator-code"] or "",
    registration = gpp["registration-state"] or "",
    rejected = known(gpp["network-rejection-error"]) and gpp["network-rejection-error"] or nil,
    technology = list(g["access-technologies"])[1] or "",
    signal = tonumber((g["signal-quality"] or {}).value) or 0,
    bearers = list(g.bearers),
    modes = modes, mode = mode,
  }
end

--- What `mmcli -J -i <sim>` says.
function M.sim(reply)
  local p = ((reply or {}).sim or {}).properties
  if type(p) ~= "table" then return nil end
  local iccid = known(p.iccid) and p.iccid or ""
  return {
    operator_code = known(p["operator-code"]) and p["operator-code"] or "",
    operator = known(p["operator-name"]) and p["operator-name"] or "",
    iccid_tail = iccid ~= "" and iccid:sub(-4) or "",
    active = p.active == "yes",
  }
end

--- What `mmcli -J -b <bearer>` says.
function M.bearer(reply)
  local b = (reply or {}).bearer
  if type(b) ~= "table" then return nil end
  local p, s, st = b.properties or {}, b.status or {}, b.stats or {}
  local e = s["connection-error"]
  return {
    path = b["dbus-path"],
    apn = known(p.apn) and p.apn or "",
    user = known(p.user) and p.user or "",
    ip_type = p["ip-type"] or "",
    connected = s.connected == "yes",
    worked = (tonumber(st["total-duration"]) or 0) > 0 or (tonumber(st["total-bytes-rx"]) or 0) > 0,
    error = type(e) == "table" and { name = e.name or "", message = e.message or "" } or nil,
  }
end

-- -------------------------------------------------------- diagnosis --

--- A failed connection in plain words, and the fix that answers it:
--- { text, fix = "ipv4" | "ipv6" | "reset" | "apn" | "unlock" | nil, apn }.
function M.diagnose(modem, bearers)
  if not modem then return nil end
  if modem.locked then return { text = "The SIM is locked. Enter its PIN to use mobile data.", fix = "unlock" } end
  if modem.failed then return { text = "The modem reports: " .. modem.failed:gsub("%-", " ") .. "." } end
  if modem.state == "connected" then return nil end
  local last
  for _, b in ipairs(bearers or {}) do
    if b.error and not b.connected then last = b break end
  end
  if not last then
    if modem.rejected then return { text = "The network rejected registration: " .. modem.rejected:gsub("%-", " ") .. "." } end
    return nil
  end
  local m = (last.error.name .. " " .. last.error.message):lower()
  if m:find("ipv6%-call%-disallowed") or m:find("ipv4 only") then
    return { text = "This network allows IPv4 only (common when roaming).", fix = "ipv4" }
  end
  if m:find("ipv4%-call%-disallowed") or m:find("ipv6 only") then
    return { text = "This network allows IPv6 only.", fix = "ipv6" }
  end
  if m:find("throttl") then
    return { text = "Too many failed attempts; the network paused new ones.", fix = "reset" }
  end
  if m:find("unsubscribed") or m:find("unknown%-apn") or m:find("missing or unknown apn") or m:find("apn") then
    local worked
    for _, b in ipairs(bearers or {}) do
      if b.worked and b.apn ~= "" and b.apn ~= last.apn then worked = b.apn break end
    end
    local text = "The network rejected the APN" .. (last.apn ~= "" and (" " .. last.apn) or "") .. "."
    return { text = text, fix = "apn", apn = worked }
  end
  if m:find("roaming") or m:find("plmn%-not%-allowed") then
    return { text = "Your plan or the network does not allow roaming here." }
  end
  return { text = "Connection failed: " .. (last.error.message ~= "" and last.error.message or last.error.name) }
end

-- ------------------------------------------------------- operators --

--- `mmcli -J --3gpp-scan`'s networks: { code, name, technology, availability }.
function M.networks(reply)
  local out = {}
  for _, text in ipairs(list(((((reply or {}).modem or {})["3gpp"]) or {})["scan-networks"])) do
    local row = {}
    for key, value in text:gmatch("([%w%-]+):%s*([^,]+)") do row[key] = value:gsub("%s+$", "") end
    if row["operator-code"] then
      out[#out + 1] = { code = row["operator-code"], name = row["operator-name"] or row["operator-code"],
        technology = row["access-technologies"] or "", availability = row.availability or "" }
    end
  end
  return out
end

-- -------------------------------------------------------------- USSD --

--- The network's words in an `mmcli` USSD reply.
function M.ussd(text)
  return (text or ""):match("'(.*)'") or (text or ""):gsub("^%s+", ""):gsub("%s+$", "")
end

-- --------------------------------------------------------- messages --

--- SMS paths from `mmcli -J --messaging-list-sms`.
function M.sms_paths(reply)
  return list((reply or {})["modem.messaging.sms"])
end

--- One message from `mmcli -J -s <path>`.
function M.sms(reply)
  local s = (reply or {}).sms
  if type(s) ~= "table" then return nil end
  local c, p = s.content or {}, s.properties or {}
  return { path = s["dbus-path"], number = known(c.number) and c.number or "",
    text = known(c.text) and c.text or "", state = p.state or "", timestamp = known(p.timestamp) and p.timestamp or "" }
end

-- --------------------------------------------------------- profiles --

local IP = {
  ipv4 = { "ipv4.method", "auto", "ipv6.method", "disabled" },
  ipv6 = { "ipv4.method", "disabled", "ipv6.method", "auto" },
  ipv4v6 = { "ipv4.method", "auto", "ipv6.method", "auto" },
}

--- The IP type a profile asks for, from its ipv4/ipv6 methods.
function M.ip_type(row)
  local v4 = (row.ipv4_method or "auto") ~= "disabled"
  local v6 = (row.ipv6_method or "auto") ~= "disabled" and row.ipv6_method ~= "ignore"
  if v4 and v6 then return "ipv4v6" end
  return v4 and "ipv4" or "ipv6"
end

--- `nmcli connection modify` arguments for one profile change.
function M.profile_args(uuid, change)
  local args = { "nmcli", "connection", "modify", uuid }
  local function add(...) for _, v in ipairs { ... } do args[#args + 1] = v end end
  if change.apn ~= nil then
    if change.apn == "" then add("gsm.auto-config", "yes", "gsm.apn", "")
    else add("gsm.auto-config", "no", "gsm.apn", change.apn) end
  end
  if change.user ~= nil then add("gsm.username", change.user) end
  if change.password ~= nil then add("gsm.password", change.password) end
  if change.ip_type and IP[change.ip_type] then add(table.unpack(IP[change.ip_type])) end
  if change.roaming ~= nil then add("gsm.home-only", change.roaming and "no" or "yes") end
  return args
end

return M
