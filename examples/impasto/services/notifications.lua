-- The shell is the notification daemon.
--
-- Port of NotificationService.qml over examples/lib/notifications.lua. One
-- notification at a time is `current` -- the island shows it -- and the last
-- fifty are kept as history for the notifications module. A critical one
-- never times out and is not replaced by an ordinary one; with Do not
-- disturb on, only critical ones reach the island, though all are kept.
--
-- The name is queued for, not taken: a shell that starts beside another
-- daemon waits its turn instead of stealing it.

local daemon = require("lib.notifications")
local settings = require("services.settings")

local M = {}

M.HISTORY_LIMIT = 50
M.current = morf.signal("impasto.notify.current", 0)     -- id, 0 for none
M.revision = morf.signal("impasto.notify.revision", 0)   -- bumps on any change

local entries = {}      -- id -> entry (live or kept)
local history = {}      -- newest first
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
  local mine = current_timer
  local timeout = timeout_for(entry)
  if timeout > 0 then
    morf.timer(timeout, function()
      if mine == current_timer and M.current:get() == entry.id then M.dismiss() end
    end, false)
  end
end

local function on_change(list)
  local live = {}
  for _, entry in ipairs(list) do
    live[entry.id] = true
    local known = entries[entry.id]
    entries[entry.id] = entry
    if not known then
      table.insert(history, 1, entry)
      while #history > M.HISTORY_LIMIT do
        local gone = table.remove(history)
        entries[gone.id] = nil
      end
      present(entry)
    elseif known ~= entry then
      -- A replacement (same id): keep its place in history, show it again.
      for index, kept in ipairs(history) do
        if kept.id == entry.id then history[index] = entry end
      end
      if M.current:get() == entry.id then present(entry) end
    end
  end
  -- Closed by its application: off the island, kept in history.
  local shown = M.current:get()
  if shown ~= 0 and not live[shown] and entries[shown] and entries[shown].closed_by_app then
    M.current:set(0)
  end
  bump()
end

--- Takes the notification off the island, without answering it.
function M.dismiss() M.current:set(0) end

--- The person closed it: off the island, and the application is told.
function M.close()
  local id = M.current:get()
  if id ~= 0 and server then server.dismiss(id) end
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
    -- The library's own expiry takes a notification out of its list; the
    -- island's timer and the history here decide what the person sees, so
    -- the list keeps them until they are dismissed or closed by the app.
    default_timeout_ms = 0,
    on_change = on_change,
  }
  if not started then
    morf.log("warn", "impasto: not the notification daemon: " .. tostring(why))
    return nil
  end
  server = started
  return server
end

return M
