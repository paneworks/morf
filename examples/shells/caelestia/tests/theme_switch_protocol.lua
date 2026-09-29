-- Deterministic multi-output protocol test. Run: lua tests/theme_switch_protocol.lua
local root=arg[0]:match("^(.*)/tests/") or "."
local now, queue, timers, workers, reloads=0,{},{},{},0
local function copy(t)
  if type(t)~="table" then return t end
  local out={} for k,v in pairs(t) do out[k]=copy(v) end return out
end
local function signal(value)
  return {get=function() return value end,set=function(_,v) value=v end}
end
local function make(index, bank)
  local modules, worker={}, {reason=nil, bank=bank or signal({})}
  local m={screens={{name="output"..index},{name="output"..(3-index)}},ipc={},time={now_ms=function() return now end}}
  m.signal=function(_,v) return signal(v) end
  m.reloadable=function() return worker.bank end
  m.primary=function() return index==1 end
  m.timer=function(ms,fn)
    local timer={at=now+ms,fn=fn,cancel=function(self) self.cancelled=true end,worker=worker}
    timers[#timers+1]=timer return timer
  end
  m.animation={play=function() return {stop=function() end} end}
  m.broadcast=function(...) queue[#queue+1]={...} return true end
  m.reload=function() reloads=reloads+1 end
  m.effect=function() end
  m.on_reload_completed=function(fn) worker.complete=fn end
  m.on_reload_failed=function(fn) worker.fail=fn end
  modules.morf=m
  modules["morf.ui"]={}
  modules.themes={current={id=worker.bank:get().target or "material"},font=worker.bank:get().font or "",
    preferences={set=function(key,v) if key=="theme" then worker.saved=v else worker.saved_font=v end end,flush=function() return true end}}
  local env=setmetatable({},{__index=_G})
  env.require=function(name)
    if not modules[name] then modules[name]=assert(loadfile(root.."/"..name:gsub("%.","/")..".lua","t",env))() end
    return modules[name]
  end
  worker.session=env.require("themes.session")
  worker.draft=worker.session.keep("draft","")
  worker.switch=env.require("themes.switcher")
  worker.switch.start(function() return worker.reason end)
  return worker
end
local function drain()
  while #queue>0 do
    local event=table.remove(queue,1)
    for _,worker in ipairs(workers) do worker.switch.receive(table.unpack(event,2)) end
  end
end
local function advance(ms)
  local until_at=now+ms
  while true do
    drain()
    table.sort(timers,function(a,b) return a.at<b.at end)
    if not timers[1] or timers[1].at>until_at then break end
    local timer=table.remove(timers,1) now=timer.at
    if not timer.cancelled and not timer.worker.dead then timer.fn() end
  end
  now=until_at drain()
end
local function reset()
  now,queue,timers,reloads=0,{},{},0
  workers={make(1),make(2)}
end
reset()
workers[1].draft:set("unsaved task") workers[2].draft:set("second screen")
assert(workers[2].switch.request("tsugumori"))
drain()
assert(workers[1].switch.busy:get() and workers[2].switch.busy:get())
assert(not workers[1].switch.request("material"))
advance(239) assert(reloads==0)
advance(1) assert(reloads==1,"more than one global reload")
for i,worker in ipairs(workers) do
  worker.dead=true
  workers[i]=make(i,signal(copy(worker.bank:get())))
  workers[i].complete()
end
advance(600)
assert(workers[1].draft:get()=="unsaved task" and workers[2].draft:get()=="second screen")
assert(workers[1].saved=="tsugumori" and not workers[2].saved)
assert(not workers[1].switch.busy:get() and not workers[2].switch.busy:get())
assert(not workers[1].bank:get().switching and workers[1].bank:get().draft==nil)
assert(workers[1].switch.request("material")) advance(300) assert(reloads==2)
workers[1].fail() advance(600)
assert(not workers[1].switch.busy:get() and not workers[2].switch.busy:get())
assert(workers[1].bank:get().target=="tsugumori","failed switch changed current theme")
reset() workers[1].reason="Finish authentication"
assert(workers[2].switch.request("tsugumori")) advance(1000)
assert(reloads==0 and not workers[2].switch.busy:get(),"busy primary did not cancel other screen")
reset() workers[2].switch.request("tsugumori") drain()
workers[2].reason="Capture started" advance(1000)
assert(reloads==0 and not workers[1].switch.busy:get())
reset() workers[2].switch.request("invalid") advance(1000) assert(reloads==0)
reset() workers[1].switch.request("tsugumori") workers[2].switch.request("tsugumori") advance(300)
assert(reloads==1,"simultaneous requests raced")
reset() workers[1].switch.request("tsugumori") drain() workers[2].dead=true
advance(7000) assert(reloads==0 and not workers[1].switch.busy:get(),"missing screen stranded cover")
print("ok: coordinated reload, state handoff, repeat clicks, busy guards, failure and timeout")
reset()
workers[2].draft:set("unfinished text")
assert(workers[2].switch.request_font("Goku")) advance(300)
assert(reloads==1)
for i,worker in ipairs(workers) do
  assert(worker.bank:get().font=="Goku" and worker.bank:get().target=="material")
  worker.dead=true
  workers[i]=make(i,signal(copy(worker.bank:get())))
  workers[i].complete()
end
advance(600)
assert(workers[1].saved_font=="Goku" and not workers[2].saved_font)
assert(workers[2].draft:get()=="unfinished text")
assert(not workers[1].switch.request_font("Goku"))
assert(workers[1].switch.request("tsugumori")) advance(300)
assert(workers[1].bank:get().font=="Goku","theme change discarded chosen font")
workers[1].fail() advance(600)
assert(workers[1].bank:get().font=="Goku","failed change lost chosen font")
assert(workers[1].switch.request_font("")) advance(300)
assert(workers[1].bank:get().font=="","default font was not handed off")
reset() workers[1].reason="Finish authentication"
assert(workers[2].switch.request_font("Goku")) advance(1000)
assert(reloads==0,"font change bypassed authentication guard")
assert(not workers[1].switch.request_font("bad\nfont"))
print("ok: font selection, persistence, reset, theme independence and guarded handoff")
