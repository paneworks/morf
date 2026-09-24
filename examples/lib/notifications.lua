-- A notification server, in the configuration.
--
-- `org.freedesktop.Notifications` is a name plus four methods and two
-- signals, and every desktop's notification popup is one of these. The engine
-- gives a configuration the bus name through `morf.dbus.serve`; what a
-- notification *is* -- its id, its timeout, which one replaces which -- is
-- policy, and policy lives here, in Lua, where a shell can change it.
--
-- What comes out is a list the shell draws however it likes. Each entry:
--   id, app, icon, summary, body, actions (list of {key, label}),
--   urgency (0 low, 1 normal, 2 critical), timeout_ms (0 = never), hints,
--   image_path (a file path or URI, "" if none), image_data (nil, or
--   { width, height, rowstride, has_alpha, bits_per_sample, channels, data }
--   with `data` the raw pixel bytes as a list), image_source (nil, or the
--   raw image as a source `ui.Image` draws, from `morf.image.from_dbus`;
--   released when the notification goes), category, desktop_entry,
--   resident, transient
-- and two verbs on the server: `dismiss(id)` when the person closed it and
-- `invoke(id, key)` when they pressed an action, both of which tell the
-- application through the signals it is waiting on.

local morf = require("morf")

local notifications = {}

local BUS_NAME = "org.freedesktop.Notifications"
local PATH = "/org/freedesktop/Notifications"
local INTERFACE = "org.freedesktop.Notifications"

-- Why a notification went away, in the protocol's own numbering.
local EXPIRED, DISMISSED, CLOSED_BY_APP, UNDEFINED = 1, 2, 3, 4

-- A D-Bus `u`. Ids and reasons are unsigned on the wire, and a caller that
-- asked for `u` and is handed an `x` rejects the reply; the engine cannot
-- guess which a Lua integer meant, so the library says.
local function u(value) return { signature = "u", value = value } end

--- Starts serving. `on_change(list)` is called whenever the list changes.
---
--- Returns the server, or nil and a reason -- usually that another daemon
--- holds the name and would not give it up. `replace` (default true) asks it
--- to; a shell that cannot restart without the user killing the old daemon is
--- a shell nobody restarts.
function notifications.serve(options)
  options = options or {}
  local service, outcome = morf.dbus.serve("session", BUS_NAME, PATH,
    options.replace ~= false)
  if outcome ~= "owned" then
    return nil, "the notification name is " .. outcome
  end

  local server = {
    list = {},
    next_id = 1,
    default_timeout_ms = options.default_timeout_ms or 5000,
    on_change = options.on_change or function() end,
  }
  local by_id = {}

  local function changed()
    server.on_change(server.list)
  end

  local function remove(id, reason)
    local entry = by_id[id]
    if not entry then return false end
    by_id[id] = nil
    if entry.image_source then morf.image.release(entry.image_source) end
    for index, candidate in ipairs(server.list) do
      if candidate.id == id then
        table.remove(server.list, index)
        break
      end
    end
    -- The application is told why, which is how a mail client knows whether
    -- to mark the message read (dismissed) or leave it (expired).
    service:emit(PATH, INTERFACE, "NotificationClosed", { u(id), u(reason) })
    changed()
    return true
  end

  --- Reads the pairs libnotify sends as `actions`: key, label, key, label.
  local function pair_up(flat)
    local actions = {}
    for index = 1, #flat - 1, 2 do
      actions[#actions + 1] = { key = flat[index], label = flat[index + 1] }
    end
    return actions
  end

  local function urgency_of(hints)
    local value = hints and hints.urgency
    if type(value) == "number" then return value end
    return 1
  end

  -- The spec renamed its image hints twice; senders still use all three
  -- spellings, newest first here.
  local function hint(hints, ...)
    for index = 1, select("#", ...) do
      local value = hints[select(index, ...)]
      if value ~= nil then return value end
    end
  end

  --- The picture a notification carries. A raw image (`image-data`, an
  --- `(iiibiiay)` struct) wins over a path, which wins over the app icon
  --- when that icon is itself a path -- the spec's own order.
  local function image_of(hints, icon)
    local data = hint(hints, "image-data", "image_data", "icon_data")
    local image_data
    if type(data) == "table" and #data >= 7 then
      image_data = {
        width = data[1], height = data[2], rowstride = data[3], has_alpha = data[4] == true,
        bits_per_sample = data[5], channels = data[6], data = data[7],
      }
    end
    local path = hint(hints, "image-path", "image_path")
    if type(path) ~= "string" or path == "" then
      path = (type(icon) == "string" and (icon:sub(1, 1) == "/" or icon:find("^file://")))
        and icon or ""
    end
    return path, image_data
  end

  service:on_call(function(call)
    local m = call.member
    if m == "GetServerInformation" then
      -- Name, vendor, version, spec version. What `notify-send` asks first.
      service:reply(call.id, { "morf", "morf", morf.version or "0", "1.2" })
    elseif m == "GetCapabilities" then
      -- Only what this shell will honour. Claiming `sound` and then being
      -- silent is worse than not claiming it.
      -- One argument of type `as`, not four strings.
      service:reply(call.id, {
        signature = "as", value = { "body", "actions", "persistence", "body-markup" },
      })
    elseif m == "Notify" then
      local a = call.arguments
      local app, replaces, icon, summary, body, actions, hints, timeout =
        a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8]
      local id = replaces
      if not id or id == 0 or not by_id[id] then
        id = server.next_id
        server.next_id = server.next_id + 1
      end
      hints = hints or {}
      local image_path, image_data = image_of(hints, icon)
      -- One name per id: a replacement's picture releases the one before.
      local image_source
      if image_data and morf.image and morf.image.from_dbus then
        image_source = morf.image.from_dbus(image_data, { name = "notification-" .. id })
      elseif by_id[id] and by_id[id].image_source then
        morf.image.release(by_id[id].image_source)
      end
      local entry = {
        id = id,
        app = app or "",
        icon = icon or "",
        summary = summary or "",
        body = body or "",
        actions = pair_up(actions or {}),
        hints = hints,
        urgency = urgency_of(hints),
        image_path = image_path,
        image_data = image_data,
        image_source = image_source,
        category = type(hints.category) == "string" and hints.category or "",
        desktop_entry = type(hints["desktop-entry"]) == "string" and hints["desktop-entry"] or "",
        -- Resident: stays after an action is pressed (a music player's
        -- controls). Transient: not to be kept in a history.
        resident = hints.resident == true,
        transient = hints.transient == true,
        -- -1 means "you decide"; 0 means "never"; anything else is theirs.
        timeout_ms = (timeout == nil or timeout < 0) and server.default_timeout_ms or timeout,
      }
      if by_id[id] then
        -- A replacement keeps its place in the list: a progress notification
        -- that jumped to the top on every update would be unreadable.
        for index, candidate in ipairs(server.list) do
          if candidate.id == id then server.list[index] = entry end
        end
      else
        table.insert(server.list, 1, entry)
      end
      by_id[id] = entry
      -- Critical never expires on its own; that is what critical means.
      if entry.timeout_ms > 0 and entry.urgency < 2 then
        morf.timer(entry.timeout_ms, function()
          if by_id[id] == entry then remove(id, EXPIRED) end
        end, false)
      end
      service:reply(call.id, u(id))
      changed()
    elseif m == "CloseNotification" then
      local id = call.arguments[1]
      if by_id[id] then
        remove(id, CLOSED_BY_APP)
        service:reply(call.id, nil)
      else
        service:reply_error(call.id, "org.freedesktop.Notifications.Error.InvalidId",
          "no notification " .. tostring(id))
      end
    else
      service:reply_error(call.id, "org.freedesktop.DBus.Error.UnknownMethod",
        "no method " .. tostring(m) .. " on " .. INTERFACE)
    end
  end)

  --- The person closed it.
  function server.dismiss(id)
    return remove(id, DISMISSED)
  end

  --- The person pressed an action. Tells the application, then closes --
  --- unless the notification is resident, which is the sender asking for
  --- it to stay.
  function server.invoke(id, key)
    local entry = by_id[id]
    if not entry then return false end
    service:emit(PATH, INTERFACE, "ActionInvoked", { u(id), key })
    if entry.resident then return true end
    return remove(id, DISMISSED)
  end

  --- Stops serving and lets whoever is queued take the name.
  function server.close()
    for id in pairs(by_id) do remove(id, UNDEFINED) end
    service:close()
  end

  return server
end

return notifications
