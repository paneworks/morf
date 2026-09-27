-- The greeter at rest and with its sheet up. Nested there is no greetd, so
-- nothing here reaches PAM.

local test = morf.test
test.describe("greet", function()
  test.it("rest and sheet", function()
    test.load("../greet/init.lua", { size = { 1920, 1080 } })
    test.settle(1500)
    test.key("Return")
    test.settle(1200)
    test.eq(test.ipc("stage"), "sheet")
    test.type("abc")
    test.settle(500)
  end)
end)
