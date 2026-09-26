-- The capture drawer: out of the frame's bottom edge, as the dashboard
-- comes out of its top. One place for both: choose what -- a region, a
-- window or the screen -- and a delay, then press Screenshot or Record
-- (Record turns into Stop while it runs). Both go to one folder
-- (`capture.folder`), and the latest of them are listed under the buttons;
-- a click opens one. It opens when the pointer reaches the bottom edge,
-- and over IPC (`capture`, `screenshot [region|window|screen]`, `record
-- [region|window|screen]`).
--
-- What each capture runs is a setting (`capture.commands.*`: `$FILE` the
-- file to write, named for the time); CAELESTIA_DRY_RUN=1 logs it instead.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local drawer = require("drawer")
local tabbed = require("tabbed")

local C = theme.color
local M = {}

local CARD_W = 560
M.WIDTH = CARD_W + 2 * tabbed.PAD
local PAD, GAP = 16, 12
local RECENT = 4
local ROW = 36
local CARD_H = PAD + 44 + GAP + 32 + GAP + 64 + GAP + 24 + RECENT * ROW + PAD

M.target = morf.signal("caelestia.capture.target", "region")
M.delay = morf.signal("caelestia.capture.delay", 0)
M.recording = morf.signal("caelestia.capture.recording", false)
M.recent = morf.signal("caelestia.capture.recent", {})

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end

local function expand(word)
  local home = morf.fs.home()
  return (tostring(word):gsub("^~/", home .. "/"):gsub("%$HOME", home))
end

--- The folder captures go to, expanded.
function M.folder() return expand(config.get("capture.folder") or "~/Pictures/Captures") end

--- Lists the folder's latest captures, newest first: `{ path, name, video }`.
function M.scan()
  local ok, entries = pcall(morf.fs.list, M.folder())
  local out = {}
  if ok and type(entries) == "table" then
    for _, e in ipairs(entries) do
      local ext = (e.extension or ""):lower()
      local video = ext == "mp4" or ext == "mkv" or ext == "webm"
      if e.is_file and (video or ext == "png" or ext == "jpg") then
        out[#out + 1] = { path = e.path, name = e.name or e.path:match("([^/]+)$"), video = video }
      end
    end
  end
  table.sort(out, function(a, b) return a.name > b.name end)
  M.recent:set(out)
end

--- Runs `capture.commands.NAME` with `extra` words after it; `$FILE` is a
--- new file in the folder, named for the time (the command adds its
--- extension). `done(result)` hears how it ended.
local function run(name, extra, done)
  local words = config.get("capture.commands." .. name)
  if type(words) ~= "table" or #words == 0 then
    morf.log("info", "caelestia: capture " .. name .. ": nothing to run")
    return false
  end
  local file = M.folder() .. "/capture_" .. morf.time.format("%Y%m%d_%H-%M-%S")
  local argv = {}
  for i, w in ipairs(words) do argv[i] = (expand(w):gsub("%$FILE", file)) end
  for _, w in ipairs(extra or {}) do argv[#argv + 1] = w end
  if dry_run() then
    morf.log("info", "caelestia: capture " .. name .. " (dry run): " .. table.concat(argv, " "))
    return true
  end
  pcall(morf.fs.mkdir, M.folder())
  morf.run(argv, {}, function(result)
    if result and not result.ok then
      morf.log("warn", "caelestia: capture " .. name .. " failed: " .. tostring(result.stderr or result.code))
    end
    M.scan()
    if done then done(result) end
  end)
  return true
end

local d -- the drawer, below

-- The drawer shuts first, so it is not in the picture; then the delay.
local function after_shutting(fn)
  if d then d.set(false) end
  morf.timer(250 + (M.delay:get() or 0) * 1000, fn, false)
end

local TARGETS = {
  { id = "region", name = "Region", icon = "screenshot_region" },
  { id = "window", name = "Window", icon = "select_window" },
  { id = "screen", name = "Screen", icon = "fullscreen" },
}
local function checked(what)
  what = what or M.target:get()
  for _, t in ipairs(TARGETS) do if t.id == what then return what end end
  error("`" .. tostring(what) .. "`: region, window or screen")
end

--- Takes a screenshot of `what` (the chosen target by default).
function M.shoot(what)
  what = checked(what)
  after_shutting(function() run("screenshot_" .. what) end)
  return what
end

--- Starts recording `what` (the chosen target by default), or stops.
function M.record(what)
  if M.recording:get() then
    run("record_stop")
    M.recording:set(false)
    return "stopped"
  end
  what = checked(what)
  M.recording:set(true)
  after_shutting(function()
    run("record_" .. what, nil, function() M.recording:set(false) end)
  end)
  return what
end

-- ------------------------------------------------------------------ pieces --

--- A button whose shape answers the pointer: rounder at rest, squarer on,
--- and squarer still under the finger (M3 expressive's shape morph).
--- `props.color(on)` answers the fill and the colour over it.
local function button(props)
  local area
  local shape = ui.Rect {
    anchors = { fill = true },
    behavior = { radius = ui.spring { stiffness = 420, damping = 26 }, color = { duration = theme.duration.small } },
  }
  local on = props.on or function() return false end
  local rest = props.radius or props.height / 2
  area = ui.MouseArea {
    id = props.id, x = props.x, width = props.width, height = props.height, cursor = "pointer",
    on_clicked = props.on_clicked,
    shape,
    props.child,
  }
  shape.radius = function()
    if area.pressed then return math.min(10, rest) end
    return on() and math.min(12, rest) or rest
  end
  shape.color = function()
    local base, fg = props.color(on())
    return area.hovered and base:mix(fg, 0.08) or base
  end
  return area
end

local DELAYS = { { s = 0, name = "Now" }, { s = 3, name = "3 s" }, { s = 5, name = "5 s" } }

local function targets(w)
  local bw = (w - 2 * 8) / 3
  local row = { gap = 8 }
  for _, t in ipairs(TARGETS) do
    local function on() return M.target:get() == t.id end
    local fg = function() return on() and C.onPrimary or C.onSurface end
    row[#row + 1] = button {
      id = "capture-target-" .. t.id, width = bw, height = 44,
      on = on, on_clicked = function() M.target:set(t.id) end,
      color = function(lit) return lit and C.primary or C.surfaceContainerHighest, lit and C.onPrimary or C.onSurface end,
      child = ui.Row {
        anchors = { center_in = true }, gap = 8, align = "center",
        kit.icon(t.icon, 20, fg),
        kit.text { text = t.name, font_size = theme.size.normal, color = fg },
      },
    }
  end
  return ui.Row(row)
end

local function delays_and_folder(w)
  local row = { gap = 6 }
  for _, t in ipairs(DELAYS) do
    local function on() return M.delay:get() == t.s end
    row[#row + 1] = button {
      id = "capture-delay-" .. t.s, width = 56, height = 32,
      on = on, on_clicked = function() M.delay:set(t.s) end,
      color = function(lit) return lit and C.secondaryContainer or C.surfaceContainerHighest, C.onSurface end,
      child = kit.text {
        anchors = { center_in = true }, text = t.name, font_size = theme.size.small,
        color = function() return on() and C.onSecondaryContainer or C.onSurface end,
      },
    }
  end
  local folder_w = w - 52 - 3 * 56 - 2 * 6 - GAP
  return ui.Item {
    width = w, height = 32,
    kit.text {
      anchors = { vertical_center = true }, text = "Delay", font_size = theme.size.normal,
      color = function() return C.onSurfaceVariant end,
    },
    ui.Row { x = 52, anchors = { vertical_center = true }, table.unpack(row) },
    button {
      id = "capture-folder", x = w - folder_w, width = folder_w, height = 32,
      on_clicked = function() run("open", { M.folder() }) end,
      color = function() return C.surfaceContainerHighest, C.onSurface end,
      child = ui.Row {
        x = 12, anchors = { vertical_center = true }, gap = 8, align = "center",
        kit.icon("folder_open", 18, function() return C.onSurfaceVariant end),
        kit.text {
          width = folder_w - 44, elide = "middle", font_size = theme.size.small,
          text = function() return config.get("capture.folder") or "" end,
          color = function() return C.onSurface end,
        },
      },
    },
  }
end

local function actions(w)
  local bw = (w - GAP) / 2
  local shot = button {
    id = "capture-screenshot", width = bw, height = 64, radius = 20,
    on_clicked = function() M.shoot() end,
    color = function() return C.primaryContainer, C.onPrimaryContainer end,
    child = ui.Row {
      anchors = { center_in = true }, gap = 10, align = "center",
      kit.icon("photo_camera", 26, function() return C.onPrimaryContainer end),
      kit.text { text = "Screenshot", font_size = theme.size.large, color = function() return C.onPrimaryContainer end },
    },
  }
  local function live() return M.recording:get() end
  local rec = button {
    id = "capture-record", width = bw, height = 64, radius = 20,
    on = live,
    on_clicked = function() M.record() end,
    color = function(lit)
      if lit then return C.error, C.onError end
      return C.tertiaryContainer, C.onTertiaryContainer
    end,
    child = ui.Row {
      anchors = { center_in = true }, gap = 10, align = "center",
      kit.icon(function() return live() and "stop_circle" or "screen_record" end, 26,
        function() return live() and C.onError or C.onTertiaryContainer end),
      kit.text {
        text = function() return live() and "Stop" or "Record" end,
        font_size = theme.size.large,
        color = function() return live() and C.onError or C.onTertiaryContainer end,
      },
    },
  }
  return ui.Row { gap = GAP, shot, rec }
end

local function recent(w)
  local rows = {}
  for i = 1, RECENT do
    local function item() return M.recent:get()[i] end
    local wash = ui.Rect { anchors = { fill = true }, radius = 12, behavior = { color = { duration = theme.duration.small } } }
    local area = ui.MouseArea {
      id = "capture-recent-" .. i, width = w, height = ROW, cursor = "pointer",
      visible = function() return item() ~= nil end,
      on_clicked = function()
        local it = item()
        if it then run("open", { it.path }) end
      end,
      wash,
      ui.Row {
        x = 10, anchors = { vertical_center = true }, gap = 10, align = "center",
        kit.icon(function() local it = item() return it and it.video and "movie" or "image" end, 18,
          function() return C.onSurfaceVariant end),
        kit.text { width = w - 48, elide = "middle", text = function() local it = item() return it and it.name or "" end },
      },
    }
    wash.color = function() return area.hovered and C.onSurface:alpha(0.06) or C.onSurface:alpha(0) end
    rows[i] = area
  end
  return ui.Column {
    gap = 0,
    kit.text {
      height = 24, text = "Recent", font_size = theme.size.small,
      color = function() return C.onSurfaceVariant end,
    },
    ui.Item {
      width = w, height = RECENT * ROW,
      ui.Column { gap = 0, table.unpack(rows) },
      ui.Row {
        id = "capture-none",
        anchors = { center_in = true }, gap = 8, align = "center",
        visible = function() return #M.recent:get() == 0 end,
        kit.icon("hide_image", 18, function() return C.outline end),
        kit.text { text = "Nothing captured yet", font_size = theme.size.normal, color = function() return C.outline end },
      },
    },
  }
end

local function page(w)
  local inner = w - 2 * PAD
  return kit.card {
    id = "capture-page",
    width = w, height = CARD_H,
    ui.Column {
      x = PAD, y = PAD, gap = GAP,
      targets(inner),
      delays_and_folder(inner),
      actions(inner),
      recent(inner),
    },
  }
end

-- ------------------------------------------------------------------ drawer --

M.TABS = {
  { key = "capture", name = "Capture", icon = "screenshot_monitor", build = function(w) return page(w) end },
}

function M.height() return tabbed.TABS_H + 2 * tabbed.PAD + CARD_H end

local panel = tabbed.new { id = "capture", width = M.WIDTH, height = M.height, tabs = M.TABS }
M.panel = panel

d = drawer.new {
  name = "capture",
  edge = "bottom",
  width = M.WIDTH,
  height = M.height,
  content = panel.content,
}
M.drawer = d

morf.effect("caelestia.capture.shown", function()
  local open = d.open:get()
  panel.shown(open)
  if open then M.scan() end
end)

return M
