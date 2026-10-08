local test=morf.test

test.it("greeter offers a pattern only for the selected enrolled account",function()
  test.load("../greet/init.lua",{size={400,900},env={CAELESTIA_DRY_RUN="1",CAELESTIA_STYLE="tsugumori"},source=[[
    package.loaded["lib.services.accounts"]={list=function() return {
      {name="owner",label="Owner",initial="O"},
      {name="visitor",label="Visitor",initial="V"},
    } end}
    package.loaded["lib.services.sessions"]={list=function() return {{name="Fixture",command={"false"}}} end,
      default_index=function() return 1 end}
    package.loaded["lib.services.keyboards"]={attached=function() return false end}
    local read,exists=morf.fs.read,morf.fs.exists
    morf.fs.read=function(path)
      if path=="/etc/pam.d/greetd" then return "auth sufficient pam_exec.so morf-pattern-check --check" end
      return read(path)
    end
    morf.fs.exists=function(path)
      if path:find("/etc/morf/pattern/",1,true)==1 then return path=="/etc/morf/pattern/owner" end
      return exists(path)
    end
    local ctx
    local theme=require("themes").current.greet local build=require(theme)
    package.loaded[theme]=function(context) ctx=context return build(context) end
    require("init")
    morf.ipc.open=function() ctx.open_sheet() end
    morf.ipc.choose=function(index) ctx.choose(tonumber(index)) end
    morf.ipc.draft=function() ctx.type_text("disposable") end
    morf.ipc.audit=function() return {user=ctx.person().name,has_pattern=ctx.has_pattern(),
      method=ctx.method:get(),typed=ctx.typed:get()} end
  ]]})
  test.advance(1200)
  test.ipc("open") test.advance(700)
  test.eq(test.ipc("audit"),{user="owner",has_pattern=true,method="pattern",typed=0})
  test.truthy(test.get("greet-method").visible)
  test.ipc("choose","2") test.advance(100)
  test.eq(test.ipc("audit"),{user="visitor",has_pattern=false,method="password",typed=0})
  test.ipc("choose","1") test.advance(100)
  test.eq(test.ipc("audit").method,"pattern")
  test.ipc("draft") test.advance(100)
  test.eq(test.ipc("audit").method,"password")
  test.eq(test.ipc("audit").typed,10)
  test.ipc("choose","2") test.advance(100)
  test.eq(test.ipc("audit"),{user="visitor",has_pattern=false,method="password",typed=0})
  test.eq(test.logs("error"),{})
end)
