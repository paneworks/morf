-- The frame-stall rehearsal's spec:
-- `morf test examples/demos/tests/frame_stall_spec.lua`.
--
-- A spring swung beside a busy timer reaches its target on the same frames
-- as without one, and the host's frame accounting charges the timer's cost
-- to the handlers stage, not to paint.

local test = morf.test

local DEMO = "../motion/frame-stall.lua"
-- The strip is the screen's width: the far end is 1280 - 120 - 40.
local TARGET = 1120
local FRAMES = 90
-- Instructions the busy timer burns every frame.
local BUDGET = 1000000

local function load(budget)
  test.load(DEMO, {
    size = { 1280, 720 },
    args = { "--swing", "0", "--interval", "16", "--budget", tostring(budget) },
  })
end

-- Where the spring is at each frame after a swing.
local function swing_path()
  test.ipc("swing")
  local path = {}
  for frame = 1, FRAMES do
    test.advance(16)
    path[frame] = test.ipc("x")
  end
  return path
end

-- The first frame from which the spring stays within half a pixel.
local function settled_at(path)
  local at
  for frame, x in ipairs(path) do
    if math.abs(x - TARGET) < 0.5 then
      at = at or frame
    else
      at = nil
    end
  end
  return at
end

test.describe("frame stall", function()
  test.it("a spring keeps its schedule beside a busy timer", function()
    load(0)
    local still = swing_path()
    load(BUDGET)
    local stalled = swing_path()
    test.truthy(test.ipc("fired") >= FRAMES, "the busy timer fired every frame")
    test.truthy(settled_at(still), "the spring reached its target")
    test.eq(settled_at(stalled), settled_at(still))
    test.eq(stalled, still)
  end)

  test.it("charges the timer's cost to handlers, not paint", function()
    load(BUDGET)
    test.frames()
    swing_path()
    local frames = test.frames()
    test.note(frames.summary)
    test.truthy(frames.motion_turns > 0, "the frames took motion-only turns")
    test.truthy(frames.stages.handlers > 10 * frames.stages.paint, frames.summary)
    test.truthy(frames.stages.handlers > 10 * frames.stages.flush, frames.summary)
    test.eq(frames.owners.paint, 0, frames.summary)

    -- One stall of about a tenth of a second while the spring moves, sized
    -- from what a frame's budget cost here, overruns its frame: the
    -- handlers own it.
    local per_ms = BUDGET * test.ipc("fired") / math.max(frames.stages.handlers, 0.001)
    local big = math.min(math.ceil(per_ms * 100), 400000000)
    test.ipc("swing")
    test.ipc("stall", big, 16)
    test.advance(16)
    test.ipc("stall", 0)
    test.advance(64)
    local stalled = test.frames()
    test.note(stalled.summary)
    test.truthy(stalled.overruns >= 1, stalled.summary)
    test.eq(stalled.owners.handlers, stalled.overruns, stalled.summary)
    test.truthy(stalled.worst_ms >= 2.4 * stalled.refresh_ms, stalled.summary)
  end)
end)
