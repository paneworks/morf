local test = morf.test
local HOST = [[
  local cli = require("mobile_cli")
  require("morf.ui").Item { width = 10, height = 10 }
  morf.ipc.modem = function(text) return cli.modem(cli.json(text)) end
  morf.ipc.sim = function(text) return cli.sim(cli.json(text)) end
  morf.ipc.diagnose = function(modem, bearers)
    local m = cli.modem(cli.json(modem))
    local list = {}
    for i, text in ipairs(bearers) do list[i] = cli.bearer(cli.json(text)) end
    return cli.diagnose(m, list)
  end
  morf.ipc.networks = function(text) return cli.networks(cli.json(text)) end
  morf.ipc.sms = function(text) return cli.sms(cli.json(text)) end
  morf.ipc.profile = function(uuid, change) return cli.profile_args(uuid, change) end
  morf.ipc.ip_type = function(row) return cli.ip_type(row) end
  morf.ipc.ussd = function(text) return cli.ussd(text) end
  morf.ipc.mode_args = function(id, supported)
    local modes = cli.modes(supported, nil)
    for _, choice in ipairs(modes) do if choice.id == id then return cli.mode_args(choice) end end
  end
]]

-- Trimmed from a Fairphone 6 roaming on vodafone.de with a Vodafone NL SIM.
local SUPPORTED = { "allowed: 2g; preferred: none", "allowed: 3g; preferred: none", "allowed: 4g; preferred: none",
  "allowed: 2g, 3g; preferred: 3g", "allowed: 2g, 3g, 4g; preferred: 4g", "allowed: 2g, 3g, 4g, 5g; preferred: 5g" }
local function modem(state, extra)
  local generic = { state = state, ["state-failed-reason"] = "--", ["unlock-required"] = "sim-pin2",
    ["unlock-retries"] = { "sim-pin (3)", "sim-puk (10)", "sim-pin2 (3)" }, ["access-technologies"] = { "lte" },
    ["current-modes"] = "allowed: 2g, 3g, 4g, 5g; preferred: 5g", ["supported-modes"] = SUPPORTED,
    sim = "/org/freedesktop/ModemManager1/SIM/1", ["own-numbers"] = {}, manufacturer = "QUALCOMM INCORPORATED",
    model = "0", ["signal-quality"] = { value = "96" },
    bearers = { "/org/freedesktop/ModemManager1/Bearer/3", "/org/freedesktop/ModemManager1/Bearer/1" } }
  for k, v in pairs(extra or {}) do generic[k] = v end
  return morf.json.encode { modem = { ["dbus-path"] = "/org/freedesktop/ModemManager1/Modem/0", generic = generic,
    ["3gpp"] = { ["operator-code"] = "26202", ["operator-name"] = "vodafone.de", ["registration-state"] = "roaming",
      ["network-rejection-error"] = "--" } } }
end
local function bearer(n, apn, ip, err, worked)
  return morf.json.encode { bearer = { ["dbus-path"] = "/org/freedesktop/ModemManager1/Bearer/" .. n,
    properties = { apn = apn, ["ip-type"] = ip, user = "--" },
    stats = { ["total-duration"] = worked and "23007" or "--" },
    status = { connected = "no", ["connection-error"] = err and { name = err[1], message = err[2] } or nil } } }
end
local WORKED = bearer(1, "live.vodafone.com", "ipv4v6", { "org.freedesktop.ModemManager1.Error.Core.Failed",
  "Unknown error: Call failed: cm error: emm-detached" }, true)

test.it("reads the modem, its locks and the network modes it supports", function()
  test.load("../shell/init.lua", { source = HOST, size = { 100, 100 } })
  local m = test.ipc("modem", modem("registered"))
  test.eq(m.operator, "vodafone.de") test.eq(m.registration, "roaming") test.eq(m.signal, 96)
  test.falsy(m.locked, "PIN2 only guards fixed dialling")
  test.eq(m.retries["sim-pin"], 3)
  test.eq(m.mode, "5g")
  local ids = {}
  for i, choice in ipairs(m.modes) do ids[i] = choice.id end
  test.eq(ids, { "5g", "4g", "4g-only", "3g", "2g" })
  test.eq(test.ipc("mode_args", "4g", SUPPORTED), { "--set-allowed-modes=2g|3g|4g", "--set-preferred-mode=4g" })
  test.eq(test.ipc("mode_args", "4g-only", SUPPORTED), { "--set-allowed-modes=4g" })
  test.truthy(test.ipc("modem", modem("locked", { ["unlock-required"] = "sim-pin" })).locked)
end)

test.it("explains each roaming failure seen at the airport and offers its fix", function()
  test.load("../shell/init.lua", { source = HOST, size = { 100, 100 } })
  local ipv6 = test.ipc("diagnose", modem("registered"), { bearer(3, "access.vodafone.de", "ipv4v6",
    { "org.freedesktop.ModemManager1.Error.Core.Failed", "pdn-ipv6-call-disallowed: IPv4 only allowed" }), WORKED })
  test.eq(ipv6.fix, "ipv4")
  local throttled = test.ipc("diagnose", modem("registered"), { bearer(3, "access.vodafone.de", "ipv4v6",
    { "org.freedesktop.ModemManager1.Error.Core.Throttled", "Throttled: pdn-ipv4-call-throttled" }), WORKED })
  test.eq(throttled.fix, "reset")
  local apn = test.ipc("diagnose", modem("registered"), { bearer(3, "access.vodafone.de", "ipv4",
    { "org.freedesktop.ModemManager1.Error.MobileEquipment.ServiceOptionNotSubscribed",
      "Requested service option not subscribed: option-unsubscribed" }), WORKED })
  test.eq(apn.fix, "apn")
  test.eq(apn.apn, "live.vodafone.com", "offers the APN that worked before")
  test.falsy(test.ipc("diagnose", modem("connected"), { WORKED }), "a connected modem has no problem")
  test.eq(test.ipc("diagnose", modem("locked", { ["unlock-required"] = "sim-pin" }), {}).fix, "unlock")
end)

test.it("reads the SIM, nearby networks, messages and USSD replies", function()
  test.load("../shell/init.lua", { source = HOST, size = { 100, 100 } })
  local sim = test.ipc("sim", morf.json.encode { sim = { properties = { ["operator-code"] = "20404",
    ["operator-name"] = "--", iccid = "8931440302055992842", active = "yes" } } })
  test.eq(sim.operator_code, "20404") test.eq(sim.operator, "") test.eq(sim.iccid_tail, "2842")
  local nets = test.ipc("networks", morf.json.encode { modem = { ["3gpp"] = { ["scan-networks"] = {
    "operator-code: 26202, operator-name: vodafone.de, access-technologies: lte, availability: current",
    "operator-code: 26201, operator-name: Telekom.de, access-technologies: lte, availability: forbidden" } } } })
  test.eq(#nets, 2) test.eq(nets[1].code, "26202") test.eq(nets[2].availability, "forbidden")
  local sms = test.ipc("sms", morf.json.encode { sms = { ["dbus-path"] = "/org/freedesktop/ModemManager1/SMS/0",
    content = { number = "+31600000000", text = "Welcome to Germany" }, properties = { state = "received",
      timestamp = "2026-10-10T08:55:00+02:00" } } })
  test.eq(sms.text, "Welcome to Germany") test.eq(sms.path, "/org/freedesktop/ModemManager1/SMS/0")
  test.eq(test.ipc("ussd", "USSD session initiated; new reply from network: 'Your balance is 12.34'"),
    "Your balance is 12.34")
end)

test.it("turns profile changes into nmcli arguments", function()
  test.load("../shell/init.lua", { source = HOST, size = { 100, 100 } })
  test.eq(test.ipc("profile", "uuid-1", { apn = "live.vodafone.com", ip_type = "ipv4" }),
    { "nmcli", "connection", "modify", "uuid-1", "gsm.auto-config", "no", "gsm.apn", "live.vodafone.com",
      "ipv4.method", "auto", "ipv6.method", "disabled" })
  test.eq(test.ipc("profile", "uuid-1", { apn = "", roaming = false }),
    { "nmcli", "connection", "modify", "uuid-1", "gsm.auto-config", "yes", "gsm.apn", "", "gsm.home-only", "yes" })
  test.eq(test.ipc("ip_type", { ipv4_method = "auto", ipv6_method = "disabled" }), "ipv4")
  test.eq(test.ipc("ip_type", { ipv4_method = "auto", ipv6_method = "auto" }), "ipv4v6")
end)
