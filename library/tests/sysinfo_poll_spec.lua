local test=morf.test
local SOURCE=[[
  local reads, heartbeat, readings_at_beat=0,0,nil
  local fs=morf.fs
  local real_read,real_list=fs.read,fs.list
  fs.read=function(path,...)
    if path=="/sys/class/hwmon/test/name" then return "cpu_thermal" end
    if path:match("^/sys/class/hwmon/test/temp%d+_input$") then reads=reads+1 return "42000" end
    if path:match("^/sys/class/hwmon/") then return nil,"missing" end
    if path=="/proc/stat" then return "cpu 1 0 1 100 0 0 0 0\ncpu0 1 0 1 100 0 0 0 0\n" end
    return real_read(path,...)
  end
  fs.list=function(path,...)
    if path=="/sys/class/hwmon" then return {{name="test"}} end
    if path=="/sys/class/hwmon/test" then
      local rows={} for i=1,40 do rows[#rows+1]={name="temp"..i.."_input"} end return rows
    end
    if path=="/sys/class/thermal" then return {} end
    return real_list(path,...)
  end
  fs.read_async=function(paths,callback,limit)
    morf.timer(1,function()
      local values={} for i,path in ipairs(paths) do values[i]=fs.read(path,limit) or false end
      callback(true,values)
    end,false)
    return true
  end
  local sys=require("lib.services.sysinfo")
  sys.sources.temperatures:refresh()
  sys.sources.cpu:refresh()
  morf.timer(4,function() heartbeat=heartbeat+1 readings_at_beat=reads end,false)
  morf.ipc.state=function()
    return {reads=reads,heartbeat=heartbeat,at_beat=readings_at_beat,
      value=sys.sources.temperatures.value.cpu,history=sys.history("temperature"),
      error=sys.sources.temperatures.error}
  end
  require("morf.ui").Item{width=1,height=1}
]]
test.it("sensor polling yields to input/timers and keeps concurrent history intact",function()
  test.load("sysinfo_poll_spec.lua",{source=SOURCE})
  test.advance(400)
  local s=test.ipc("state")
  test.eq(s.heartbeat,1) test.truthy(s.at_beat<40)
  test.eq(s.reads,40) test.eq(s.value,42)
  test.eq(s.history,{42}) test.falsy(s.error)
  test.eq(#test.logs("error"),0)
end)

test.it("diskstats inventories disks, partitions and mapper holders without scanning sysfs attributes",function()
  test.load("sysinfo_poll_spec.lua",{source=[[
    local fs=morf.fs
    local values={
      ["/proc/diskstats"]=" 8 0 sda 1 0 10 0 1 0 20 0 0 1\n 8 1 sda1 1 0 10 0 1 0 20 0 0 1\n 253 0 dm-0 1 0 10 0 1 0 20 0 0 1\n 179 0 mmcblk0 1 0 10 0 1 0 20 0 0 1\n 179 1 mmcblk0p1 1 0 10 0 1 0 20 0 0 1\n",
      ["/sys/block/dm-0/dm/name"]="cryptroot",
    }
    fs.read=function(p) return values[p] end
    fs.exists=function(p) return p=="/sys/block/sda" or p=="/sys/block/mmcblk0"
      or p=="/sys/block/sda/sda1" or p=="/sys/block/mmcblk0/mmcblk0p1" end
    fs.lines=function(p) if p=="/proc/mounts" then return {"/dev/mapper/cryptroot / ext4 rw 0 0"} end return {} end
    fs.list=function(p)
      assert(p:match("/holders$"),"unnecessary inventory scan: "..p)
      if p=="/sys/block/sda/sda1/holders" then return {{name="dm-0"}} end
      return {}
    end
    morf.ipc.sample=function() return require("lib.services.sysinfo").sample("drives") end
    require("morf.ui").Item{width=1,height=1}
  ]]})
  local drives=test.ipc("sample").drives
  test.eq(#drives,2)
  test.eq(drives[1].name,"mmcblk0") test.eq(drives[1].units[1].name,"mmcblk0p1")
  test.eq(drives[2].name,"sda") test.truthy(drives[2].system)
  test.eq(drives[2].units[1].name,"sda1") test.eq(drives[2].units[2].name,"dm-0")
  test.eq(#test.logs("error"),0)
end)
