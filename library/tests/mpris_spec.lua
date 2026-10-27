-- Characterization of lib.services.mpris against a fake client
-- (PLAN item 7): the behaviour that keeps it hand-written -- a
-- dynamic set of bus names, an active-player choice, and a position
-- clock are not the registry's fixed-service pattern.
--
--     morf test library/tests/mpris_spec.lua

local test = morf.test

local HOST = [[
  local db = require("lib.services.dbus")
  local PREFIX = "org.mpris.MediaPlayer2."
  local A, B = PREFIX .. "alpha", PREFIX .. "beta"
  local now = 100000
  local props = {
    [A] = { PlaybackStatus = "Paused", Position = 5000000, Rate = 1,
            Metadata = { ["mpris:trackid"] = "/track/1", ["mpris:length"] = 200000000,
                         ["xesam:title"] = "Alpha Song" } },
    [B] = { PlaybackStatus = "Playing", Position = 10000000, Rate = 1,
            Metadata = { ["mpris:trackid"] = "/track/2", ["mpris:length"] = 100000000,
                         ["xesam:title"] = "Beta Song" } },
  }
  local roots = { [A] = { Identity = "Alpha" }, [B] = { Identity = "Beta" } }
  local calls, owner_fn = {}, nil
  local client = {}
  client.list_names = function() return { A, B, "org.freedesktop.DBus" } end
  client.on_owner_changed = function(fn) owner_fn = fn end
  client.get_all = function(name, _, interface)
    if interface == "org.mpris.MediaPlayer2.Player" then return props[name] end
    return roots[name]
  end
  client.on_properties = function() return { close = function() end } end
  client.on_signal = function() return { close = function() end } end
  client.call_async = function(name, _, _, method, _, _, done)
    calls[#calls + 1] = name .. ":" .. method
    if done then done() end
    return true
  end
  client.set_async = function() return true end
  client.forget = function() end
  db.new = function() return client end

  local mpris = require("lib.services.mpris")
  local media = mpris.connect({ clock = function() return now end, debounce_ms = 1 })
  morf.ipc.read = function()
    local listed = 0
    for _ in pairs(media.players()) do listed = listed + 1 end
    return {
      available = media.state.available, count = media.state.count,
      listed = listed,
      active = media.active() or "", title = media.state.active.title,
      position = media.position(),
    }
  end
  morf.ipc.play_pause = function() media.play_pause() end
  morf.ipc.calls = function() return table.concat(calls, ",") end
  morf.ipc.advance = function(ms) now = now + ms end
  morf.ipc.leave = function()
    -- A released name stops answering, as a real one does.
    props[B] = nil
    owner_fn(B, B, "")
  end
]]

test.describe("mpris (characterization)", function()
  test.it("chooses the playing player, routes controls, and keeps time", function()
    test.load("mpris-fixture.lua", { source = HOST, size = { 20, 20 } })
    test.settle()
    local read = test.ipc("read")
    test.eq(read.available, true)
    test.eq(read.count, 2)
    test.eq(read.title, "Beta Song")
    test.eq(read.position, 10)

    test.ipc("advance", 5000)
    test.eq(test.ipc("read").position, 15)

    test.ipc("play_pause")
    test.eq(test.ipc("calls"), "org.mpris.MediaPlayer2.beta:PlayPause")

    test.ipc("leave")
    test.settle()
    local after = test.ipc("read")
    test.eq(after.listed, 1)
    test.eq(after.count, 1)
    test.eq(after.title, "Alpha Song")
  end)
end)
