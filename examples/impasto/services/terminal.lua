-- A program that wants a terminal, in a window of the shell's own.
--
-- A desktop entry with `Terminal=true` (htop, btop, nvim) and the package
-- panel's `sudo pacman` run here: a floating window with a `ui.Terminal`
-- in it, in the island's colours, instead of whichever terminal emulator
-- happens to be installed. The window takes the program's own title, and
-- closes when the program exits (a failure stays up, with its exit code,
-- until Escape or the window is closed, so what went wrong can be read).
-- Closing the window hangs the program up. Ctrl+Shift+C copies what is
-- selected with the pointer.
--
-- Each program gets a window of its own, destroyed -- surface, tree and
-- terminal -- when it is done with. At most eight at once.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color
local M = {}

local MAX = 8
-- Which of the eight places are taken, and each place's status line: a
-- signal made once per place and reused, not one per program run.
local slots = {}
local statuses = {}

-- A theme colour whether the palette moves it (a function) or not.
local function col(name)
  local value = C[name]
  if type(value) == "function" then value = value() end
  return value
end

-- The sixteen ANSI colours, from the palette the island draws with.
local function colours()
  return {
    foreground = col("text"), background = col("island"), cursor = col("accent"),
    palette = {
      "#1c1c1e", col("red"), col("green"), col("yellow"), col("blue"), "#bf5af2", "#64d2ff", "#d1d1d6",
      "#48484a", "#ff6961", "#63e6be", "#ffe066", "#409cff", "#da8fff", "#8ae1ff", "#ffffff",
    },
  }
end

--- The program in `entry` is done with: hung up if it still runs, its window
--- destroyed with everything in it, its place free for the next.
function M.release(entry)
  if entry.released then return end
  entry.released = true
  slots[entry.slot] = nil
  entry.status:set("")
  local term, heard, window = entry.term, entry.heard, entry.window
  entry.term, entry.heard, entry.window = nil, nil, nil
  -- Closed while the program ran: its end is told here, since a destroyed
  -- terminal says nothing more.
  if heard then pcall(heard, nil, nil) end
  if term then
    local ok, running = pcall(function() return term.running end)
    if ok and running then pcall(term.kill, term, "HUP") end
  end
  -- Runs `on_closed` when the window is still up, which comes back here and
  -- finds the entry already released.
  if window then pcall(window.destroy, window) end
end

local function title_of(entry, text)
  if entry.window then pcall(function() entry.window:title(text) end) end
end

--- Runs `command` (an argv) in a terminal window. `options.title` names the
--- window until the program names itself; `options.cwd`, `options.env` go
--- to the program; `options.hold` keeps the window up after a clean exit;
--- `options.on_exit(code)` hears the end. Returns the terminal node, or nil
--- and why.
function M.run(command, options)
  options = options or {}
  if type(command) ~= "table" or #command == 0 then return nil, "no command" end
  local slot
  for index = 1, MAX do
    if not slots[index] then slot = index break end
  end
  if not slot then return nil, "too many terminals open" end
  statuses[slot] = statuses[slot] or morf.signal("impasto.terminal.status." .. slot, "")
  local entry = { slot = slot, status = statuses[slot] }
  slots[slot] = entry
  entry.status:set("")

  local term
  term = ui.Terminal {
    command = command,
    cwd = options.cwd,
    env = options.env,
    font_family = theme.font_mono(),
    font_size = 13,
    padding = 6,
    colors = function() return colours() end,
    anchors = { fill = true },
    focus = true,
    on_title = function(text)
      if text and text ~= "" then title_of(entry, text) end
    end,
    on_key_pressed = function(keysym, _, modifiers)
      local mods = tostring(modifiers or "")
      -- Ctrl+Shift+C: the selection to the clipboard.
      if (keysym == 0x43 or keysym == 0x63) and mods:find("ctrl") and mods:find("shift") then
        local text = term:selection()
        if text and text ~= "" then pcall(morf.clipboard.set, text) end
        return true
      end
      -- Escape closes a window whose program has ended.
      if keysym == 0xff1b and not term.running then
        M.release(entry)
        return true
      end
      return false
    end,
    on_exit = function(code, signal)
      if entry.term == term then entry.heard = nil end
      if options.on_exit then pcall(options.on_exit, code, signal) end
      if entry.term ~= term then return end
      if code == 0 and not options.hold then
        M.release(entry)
        return
      end
      entry.status:set(code == 0 and "Done · Escape closes"
        or ("Exited with " .. tostring(code) .. " · Escape closes"))
    end,
  }
  entry.term = term
  entry.heard = options.on_exit
  local root = ui.Rect {
    color = function() return col("island") end,
    ui.Item { anchors = { fill = true, margins = 8 }, term },
    ui.Text {
      anchors = { right = true, bottom = true, right_margin = 12, bottom_margin = 8 },
      text = function() return entry.status:get() end,
      color = function() return col("textMuted") end,
      font_size = 11,
      visible = function() return entry.status:get() ~= "" end,
    },
  }
  entry.window = morf.window.floating {
    title = options.title or command[1], app_id = "impasto-terminal",
    width = 900, height = 560,
    root = root,
    visible = true,
    -- Closed by the compositor (its close button, a keybinding): the
    -- program is hung up and the window destroyed.
    on_closed = function() M.release(entry) end,
  }
  return term
end

--- How many terminal windows are up now.
function M.open_count()
  local count = 0
  for index = 1, MAX do if slots[index] then count = count + 1 end end
  return count
end

return M
