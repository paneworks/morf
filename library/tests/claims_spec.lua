-- ui.claims: a presence is a signal a binding paints with, eased as its
-- claim's property is, read-only; an asked claim comes and goes by hand.
--
--     morf test --no-dbus library/tests/claims_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local opened = morf.signal("spec.opened", false)
  local panel = ui.Rect { id = "panel", width = 100, height = 40, color = "#335577" }
  local claims = ui.claims(panel, {
    motion = { duration = 160, easing = "out_cubic" },
    open = { when = opened, set = { x = 100 } },
    peek = { class = "asked", set = { x = 20 } },
    rest = { set = { x = 0 } },
  })
  ui.Item { width = 300, height = 100,
    panel,
    ui.Rect { id = "glow", y = 50, width = 100, height = 40, color = "#ffffff",
      opacity = function() return claims.open.presence:get() end },
  }
  morf.ipc.open = function() opened:set(true) end
  morf.ipc.close = function() opened:set(false) end
  morf.ipc.peek = function() claims.peek.ask() end
  morf.ipc.unpeek = function() claims.peek.release() end
  morf.ipc.presence = function() return claims.open.presence:get() end
  morf.ipc.overwrite = function()
    local ok, error = pcall(function() claims.open.presence:set(1) end)
    return ok, tostring(error)
  end
]]

local function load()
  test.load { source = HOST, size = { 300, 100 } }
  test.settle(100)
end

test.it("a binding on a presence follows it frame by frame", function()
  load()
  test.eq(test.get({ id = "glow" }).opacity, 0)
  test.ipc("open")
  test.advance(48)
  local presence = test.ipc("presence")
  local glow = test.get({ id = "glow" }).opacity
  test.near(glow, presence, 1e-9)
  -- Out-cubic: 48 ms of 160 is already two thirds of the way.
  test.truthy(presence > 0.6 and presence < 0.7, "eased: " .. presence)
  test.near(test.get({ id = "panel" }).x, 100 * presence, 0.5)
  test.settle()
  test.eq(test.get({ id = "glow" }).opacity, 1)
  test.ipc("close")
  test.settle()
  test.eq(test.get({ id = "glow" }).opacity, 0)
  test.eq(test.logs("warn"), {})
end)

test.it("a presence is read-only", function()
  load()
  local ok, error = test.ipc("overwrite")
  test.eq(ok, false)
  test.truthy(error:find("read-only", 1, true), error)
  test.eq(test.ipc("presence"), 0)
end)

test.it("an asked claim holds while asked and lets go when released", function()
  load()
  test.ipc("peek")
  test.settle()
  test.near(test.get({ id = "panel" }).x, 20, 1e-6)
  test.ipc("unpeek")
  test.settle()
  test.near(test.get({ id = "panel" }).x, 0, 1e-6)
end)
