-- A settings tree: routes are stable keys, Back follows parents, and the
-- breadcrumb is derived from the same registry as the page builders. The
-- pages push and pop on a kit Navigation stack (crates/morf-kit): the stack
-- is always the route's ancestry, so Back, Alt+Left and a mouse's back
-- button all return one level.
local morf=require("morf")
local control=require("lib.kit.control")
local M={}
local made=0
function M.new(pages, selected, aliases)
  local by_key={}
  for _,page in ipairs(pages) do
    assert(type(page.key)=="string" and page.key~="" and not by_key[page.key],"Duplicate settings route")
    by_key[page.key]=page
  end
  for _,page in ipairs(pages) do
    local seen={[page.key]=true}
    local parent=page.parent or ""
    while parent~="" do
      assert(by_key[parent],"Unknown settings parent: "..parent)
      assert(not seen[parent],"Settings parent cycle")
      seen[parent]=true parent=by_key[parent].parent or ""
    end
  end
  local tree={pages=pages}
  function tree.request(key)
    key=(aliases or {})[key] or key
    if key~="" and not by_key[key] then return false end
    selected:set(key) if tree.follow then tree.follow() end return true
  end
  function tree.path(key)
    local path={}
    while by_key[key] do table.insert(path,1,by_key[key]) key=by_key[key].parent or "" end
    return path
  end
  -- The stack, and the page it shows, kept in Lua so following the signal
  -- reads nothing reactive of the stack's own.
  local at, syncing="", false
  local nav=control.headless("Navigation",{mode="stack",current="",
    on_current_changed=function(key)
      at=key
      if not syncing and selected:get()~=key then selected:set(key) end
    end})
  local function sync(key)
    if key==at then return end
    syncing=true
    nav.send("go","")
    for _,page in ipairs(tree.path(key)) do nav.send("push",page.key) end
    syncing=false
  end
  made=made+1
  morf.effect("lib.settings_pages."..made,function() sync(selected:get()) end)
  -- Effects run later: every call first brings the stack to the route.
  function tree.follow() sync(selected:get()) end
  function tree.back()
    tree.follow()
    if at=="" then return tree.request("") end
    nav.send("pop")
    return true
  end
  --- True while there is a page to go back to.
  function tree.can_go_back() tree.follow() return at~="" end
  --- The stack's depth: 1 at the root.
  function tree.depth() tree.follow() return nav.t.depth end
  --- A key for the stack (Alt+Left, XF86Back); true when it went back.
  function tree.key(name,modifiers) tree.follow() return nav.key(name,modifiers) end
  --- Shortcuts for the node the pages show in: Alt+Left and the back
  --- button pop, and pass on at the root.
  function tree.shortcuts()
    local function pop() if not tree.can_go_back() then return false end tree.back() return true end
    return {["alt+Left"]=pop,["back"]=pop}
  end
  function tree.breadcrumb()
    local names={"Settings"}
    local path=tree.path(selected:get())
    for i=1,#path-1 do names[#names+1]=path[i].name end
    return table.concat(names," / ")
  end
  return tree
end
return M
