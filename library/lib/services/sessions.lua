-- The sessions a login screen can start: the desktop entries in
-- wayland-sessions and xsessions, as greetd wants them.
--
--   local sessions = require("lib.services.sessions")
--   local list = sessions.list()
--   local pick = sessions.default_index(list)
--   door:start(list[pick])            -- lib.auth's greeter
--
-- `command` is an argument vector, not `Exec`'s command line: greetd takes a
-- vector, and handing it the line asks for a program named after the whole
-- line. Each also carries the environment it expects: a session started
-- without XDG_CURRENT_DESKTOP comes up with its portals guessing.

local morf = require("morf")
local core = require("morf.core")

local sessions = {}

--- `{ id, name, kind ("wayland" or "x11"), command, environment }`, Wayland
--- first, then by name.
function sessions.list()
  local found = {}
  local ok, entries = pcall(function() return core.desktop_entries(core.session_paths()):applications() end)
  for _, entry in ipairs(ok and entries or {}) do
    if entry.command and #entry.command > 0 then
      -- The directory it was found in is the only statement of its kind.
      local kind = tostring(entry.source or ""):match("xsessions$") and "x11" or "wayland"
      local environment = {
        "XDG_SESSION_TYPE=" .. kind,
        "XDG_SESSION_DESKTOP=" .. entry.id,
        "DESKTOP_SESSION=" .. entry.id,
      }
      if entry.desktop_names and #entry.desktop_names > 0 then
        environment[#environment + 1] = "XDG_CURRENT_DESKTOP=" .. table.concat(entry.desktop_names, ":")
      end
      found[#found + 1] = {
        id = entry.id, name = entry.name, kind = kind,
        command = entry.command, environment = environment,
      }
    end
  end
  table.sort(found, function(a, b)
    if a.kind ~= b.kind then return a.kind == "wayland" end
    return a.name:lower() < b.name:lower()
  end)
  return found
end

--- Which of `list` to open on: the one `/etc/greetd/default-session` names
--- (desktop file name or title), else the first.
function sessions.default_index(list, path)
  local ok, text = pcall(morf.fs.read, path or "/etc/greetd/default-session")
  if not ok or type(text) ~= "string" then return 1 end
  local wanted = (text:match("^%s*(.-)%s*$") or ""):lower()
  for index, entry in ipairs(list) do
    if entry.id:lower() == wanted or entry.name:lower() == wanted then return index end
  end
  return 1
end

return sessions
