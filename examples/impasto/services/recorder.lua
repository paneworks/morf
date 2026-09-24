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
-- container: SIGTERM leaves a file nothing can play.
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

local function now_s() return morf.time.now_ms() / 1000 end

local function tick()
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

local function say(summary, body, icon)
  local ok, notify = pcall(require, "services.notifications")
  if ok and notify.post then notify.post { app = "Recorder", summary = summary, body = body or "", icon = icon or "" } end
end

-- ---------------------------------------------------------------- audio --

-- System audio is the default sink's monitor: given a bare audio flag both
-- encoders record the microphone instead.
local pactl = act.which("pactl")
local function probe_audio()
  if not pactl then return end
  morf.run({ pactl, "get-default-sink" }, { timeout_ms = 3000 }, function(result)
    local sink = tostring(result.stdout or ""):match("^%s*(.-)%s*$")
    s.audio_source:set((result.ok and sink ~= "") and (sink .. ".monitor") or "")
  end)
end
probe_audio()

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
  local output = ""
  if not box then
    local screen = (morf.screens or {})[1]
    output = screen and screen.name or ""
  end
  -- One that rejects its arguments, or cannot capture at all, exits at
  -- once; the take goes to the next encoder rather than failing.
  local index = 0
  local function try()
    index = index + 1
    local encoder = encoders[index]
    if not encoder then
      say("The recorder would not start", encoders[1].name, "󰀦")
      return
    end
    local handle = morf.spawn { command = command_for(encoder, box, path, audio, output), detached = true }
    if not handle then return try() end
    morf.timer(300, function()
      if not handle:running() then
        fs.remove(path)
        return try()
      end
      child = handle
      begin(path, shape, audio, handle:pid())
    end, false)
  end
  try()
  return true
end

local function finish()
  local seconds = math.max(0, math.floor(now_s() - started_at))
  s.seconds:set(seconds)
  tick()
  s.recording:set(false)
  tend()
  fs.remove(state_path())
  child = nil
  say("Recorded " .. M.display(), M.name(), "󰕧")
end

function M.stop()
  if not s.recording:get() then return end
  local ok, kept = pcall(morf.json.decode, fs.read(state_path()) or "")
  if not ok or type(kept) ~= "table" then kept = {} end
  if child and child:running() then
    child:kill("INT")
  elseif not kept.dry and alive(tonumber(kept.pid)) then
    -- A take started by an earlier shell: its handle is gone, its pid is not.
    local kill = act.which("kill")
    if kill then morf.run({ kill, "-INT", tostring(math.floor(kept.pid)) }, function() end) end
  end
  -- The encoder closes the container on SIGINT; give it a moment.
  morf.timer(kept.dry and 1 or 600, finish, false)
end

--- Shared by the key, the tile and the island: the whole screen, at once.
--- A region or a window is the capture surface's, in video mode.
function M.toggle()
  if s.recording:get() then M.stop() else M.start("screen", nil) end
end

-- A take left running by a shell that has since restarted.
do
  local text = fs.read(state_path())
  local ok, kept = pcall(morf.json.decode, text or "")
  if ok and type(kept) == "table" then
    if not kept.dry and alive(tonumber(kept.pid)) then
      started_at = tonumber(kept.started) or now_s()
      s.path:set(tostring(kept.path or ""))
      s.shape:set(tostring(kept.shape or "screen"))
      s.sound:set(kept.audio == true)
      s.recording:set(true)
      tick()
      tend()
    else
      fs.remove(state_path())
    end
  end
end

return M
