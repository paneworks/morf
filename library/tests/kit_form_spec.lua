-- lib.kit.form on the Form archetype, drawn by the default kit: a sign-in
-- of two real kit fields. Return on an invalid form sends the keyboard to
-- the first field that is wrong and shows why; a message waits until its
-- field was left; fixing it hides it; Return on a valid form sends the
-- values once, however often it is pressed while sending; a failed send
-- shows in the summary; a reset forgets the try.
--
--     morf test --no-dbus library/tests/kit_form_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  require("lib.kit.skins.default").make { variant = "light" }
  local w = require("lib.kit.widgets")
  local sent, finish = {}, nil
  local node, form = w.login_form { id = "login", width = 320,
    on_submit = function(values, done)
      sent[#sent + 1] = "user=" .. tostring(values.user) .. ",password=" .. tostring(values.password)
      finish = done
    end }
  form.field("user", { label = "Username", message = "Enter your username",
    control = { w.entry { id = "user", label = "Username", width = 320, height = 56, inset = { 16, 24, 16, 6 },
      required = true } } })
  form.field("password", { label = "Password", message = "Eight characters or more",
    valid = function(v) return #(v or "") >= 8 end,
    control = { w.password { id = "password", label = "Password", width = 320, height = 56, text = "hunter22",
      inset = { 16, 24, 44, 6 } } } })
  ui.Item { width = 600, height = 500, ui.Item { x = 40, y = 20, width = 320, height = 460, node } }
  morf.ipc.error = function(name) return form.error(name)() end
  morf.ipc.focused = function() local f = morf.focus.get() return f and f.id or "" end
  morf.ipc.sent = function() return table.concat(sent, ";") end
  morf.ipc.state = function(field) return form.t[field] end
  morf.ipc.fail = function(message) finish(false, message) end
  morf.ipc.reset = function() form.reset() end
]]

local function load() test.load { source = HOST, size = { 600, 500 } } test.settle(300) end
local function height(id) return test.get(id).height end

test.it("Return on an invalid form focuses the first invalid field and shows its message", function()
  load()
  test.click("password") test.settle(30)
  test.eq(test.ipc("focused"), "password")
  test.key("Return") test.settle(400)
  test.eq(test.ipc("focused"), "user", "the keyboard went to the field that is wrong")
  test.eq(test.ipc("error", "user"), "Enter your username")
  test.truthy(height("login-user-message") > 20, "its message opened under it")
  test.truthy(height("login-summary") > 40, "the summary slid down")
  test.eq(test.ipc("sent"), "", "nothing was sent")
  -- The summary's row for it sends the keyboard back to it.
  test.click("password") test.settle(30)
  test.click("login-summary-user") test.settle(30)
  test.eq(test.ipc("focused"), "user", "its row in the summary leads to it")
end)

test.it("a message waits until its field was left, and fixing it hides it", function()
  load()
  test.click("user") test.settle(30)
  test.eq(test.ipc("error", "user"), "", "not shown while it was never left")
  test.click("password") test.settle(400)
  test.eq(test.ipc("error", "user"), "Enter your username", "shown once it was left")
  test.truthy(height("login-user-message") > 20)
  test.click("user") test.settle(30)
  test.type("ana") test.settle(400)
  test.eq(test.ipc("error", "user"), "", "fixed, it hides")
  test.truthy(height("login-user-message") < 1, "and its message closed")
end)

test.it("Return submits a valid form with its values, once while it sends", function()
  load()
  test.click("user") test.settle(30)
  test.type("ana") test.settle(30)
  test.key("Return") test.settle(100)
  test.eq(test.ipc("sent"), "user=ana,password=hunter22")
  test.eq(test.ipc("state", "submitting"), true)
  test.key("Return") test.settle(100)
  test.eq(test.ipc("sent"), "user=ana,password=hunter22", "a second Return while sending does nothing")
end)

test.it("a failed send shows in the summary, and a reset forgets the try", function()
  load()
  test.click("user") test.settle(30)
  test.type("ana") test.settle(30)
  test.key("Return") test.settle(100)
  test.ipc("fail", "Wrong password") test.settle(400)
  test.eq(test.ipc("state", "submitting"), false)
  test.eq(test.get("login-summary-head").text, "Wrong password")
  test.truthy(height("login-summary") > 40, "the summary shows it")
  test.eq(test.ipc("state", "tried"), true)
  test.ipc("reset") test.settle(400)
  test.eq(test.ipc("state", "tried"), false, "the reset forgot the try")
  test.truthy(height("login-summary") < 1, "and the summary closed")
  test.eq(test.ipc("error", "user"), "")
end)
