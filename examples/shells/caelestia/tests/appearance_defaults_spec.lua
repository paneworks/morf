-- System defaults must work for the greeter's empty home without overriding
-- a person's explicit theme selection. No PAM or greetd is involved.
local test=morf.test
for _,case in ipairs {
  {name="greeter inherits installed skin",installed="tsugumori",expected="tsugumori"},
  {name="personal selection wins",installed="tsugumori",personal="material",expected="material"},
  {name="environment preview wins",installed="material",preview="tsugumori",expected="tsugumori"},
  {name="source without installed defaults stays Material",expected="material"},
} do
  test.it(case.name,function()
    test.load("../greet/init.lua",{env={CAELESTIA_STYLE=case.preview or false,
      TEST_DEFAULT=case.installed or "",TEST_PERSONAL=case.personal or "",
      CAELESTIA_APPEARANCE=morf.env("XDG_CACHE_HOME").."/auth-appearance-defaults-test.json"},source=[[
      local read=morf.fs.read
      morf.fs.read=function(path)
        if path==morf.config_path("appearance-default.json") then
          if morf.env("TEST_DEFAULT")=="" then error("No installed default") end
          return morf.json.encode({theme=morf.env("TEST_DEFAULT"),font=""})
        end
        if path==morf.env("CAELESTIA_APPEARANCE") then
          if morf.env("TEST_PERSONAL")=="" then error("No personal selection") end
          return morf.json.encode({theme=morf.env("TEST_PERSONAL")})
        end
        return read(path)
      end
      local themes=require("themes")
      morf.ipc.appearance=function() return themes.current.id end
      require("morf.ui").Item {width=100,height=100}
    ]]})
    test.eq(test.ipc("appearance"),case.expected)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
