-- GCR keyring prompts. The native bridge owns the protocol and encryption;
-- this module transports UI metadata and local answers over private pipes.
-- Request.endpoint lets another output answer through the native bridge's
-- private Unix socket without passing passwords through shell broadcasts.
local morf = require("morf")
local M = {}
function M.serve(options)
  options = options or {}
  local helper = options.helper or require("lib.util.poll").which("morf-keyring")
  if not helper then return nil, "morf-keyring is not installed" end
  local agent = { current = nil, names = {}, stopped = false }
  local child
  local function closed()
    if agent.current then
      local old = agent.current agent.current = nil
      if options.on_close then options.on_close(old) end
    end
  end
  local function send(value)
    if agent.stopped or not child then return false end
    local ok = pcall(function() child:write(morf.json.encode(value) .. "\n") end)
    if not ok then closed() end
    return ok
  end
  local function receive(line)
    local ok, event = pcall(morf.json.decode, line)
    if not ok or type(event) ~= "table" then return end
    if event.event == "status" then
      agent.names = event.names or {}
      if options.on_status then options.on_status(agent.names) end
    elseif event.event == "prompt" and type(event.id) == "number"
      and (event.kind == "password" or event.kind == "confirm") then
      closed()
      local request = { id = event.id, kind = event.kind, properties = event.properties or {}, endpoint = event.endpoint }
      local answered = false
      function request.answer(password, choice)
        if agent.current ~= request or answered then return false end
        answered = true
        return send { id = request.id, action = "continue", password = password, choice = choice == true }
      end
      function request.cancel()
        if agent.current ~= request or answered then return false end
        answered = true
        return send { id = request.id, action = "cancel" }
      end
      agent.current = request
      if options.on_request then options.on_request(request) else request.cancel() end
    elseif event.event == "close" and agent.current and event.id == agent.current.id then
      closed()
    end
  end
  child = morf.spawn {
    command = { helper }, env = { LD_LIBRARY_PATH = "", G_MESSAGES_DEBUG = "",
      DBUS_SESSION_BUS_ADDRESS = morf.env("DBUS_SESSION_BUS_ADDRESS") },
    stdin = "pipe", lines = true, max_line = 65536,
    on_stdout = receive,
    -- No helper output is copied into the UI or logs: a password never belongs there.
    on_stderr = function() end,
    on_exit = function()
      agent.stopped = true agent.names = {} closed()
      if options.on_status then options.on_status({}) end
      if options.on_exit then options.on_exit() end
    end,
  }
  function agent.close()
    if agent.stopped then return end
    if agent.current then agent.current.cancel() end
    agent.stopped = true closed()
    child:close_stdin()
  end
  return agent
end
return M
