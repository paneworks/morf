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
-- Windows are reused: a floating window cannot be destroyed, only closed,
-- so a closed one takes the next program. At most eight at once.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color
local M = {}

local MAX = 8
local pool = {}

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

local function make_entry()
  local entry = { busy = false }
  entry.status = morf.signal("impasto.terminal.status." .. (#pool + 1), "")
  entry.body = ui.Item { anchors = { fill = true, margins = 8 } }
  entry.root = ui.Rect {
    color = function() return col("island") end,
    entry.body,
    ui.Text {
      anchors = { right = true, bottom = true, right_margin = 12, bottom_margin = 8 },
      text = function() return entry.status:get() end,
      color = function() return col("textMuted") end,
      font_size = 11,
      visible = function() return entry.status:get() ~= "" end,
    },
  }
  entry.window = morf.window.floating {
    title = "Terminal", app_id = "impasto-terminal",
    width = 900, height = 560,
    root = entry.root,
    visible = false,
    on_closed = function() M.release(entry) end,
  }
  pool[#pool + 1] = entry
  return entry
end

--- The program in `entry` is done with: hung up if it still runs, its node
--- destroyed, the window free for the next.
function M.release(entry)
  local term = entry.term
  local heard = entry.heard
  entry.term, entry.heard = nil, nil
  entry.busy = false
  entry.status:set("")
  -- Closed while the program ran: its end is told here, since a destroyed
  -- terminal says nothing more.
  if heard then pcall(heard, nil, nil) end
  if term then
    if term.running then pcall(term.kill, term, "HUP") end
    pcall(ui.destroy, term)
  end
end

local function title_of(entry, text)
  pcall(function() entry.window:title(text) end)
end

--- Runs `command` (an argv) in a terminal window. `options.title` names the
--- window until the program names itself; `options.cwd`, `options.env` go
--- to the program; `options.hold` keeps the window up after a clean exit;
--- `options.on_exit(code)` hears the end. Returns the terminal node, or nil
--- and why.
function M.run(command, options)
  options = options or {}
  if type(command) ~= "table" or #command == 0 then return nil, "no command" end
  local entry
  for _, candidate in ipairs(pool) do
    if not candidate.busy then entry = candidate break end
  end
  if not entry then
    if #pool >= MAX then return nil, "too many terminals open" end
    entry = make_entry()
  end
  entry.busy = true
  entry.status:set("")
  local name = options.title or command[1]
  title_of(entry, name)
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
        entry.window:close()
        return true
      end
      return false
    end,
    on_exit = function(code, signal)
      if entry.term == term then entry.heard = nil end
      if options.on_exit then pcall(options.on_exit, code, signal) end
      if entry.term ~= term then return end
      if code == 0 and not options.hold then
        entry.window:close()
        return
      end
      entry.status:set(code == 0 and "Done · Escape closes"
        or ("Exited with " .. tostring(code) .. " · Escape closes"))
    end,
  }
  entry.term = term
  entry.heard = options.on_exit
  ui.reparent(term, entry.body)
  entry.window:open()
  return term
end

--- How many terminal windows are up now.
function M.open_count()
  local count = 0
  for _, entry in ipairs(pool) do if entry.busy then count = count + 1 end end
  return count
end


return M
