-- The lock, in a window and in preview: it never asks PAM (a lock killed
-- mid-conversation would count as a failed login).

local test = morf.test
test.describe("lock", function()
  test.it("rest, a key opens the sheet, a wrong one says so", function()
    test.load("../lock/init.lua", { size = { 1920, 1080 }, args = { "window", "preview" } })
    test.settle(1500)
    test.eq(test.ipc("stage"), "rest")
    test.type("w")
    test.settle(300)
    test.eq(test.ipc("stage"), "sheet")
    test.type("rong")
    test.key("Return")
    test.settle(1500)
    local m = test.find { id = "lock-message" }
    if not (m and tostring(m.text):find("Wrong")) then test.fail("no wrong: " .. tostring(m and m.text)) end
  end)
end)
