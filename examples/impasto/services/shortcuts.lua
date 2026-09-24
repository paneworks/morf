-- The key sheet: every key the compositor has bound right now, grouped.
--
-- Port of the key-sheet half of ShortcutService.qml. Binds come from
-- Hyprland (`lib.hyprland`'s `binds()`, what `hyprctl binds` prints); a
-- bind's description reads "Group · Action", and binds sharing a group,
-- modifiers and an action up to a trailing number or direction fold into
-- one row ("Workspace 1…9"). Switches are left out.
--
-- Read-only. The original's other half rewrote the user's Hyprland key
-- file and reloaded the compositor; this port never writes the
-- compositor's configuration, so rebinding is not offered.

local M = {}

local ok_lib, hyprland = pcall(require, "lib.hyprland")
if not ok_lib then hyprland = nil end

local s = {
  revision = morf.signal("impasto.keys.revision", 0),
  -- "", "reading", "ok", or why there are no binds.
  status = morf.signal("impasto.keys.status", ""),
}
M.signals = s

local binds = {}

M.sheet_width = 620
M.sheet_height = 600
M.field_height = 36
M.row_height = 36
M.heading_height = 32

-- Unlisted groups follow, in the order they first appear.
M.order = { "Shell", "Applications", "Windows", "Workspaces", "Session", "Utilities", "Media" }

local MODIFIERS = { { bit = 64, cap = "Super" }, { bit = 4, cap = "Ctrl" }, { bit = 8, cap = "Alt" }, { bit = 1, cap = "Shift" } }

-- Keycap labels where the xkb name will not do. Arrows come from the icon
-- font: the interface face lacks them.
local CAPS = {
  left = "󰁍", right = "󰁔", up = "󰁝", down = "󰁅",
  TAB = "Tab", Tab = "Tab", Return = "Enter", space = "Space", Escape = "Esc",
  comma = ",", period = ".", slash = "/", minus = "-", equal = "=",
  ["mouse:272"] = "Left button", ["mouse:273"] = "Right button",
  mouse_down = "Wheel down", mouse_up = "Wheel up",
  XF86AudioRaiseVolume = "Vol +", XF86AudioLowerVolume = "Vol −",
  XF86AudioMute = "Mute", XF86AudioMicMute = "Mic mute",
  XF86MonBrightnessUp = "Bright +", XF86MonBrightnessDown = "Bright −",
  XF86AudioPlay = "Play", XF86AudioNext = "Next", XF86AudioPrev = "Previous",
}

function M.cap_of(key)
  if CAPS[key] then return CAPS[key] end
  -- Hyprland keeps the key as the config spelled it: SPACE, Return, return.
  if CAPS[key:lower()] then return CAPS[key:lower()] end
  if key == "RETURN" then return "Enter" end
  if key:sub(1, 4) == "XF86" then return key:sub(5) end
  return key
end

local function rank(name, first_seen)
  for index, each in ipairs(M.order) do if each == name then return index end end
  return 99 + first_seen
end

local sheet = {}

local function rebuild()
  local groups, order = {}, {}
  for _, bind in ipairs(binds) do
    local key = tostring(bind.key or "")
    if key ~= "" and key:sub(1, 7) ~= "switch:" then
      local text = tostring(bind.description or "")
      local category, action = text:match("^(.-) · (.+)$")
      if not category then
        category = "Other"
        action = text ~= "" and text
          or (tostring(bind.dispatcher or "") .. " " .. tostring(bind.arg or "")):match("^%s*(.-)%s*$")
        if action == "" then action = key end
      end
      local base, tail = action:match("^(.*) (%d+)$")
      local kind = base and "digits" or ""
      if not base then
        for _, word in ipairs { "left", "right", "up", "down" } do
          local b = action:match("^(.*) " .. word .. "$")
          if b then base, kind = b, "words" break end
        end
      end
      base = base or action
      if not groups[category] then
        groups[category] = {}
        order[#order + 1] = category
      end
      local rows = groups[category]
      local same
      if kind ~= "" then
        for _, row in ipairs(rows) do
          if row.base == base and row.kind == kind and row.modmask == (tonumber(bind.modmask) or 0) then same = row break end
        end
      end
      if same then
        same.keys[#same.keys + 1] = key
      else
        rows[#rows + 1] = { base = base, action = action, kind = kind, modmask = tonumber(bind.modmask) or 0, keys = { key } }
      end
      local _ = tail
    end
  end
  local seen = {}
  for index, name in ipairs(order) do seen[name] = index end
  table.sort(order, function(a, b) return rank(a, seen[a]) < rank(b, seen[b]) end)

  sheet = {}
  for _, category in ipairs(order) do
    local out = {}
    for _, row in ipairs(groups[category]) do
      local caps = {}
      for _, modifier in ipairs(MODIFIERS) do
        if row.modmask & modifier.bit ~= 0 then caps[#caps + 1] = modifier.cap end
      end
      local many = #row.keys > 1
      if many and row.kind == "digits" then
        caps[#caps + 1] = row.keys[1] .. "…" .. row.keys[#row.keys]
      else
        for _, key in ipairs(row.keys) do caps[#caps + 1] = M.cap_of(key) end
      end
      out[#out + 1] = { action = many and row.base or row.action, caps = caps, keys = row.keys }
    end
    sheet[#sheet + 1] = { name = category, rows = out }
  end
  s.revision:set(s.revision:get() + 1)
end

function M.sheet() s.revision:get() return sheet end
function M.status() return s.status:get() end

function M.count()
  local n = 0
  for _, group in ipairs(M.sheet()) do n = n + #group.rows end
  return n
end

-- What a term can start: the words of the action and its group, the caps
-- and the xkb names behind them.
local function words_of(group, row)
  local text = (group.name .. " " .. row.action .. " " .. table.concat(row.caps, " ") .. " "
    .. table.concat(row.keys, " ")):lower()
  local out = {}
  for word in text:gmatch("[%w]+") do out[#out + 1] = word end
  for _, key in ipairs(row.keys) do out[#out + 1] = key:lower() end
  return out
end

--- The sheet as one list, headings between groups, keeping the rows every
--- term of `query` begins a word of. Terms split on spaces and on `+`.
function M.find(query)
  local terms = {}
  for term in tostring(query or ""):lower():gmatch("[^%s+]+") do terms[#terms + 1] = term end
  local out = {}
  for _, group in ipairs(M.sheet()) do
    local kept = {}
    for _, row in ipairs(group.rows) do
      local words = words_of(group, row)
      local all = true
      for _, term in ipairs(terms) do
        local any = false
        for _, word in ipairs(words) do
          if word:sub(1, #term) == term then any = true break end
        end
        if not any then all = false break end
      end
      if all then kept[#kept + 1] = row end
    end
    if #kept > 0 then
      out[#out + 1] = { heading = true, name = group.name }
      for _, row in ipairs(kept) do out[#out + 1] = { heading = false, action = row.action, caps = row.caps } end
    end
  end
  return out
end

--- Reads the binds again, so a rebind made since the last opening shows.
function M.load()
  if not hyprland or not hyprland.available or not hyprland.available() then
    if #binds == 0 then s.status:set("Hyprland is not running, so there is no key list to read.") end
    return
  end
  if #binds == 0 then s.status:set("reading") end
  hyprland.binds(function(list, err)
    if type(list) ~= "table" then
      s.status:set("The key list did not come back: " .. tostring(err))
      return
    end
    binds = list
    s.status:set("ok")
    rebuild()
  end)
end

--- A test bench's binds, as Hyprland would print them.
function M.sample()
  local b = {}
  local function add(modmask, key, description)
    b[#b + 1] = { modmask = modmask, key = key, description = description }
  end
  add(64, "SPACE", "Shell · Open the launcher")
  add(64, "A", "Shell · Open the control centre")
  add(64, "TAB", "Shell · Open the workspace overview")
  add(64, "K", "Shell · Show every key")
  add(65, "S", "Shell · Capture a region")
  add(64, "Print", "Shell · Capture the whole screen")
  add(65, "R", "Shell · Start or stop recording the screen")
  add(65, "C", "Shell · Pick a colour off the screen")
  add(64, "Return", "Applications · Terminal")
  add(64, "B", "Applications · Browser")
  add(64, "E", "Applications · Files")
  add(64, "Q", "Windows · Close")
  add(64, "F", "Windows · Fullscreen")
  add(64, "V", "Windows · Float")
  for _, d in ipairs { "left", "right", "up", "down" } do add(64, d, "Windows · Focus " .. d) end
  for _, d in ipairs { "left", "right", "up", "down" } do add(65, d, "Windows · Move " .. d) end
  for i = 1, 9 do add(64, tostring(i), "Workspaces · Workspace " .. i) end
  for i = 1, 9 do add(65, tostring(i), "Workspaces · Send to workspace " .. i) end
  add(64, "mouse_down", "Workspaces · Next")
  add(64, "L", "Session · Lock the screen")
  add(69, "Delete", "Session · Session menu")
  add(0, "XF86AudioRaiseVolume", "Media · Louder")
  add(0, "XF86AudioLowerVolume", "Media · Quieter")
  add(0, "XF86AudioMute", "Media · Mute")
  add(0, "XF86AudioPlay", "Media · Play or pause")
  add(0, "switch:Lid Switch", "Session · Lid")
  binds = b
  s.status:set("ok")
  rebuild()
end

return M
