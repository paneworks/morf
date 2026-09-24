-- One screen recording at a time: started, timed and stopped here.
--
-- Port of RecorderService.qml and scripts/record.py, without the script.
-- The encoder is another program -- wl-screenrec, wf-recorder or
-- gpu-screen-recorder, whichever is installed, best first -- started by argv
-- and detached, so a take outlives a restarted shell. Its pid, file and
-- start time are kept in a small state file in the runtime folder, and a
-- shell that starts while a take is running picks it up from there.
--
-- Elapsed time is the wall clock minus the start, not a count of ticks, so
-- it cannot drift from the file; the ticker runs only while recording. A
-- take is stopped with SIGINT, which is what makes both encoders close the
-- container: SIGTERM leaves a file nothing can play. The take ends when the
-- encoder exits (its `on_exit`), not after a guess; an encoder that dies
-- mid-take is noticed and said. Every screen's shell sees the one take
-- through the state file, whichever screen started it, and any of them can
-- stop it. What happened is flashed on the island, as the original's OSD.
--
-- With IMPASTO_DRY_RUN set nothing is started: the take is pretended, so
-- the island's activity and the tile can be looked at on a test bench.

local settings = require("services.settings")
local act = require("services.act")

local fs = morf.fs
local M = {}

local ENCODERS = { "wl-screenrec", "wf-recorder", "gpu-screen-recorder" }

M.shape_names = { screen = "The screen", region = "A region", window = "A window" }

local s = {
  recording = morf.signal("impasto.recorder.recording", false),
  seconds = morf.signal("impasto.recorder.seconds", 0),
  path = morf.signal("impasto.recorder.path", ""),
  shape = morf.signal("impasto.recorder.shape", "screen"),
  sound = morf.signal("impasto.recorder.sound", false),
  size = morf.signal("impasto.recorder.size", 0),
  audio_source = morf.signal("impasto.recorder.audio", ""),
}
M.signals = s

local encoders = {}
for _, name in ipairs(ENCODERS) do
  local path = act.which(name)
  if path then encoders[#encoders + 1] = { name = name, path = path } end
end

M.tool = encoders[1] and encoders[1].name or ""
function M.available() return M.tool ~= "" end
function M.can_audio() return s.audio_source:get() ~= "" end
function M.recording() return s.recording:get() end
function M.seconds() return s.seconds:get() end
function M.path() return s.path:get() end
function M.name() return (s.path:get():match("([^/]+)$")) or "" end
function M.sound() return s.sound:get() end
function M.size() return s.size:get() end

--- "4:05".
function M.display()
  local total = math.max(0, s.seconds:get())
  return ("%d:%02d"):format(total // 60, total % 60)
end

--- The running take's shape, or the screen the next one takes.
function M.subject()
  return M.shape_names[s.recording:get() and s.shape:get() or "screen"] or M.shape_names.screen
end

--- Where recordings go: $IMPASTO_RECORDINGS, $XDG_VIDEOS_DIR, the XDG
--- videos folder, the home folder.
function M.directory()
  for _, name in ipairs { "IMPASTO_RECORDINGS", "XDG_VIDEOS_DIR" } do
    local value = morf.env(name)
    if value and value ~= "" then return fs.expand and fs.expand(value) or value end
  end
  return fs.dir("videos") or fs.home()
end

local function state_path()
  -- A test bench's pretended takes never meet a real one.
  local name = act.dry and "impasto-morf-recording-dry.json" or "impasto-morf-recording.json"
  return fs.join(fs.dir("runtime") or "/tmp", name)
end

local function alive(pid)
  return pid and pid > 0 and fs.exists("/proc/" .. math.floor(pid)) or false
end

-- ----------------------------------------------------------------- clock --

local started_at, ticker, child = 0, nil, nil
-- The pid of a take this runtime did not start (another screen's, or an
-- earlier shell's): watched from the ticker, since there is no handle.
local adopted_pid = nil
local stopping = false
local finish

local function now_s() return morf.time.now_ms() / 1000 end

local function tick()
  if adopted_pid and not alive(adopted_pid) then
    -- Gone without this runtime's word: stopped from another screen, or
    -- the encoder died. The state file says which.
    local ok, kept = pcall(morf.json.decode, fs.read(state_path()) or "")
    local asked = stopping or not (ok and type(kept) == "table") or kept.stopping == true
    adopted_pid = nil
    finish(not asked)
    return
  end
  s.seconds:set(math.max(0, math.floor(now_s() - started_at)))
  local path = s.path:get()
  if path ~= "" then
    local stat = fs.stat and fs.stat(path)
    if stat and stat.size then s.size:set(stat.size) end
  end
end

local function tend()
  if s.recording:get() and not ticker then
    ticker = morf.timer(1000, tick, true)
  elseif not s.recording:get() and ticker then
    ticker:cancel()
    ticker = nil
  end
end

local function say(summary, _, icon)
  local ok, osd = pcall(require, "services.osd")
  if ok and osd.request then osd.request(icon or "󰕧", summary, -1) end
end

-- ---------------------------------------------------------------- audio --

-- System audio is the default sink's monitor: given a bare audio flag both
-- encoders record the microphone instead. `morf.audio` follows the default
-- sink, so a change of output is a change of source.
morf.effect("impasto.recorder.audio", function()
  local sink = morf.audio.available() and morf.audio.default_sink() or nil
  s.audio_source:set(sink and sink.name ~= "" and (sink.name .. ".monitor") or "")
end)

-- ---------------------------------------------------------------- command --

local function geometry(box)
  return ("%d,%d %dx%d"):format(box.x, box.y, box.width, box.height)
end

local function command_for(encoder, box, path, audio, output)
  local source = s.audio_source:get()
  if encoder.name == "gpu-screen-recorder" then
    local argv = { encoder.path, "-f", "60", "-o", path }
    if box then
      argv[#argv + 1] = "-w"; argv[#argv + 1] = "region"
      argv[#argv + 1] = "-region"
      argv[#argv + 1] = ("%dx%d+%d+%d"):format(box.width, box.height, box.x, box.y)
    else
      argv[#argv + 1] = "-w"; argv[#argv + 1] = (output ~= "" and output or "screen")
    end
    if audio then argv[#argv + 1] = "-a"; argv[#argv + 1] = "default_output" end
    return argv
  end
  local argv = { encoder.path, "-f", path }
  if box then
    argv[#argv + 1] = "-g"; argv[#argv + 1] = geometry(box)
  elseif output ~= "" then
    argv[#argv + 1] = "-o"; argv[#argv + 1] = output
  end
  if audio then
    if encoder.name == "wl-screenrec" then
      argv[#argv + 1] = "--audio"
      if source ~= "" then argv[#argv + 1] = "--audio-device"; argv[#argv + 1] = source end
    else
      argv[#argv + 1] = source ~= "" and ("--audio=" .. source) or "--audio"
    end
  end
  return argv
end

-- ------------------------------------------------------------ start, stop --

local function begin(path, shape, audio, pid)
  started_at = now_s()
  s.path:set(path)
  s.shape:set(shape)
  s.sound:set(audio)
  s.size:set(0)
  s.seconds:set(0)
  s.recording:set(true)
  tend()
  fs.write(state_path(), morf.json.encode {
    pid = pid or 0, path = path, started = started_at, shape = shape, audio = audio,
    dry = pid == nil,
  })
  -- A flash, as the original's OSD: a notification would cover the
  -- island's recording mark for its whole timeout.
  local ok, island_state = pcall(require, "bar.island_state")
  if ok then island_state.flash("󰑊", "Recording", nil) end
end

--- Records the screen, or `box` ({ x, y, width, height } in the
--- compositor's coordinates) from the capture surface. `shape` names what
--- the box was, since a window and a region are the same rectangle by now.
function M.start(shape, box)
  if s.recording:get() then return false end
  shape = M.shape_names[shape or ""] and shape or (box and "region" or "screen")
  local audio = settings.recorderAudio and M.can_audio()
  local dir = M.directory()
  local path = fs.join(dir, morf.time.format("%Y-%m-%d-%H%M%S") .. "_impasto.mp4")
  if act.dry then
    morf.log("info", "impasto: dry run, not recording " .. shape .. " to " .. path)
    begin(path, shape, audio, nil)
    return true
  end
  if #encoders == 0 then
    say("No screen recorder installed", "wl-screenrec, wf-recorder or gpu-screen-recorder", "󰀦")
    return false
  end
  fs.mkdir(dir, { parents = true })
  -- The whole screen is the one being worked on: Hyprland's focused
  -- monitor, else this shell's own.
  local output = ""
  if not box then
    local live = require("services.live")
    output = live.name()
    if output == "" then
      local screen = (morf.screens or {})[1]
      output = screen and screen.name or ""
    end
  end
  -- One that rejects its arguments, or cannot capture at all, exits at
  -- once; the take goes to the next encoder rather than failing. An
  -- encoder still running after GRACE is recording; one that exits later
  -- ended the take.
  local GRACE = 400
  local index = 0
  local function try()
    index = index + 1
    local encoder = encoders[index]
    if not encoder then
      say("The recorder would not start", encoders[1].name, "󰀦")
      return
    end
    local confirmed, gone = false, false
    local handle
    handle = morf.spawn {
      command = command_for(encoder, box, path, audio, output),
      detached = true,
      on_exit = function()
        gone = true
        if not confirmed then
          fs.remove(path)
          return try()
        end
        if child == handle then
          child = nil
          -- Not asked to stop: the encoder died under the take.
          finish(not stopping)
        end
      end,
    }
    if not handle then return try() end
    morf.timer(GRACE, function()
      if gone then return end
      confirmed = true
      child = handle
      stopping = false
      begin(path, shape, audio, handle:pid())
    end, false)
  end
  try()
  return true
end

--- The take is over: `died` when the encoder went without being asked.
finish = function(died)
  local seconds = math.max(0, math.floor(now_s() - started_at))
  s.seconds:set(seconds)
  local path = s.path:get()
  if path ~= "" then
    local stat = fs.stat and fs.stat(path)
    if stat and stat.size then s.size:set(stat.size) end
  end
  s.recording:set(false)
  tend()
  fs.remove(state_path())
  child, adopted_pid, stopping = nil, nil, false
  if died then
    say("The recorder stopped after " .. M.display(), M.name(), "󰀦")
  else
    say("Recorded " .. M.display(), M.name(), "󰕧")
  end
end

function M.stop()
  if not s.recording:get() or stopping then return end
  stopping = true
  local ok, kept = pcall(morf.json.decode, fs.read(state_path()) or "")
  if not ok or type(kept) ~= "table" then kept = {} end
  if kept.dry then
    morf.timer(1, function() finish(false) end, false)
    return
  end
  -- Marked first, so whichever screen started it hears a stop, not a death.
  kept.stopping = true
  fs.write(state_path(), morf.json.encode(kept))
  if child and child:running() then
    -- Its `on_exit` finishes the take, once the container is closed; five
    -- seconds is as long as it is waited for (record.py).
    local asked = child
    asked:kill("INT")
    morf.timer(5000, function()
      if child == asked and stopping then
        child = nil
        finish(false)
      end
    end, false)
    return
  end
  local pid = tonumber(kept.pid)
  if not alive(pid) then
    finish(false)
    return
  end
  -- Another screen's take, or an earlier shell's: no handle, only its pid.
  -- Up to five seconds for the encoder to close the file (record.py).
  morf.kill(math.floor(pid), "INT")
  local waited = 0
  local timer
  timer = morf.timer(100, function()
    waited = waited + 100
    if alive(pid) and waited < 5000 then return end
    timer:cancel()
    if s.recording:get() then finish(false) end
  end, true)
end

--- Shared by the key, the tile and the island: the whole screen, at once.
--- A region or a window is the capture surface's, in video mode.
function M.toggle()
  if s.recording:get() then M.stop() else M.start("screen", nil) end
end

-- A take this runtime did not start: left running by a shell that has
-- since restarted, or started on another screen. Looked for at load and
-- every two seconds while idle (a stat of the state file).
local function adopt(at_load)
  local text = fs.read(state_path())
  if not text then return end
  local ok, kept = pcall(morf.json.decode, text)
  if not ok or type(kept) ~= "table" then return end
  if kept.dry then
    -- A pretended take is only its own runtime's.
    if at_load then fs.remove(state_path()) end
    return
  end
  local pid = tonumber(kept.pid)
  if not alive(pid) then
    if at_load then fs.remove(state_path()) end
    return
  end
  adopted_pid = pid
  started_at = tonumber(kept.started) or now_s()
  s.path:set(tostring(kept.path or ""))
  s.shape:set(tostring(kept.shape or "screen"))
  s.sound:set(kept.audio == true)
  s.recording:set(true)
  tick()
  tend()
end
adopt(true)
morf.timer(2000, function()
  if not s.recording:get() then adopt(false) end
end, true)

return M
