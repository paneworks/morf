local test=morf.test
test.it("a visual reload keeps bounded graph samples and sampling baselines",function()
  test.load("../shell/init.lua",{source=[[
    local sys=require("lib.services.sysinfo")
    sys.configure {history=3}
    local saved={history={cpu={10,20,30,40},["bat:BAT0:power"]={1,2,3}},
      stat={cpu={total=100,idle=50}},rc6={card0={ms=15,at=1}},
      net={eth0={rx=5,tx=2}},net_at=123,io={},io_at=123,proc={},proc_total=100}
    sys.restore_history(saved)
    morf.ipc.snapshot=sys.snapshot_history
    require("morf.ui").Item {width=10,height=10}
  ]]})
  local s=test.ipc("snapshot")
  test.eq(s.history.cpu,{20,30,40})
  test.eq(s.history["bat:BAT0:power"],{1,2,3})
  test.eq(s.stat.cpu,{total=100,idle=50})
  test.eq(s.net_at,123) test.eq(s.proc_total,100)
end)
