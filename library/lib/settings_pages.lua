-- A settings tree: routes are stable keys, Back follows parents, and the
-- breadcrumb is derived from the same registry as the page builders.
local M={}
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
    selected:set(key) return true
  end
  function tree.back()
    local page=by_key[selected:get()]
    return tree.request(page and page.parent or "")
  end
  function tree.path(key)
    local path={}
    while by_key[key] do table.insert(path,1,by_key[key]) key=by_key[key].parent or "" end
    return path
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
