-- impasto's terminal windows: one per program, destroyed when it is done.
--
--     morf test --private-bus examples/shells/impasto/tests/impasto_terminal_spec.lua
--
-- A desktop entry with `Terminal=true` is written into the run's scratch
-- data folder, found through the launcher and started: a floating window
-- with the program on a `ui.Terminal`. The program is real (`true`, `false`)
-- and ends on the wall clock, so its end is waited for with `test.wait`.

local test = morf.test

local function entry(name, exec)
  local dir = morf.env("XDG_DATA_HOME") .. "/applications"
  morf.fs.mkdir(dir, { parents = true })
  morf.fs.write(dir .. "/" .. name .. ".desktop", table.concat({
    "[Desktop Entry]",
    "Type=Application",
    "Name=" .. name,
    "Exec=" .. exec,
    "Terminal=true",
    "",
  }, "\n"))
end

local function floating()
  local count = 0
  for _, surface in ipairs(test.surfaces()) do
    if surface.kind == "floating" then count = count + 1 end
  end
  return count
end

local function launch(name)
  test.ipc("launcher_query", name)
  test.settle(500)
  test.matches(test.ipc("launcher_results"), "^> app  " .. name)
  test.ipc("launcher_key", "enter")
end

local function load()
  test.load("../shell/init.lua", {
    size = { 1280, 720 },
    env = { IMPASTO_DRY_RUN = "1", XDG_DATA_HOME = morf.env("XDG_DATA_HOME") },
  })
  test.settle(3000)
end

test.describe("impasto terminal", function()
  test.it("opens a window for a program and destroys it when it exits", function()
    entry("MorfProbeDone", "true")
    load()
    test.eq(floating(), 0)
    launch("MorfProbeDone")
    test.wait(function() return floating() == 1 end, 3000, "the window opens")
    test.wait(function() return floating() == 0 end, 5000, "and is destroyed at the exit")
  end)

  test.it("keeps a failed program's window up", function()
    entry("MorfProbeFails", "false")
    load()
    launch("MorfProbeFails")
    test.wait(function()
      return test.find { text_contains = "Exited with 1" }
    end, 5000, "the exit is shown")
    test.eq(floating(), 1)
  end)

  test.it("gives back its place: more programs than places, one after another", function()
    entry("MorfProbeDone", "true")
    load()
    for round = 1, 10 do
      launch("MorfProbeDone")
      test.wait(function() return floating() == 1 end, 3000, "window " .. round)
      test.wait(function() return floating() == 0 end, 5000, "gone " .. round)
    end
    test.eq(test.logs("warn")[1] and test.logs("warn")[1].message:find("no terminal") or nil, nil)
  end)
end)
