-- The shell is the notification daemon.
--
-- Port of NotificationService.qml over library/lib/notifications.lua. One
-- notification at a time is `current` -- the island shows it -- and the last
-- fifty are kept as history for the notifications module. A critical one
-- never times out and is not replaced by an ordinary one; with Do not
-- disturb on, only critical ones reach the island, though all are kept.
--
-- The name is queued for, not taken: a shell that starts beside another
-- daemon waits its turn instead of stealing it.
--
-- Every notification expires on its own clock from arrival, shown or not
-- (critical ones never; a replacement starts its clock again), and the
-- application is told it expired. Whatever closes it -- that clock, or the
-- application itself -- takes it off the island; the history keeps it. One
-- pushed out of the history by the limit is expired, so it cannot come back
-- as new. Only what the island draws is claimed on the bus (body, markup,
-- images): an application told "actions" sends buttons nobody shows. Each
-- entry still carries its `actions`, and `M.invoke(id, key)` answers one,
-- for a list that draws them.

local daemon = require("lib.notifications")
local settings = require("services.settings")

local M = {}

M.HISTORY_LIMIT = 50
M.current = morf.signal("impasto.notify.current", 0)     -- id, 0 for none
M.revision = morf.signal("impasto.notify.revision", 0)   -- bumps on any change

local entries = {}      -- id -> entry (live or kept)
local history = {}      -- newest first
local seen = {}         -- id -> the entry table last handed over, for "new"
local clocks = {}       -- id -> generation of its expiry timer
local current_timer = 0
local server

local function bump() M.revision:set(M.revision:get() + 1) end

function M.entry(id) return entries[id] end

--- The notification on the island, or nil. A binding follows it.
function M.shown()
  local id = M.current:get()
  return id ~= 0 and entries[id] or nil
end

function M.active() return M.current:get() ~= 0 end
function M.critical()
  local shown = M.shown()
  return shown ~= nil and shown.urgency == 2
end

--- The history, newest first. A binding follows it.
function M.history()
  M.revision:get()
  return history
end

local function timeout_for(entry)
  if entry.urgency == 2 then return 0 end
  if entry.timeout_ms and entry.timeout_ms > 0 then return math.min(entry.timeout_ms, 15000) end
  return settings.notificationTimeout
end

local function present(entry)
  local is_critical = entry.urgency == 2
  if settings.doNotDisturb and not is_critical then return end
  if M.critical() and not is_critical then return end
  M.current:set(entry.id)
  current_timer = current_timer + 1
  -- The daemon's own entries leave the island when they expire (below); the
  -- shell's own have no daemon behind them, so the island times them out.
  if entry.id > 0 then return end
  local mine = current_timer
  local timeout = timeout_for(entry)
  if timeout > 0 then
    morf.timer(timeout, function()
      if mine == current_timer and M.current:get() == entry.id then M.dismiss() end
    end, false)
  end
end

--- Starts (or restarts) the clock of an open notification.
local function expire_later(entry)
  local timeout = timeout_for(entry)
  clocks[entry.id] = (clocks[entry.id] or 0) + 1
  if timeout <= 0 then return end
  local mine = clocks[entry.id]
  morf.timer(timeout, function()
    if clocks[entry.id] ~= mine then return end
    clocks[entry.id] = nil
    if server and server.open(entry.id) then server.expire(entry.id) end
  end, false)
end

-- Trims the history to its limit; what falls off is expired if still open.
-- Returns the ids to expire, which the caller does once it is done with the
-- list (expiring changes the list, which calls back in here).
local function trim()
  local expired = {}
  while #history > M.HISTORY_LIMIT do
    local gone = table.remove(history)
    entries[gone.id] = nil
    clocks[gone.id] = nil
    if gone.id > 0 then expired[#expired + 1] = gone.id end
  end
  return expired
end

local function on_change(list)
  local open = {}
  local expired = {}
  for _, entry in ipairs(list) do
    open[entry.id] = true
    local known = seen[entry.id]
    seen[entry.id] = entry
    if not known then
      entries[entry.id] = entry
      table.insert(history, 1, entry)
      for _, id in ipairs(trim()) do expired[#expired + 1] = id end
      expire_later(entry)
      present(entry)
    elseif known ~= entry then
      -- A replacement (same id): keep its place in history, start its clock
      -- again, and show it again if it is on the island.
      if entries[entry.id] then
        entries[entry.id] = entry
        for index, kept in ipairs(history) do
          if kept.id == entry.id then history[index] = entry end
        end
      end
      expire_later(entry)
      if M.current:get() == entry.id then present(entry) end
    end
  end
  -- Gone from the daemon for any reason -- expired, closed by its
  -- application, dismissed -- is gone from the island; the history keeps it.
  for id in pairs(seen) do
    if not open[id] then
      seen[id] = nil
      clocks[id] = nil
    end
  end
  local shown = M.current:get()
  if shown > 0 and not open[shown] then M.current:set(0) end
  bump()
  if server then
    for _, id in ipairs(expired) do
      if server.open(id) then server.expire(id) end
    end
  end
end

-- The shell's own notifications (a timer that ran out) are numbered down
-- from -1, so they never meet an id the daemon hands out.
local own_id = 0

--- A notification from the shell itself: `{ summary, body, app, urgency }`.
--- It goes straight into the list, as if it had arrived over D-Bus; asking
--- the bus would mean calling the daemon this very process is.
function M.post(fields)
  own_id = own_id - 1
  local entry = {
    id = own_id,
    app = fields.app or "impasto",
    icon = fields.icon or "",
    summary = fields.summary or "",
    body = fields.body or "",
    actions = {},
    hints = {},
    urgency = fields.urgency or 1,
    image_path = "",
    category = "",
    desktop_entry = "",
    timeout_ms = -1,
  }
  entries[entry.id] = entry
  table.insert(history, 1, entry)
  local expired = trim()
  present(entry)
  bump()
  if server then
    for _, id in ipairs(expired) do
      if server.open(id) then server.expire(id) end
    end
  end
  return entry.id
end

--- Takes the notification off the island, without answering it.
function M.dismiss() M.current:set(0) end

--- The person closed it: off the island, and the application is told.
function M.close()
  local id = M.current:get()
  if id > 0 and server then server.dismiss(id) end
  M.dismiss()
end

function M.remove(id)
  for index, entry in ipairs(history) do
    if entry.id == id then table.remove(history, index) break end
  end
  entries[id] = nil
  if server then server.dismiss(id) end
  if M.current:get() == id then M.dismiss() end
  bump()
end

function M.clear()
  for _, entry in ipairs(history) do
    if server then server.dismiss(entry.id) end
  end
  history = {}
  entries = {}
  M.dismiss()
  bump()
end

--- An action pressed on a notification: the application hears it, and the
--- notification closes unless it asked to stay (resident).
function M.invoke(id, key)
  if not server or not server.open(id) then return false end
  return server.invoke(id, key)
end

function M.toggle_dnd()
  local silence = not settings.doNotDisturb
  settings.set("doNotDisturb", silence)
  if silence then M.dismiss() end
end

--- Starts the daemon. Queues for the bus name when another daemon has it.
function M.start()
  if server then return server end
  local started, why = daemon.serve {
    replace = false,
    -- Every clock is this file's (`expire_later`), which knows the settings'
    -- timeout and the cap; the library only keeps the open list.
    expire = false,
    default_timeout_ms = 0,
    capabilities = { "body", "body-markup", "icon-static" },
    on_change = on_change,
  }
  if not started then
    morf.log("warn", "impasto: not the notification daemon: " .. tostring(why))
    return nil
  end
  server = started
  return server
end

-- `morf ipc call notifications.server`: the screen whose runtime is the
-- notification daemon, and how many notifications it holds. Only that
-- runtime answers.
morf.ipc["notifications.server"] = function()
  if not server then return nil end
  return ((morf.screens or {})[1] or {}).name or "", #history
end

return M
