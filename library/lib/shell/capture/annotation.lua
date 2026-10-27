-- Theme-independent screenshot document. Coordinates are physical image pixels.
-- History shares immutable annotations; neither screenshots nor drafts are
-- duplicated into each undo entry. No shell, compositor or file access here.
local M={}
local image=require("morf").image
assert(image.annotation_bounds,"native annotations require the updated Morf engine")
M.tools={
  {"select","Select","v","touch_app"},{"rect","Rectangle","r","rectangle"},
  {"ellipse","Ellipse","o","circle"},{"line","Line","l","horizontal_rule"},
  {"arrow","Arrow","a","arrow_forward"},{"pen","Pen","p","draw"},
  {"marker","Highlight","h","ink_highlighter"},{"text","Text","t","title"},
  {"step","Steps","n","format_list_numbered"},{"blur","Blur","b","blur_on"},
  {"pixelate","Pixelate","x","grid_on"},{"zoom","Zoom","z","zoom_in"},
}
local valid={}
for _,tool in ipairs(M.tools) do valid[tool[1]]=true end
local function copy(t) local out={} for k,v in pairs(t or {}) do out[k]=v end return out end
local function clamp(n,a,b) return math.max(a,math.min(b,n)) end
M.bounds=image.annotation_bounds
M.hit=image.annotation_hit
local function shifted(a,dx,dy)
  local next=copy(a) next.points={}
  for _,p in ipairs(a.points) do next.points[#next.points+1]={x=p.x+dx,y=p.y+dy} end
  return next
end
function M.new(width,height,options)
  options=options or {}
  assert(width>0 and height>0 and width<=16384 and height<=16384,"invalid image size")
  local d={width=width,height=height,items={},crop={x=0,y=0,w=width,h=height},tool="select",
    styles={},undo_stack={},redo_stack={},revision=0,selected=nil,draft=nil}
  for _,tool in ipairs(M.tools) do
    local style=copy((options.styles or {})[tool[1]])
    style.color=tostring(style.color or "#ef5350")
    if not style.color:match("^#%x%x%x%x%x%x$") then style.color="#ef5350" end
    style.width=clamp(tonumber(style.width) or (tool[1]=="text" and 24 or 4),1,128)
    style.filled=style.filled==true d.styles[tool[1]]=style
  end
  function d.changed() d.revision=d.revision+1 if options.changed then options.changed(d) end end
  function d.remember()
    d.undo_stack[#d.undo_stack+1]={items=copy(d.items),crop=copy(d.crop)}
    if #d.undo_stack>100 then table.remove(d.undo_stack,1) end
    d.redo_stack={}
  end
  function d.choose(tool) assert(valid[tool],"unknown annotation tool") d.draft=nil d.moving=nil d.origin=nil d.preview_crop=nil d.tool=tool d.selected=nil d.changed() end
  function d.style(key,value)
    local item=d.tool=="select" and not d.origin and d.items[d.selected]
    local s=d.styles[item and item.type or d.tool]
    if key=="color" then assert(tostring(value):match("^#%x%x%x%x%x%x$"),"use a hex colour")
    elseif key=="width" then value=clamp(tonumber(value) or 4,1,128)
    elseif key=="filled" then value=value==true else error("unknown tool style") end
    if item and item[key]~=value then
      d.remember()
      local next=copy(item) next[key]=value d.items[d.selected]=next
    end
    s[key]=value
    if d.draft then d.draft[key]=value end
    if options.style_changed then options.style_changed(item and item.type or d.tool,copy(s)) end
    d.changed()
  end
  function d.set_crop(x,y,w,h,remember)
    x,y=clamp(math.floor(x),0,width-1),clamp(math.floor(y),0,height-1)
    if remember~=false then d.remember() end
    d.crop={x=x,y=y,w=clamp(math.floor(w),1,width-x),h=clamp(math.floor(h),1,height-y)} d.changed()
  end
  function d.hit(x,y,tolerance)
    return image.annotation_pick(d.items,x,y,tolerance)
  end
  function d.add(a)
    assert(#d.items<128,"annotation limit reached (128)")
    d.remember() d.items[#d.items+1]=a d.selected=#d.items d.changed()
  end
  function d.remove()
    if not d.selected then return false end
    d.remember() table.remove(d.items,d.selected) d.selected=nil d.draft=nil d.moving=nil d.origin=nil d.changed() return true
  end
  function d.undo()
    local old=table.remove(d.undo_stack) if not old then return false end
    d.redo_stack[#d.redo_stack+1]={items=copy(d.items),crop=copy(d.crop)}
    d.items,d.crop=old.items,old.crop d.draft=nil d.moving=nil d.origin=nil d.preview_crop=nil d.selected=nil d.changed() return true
  end
  function d.redo()
    local next=table.remove(d.redo_stack) if not next then return false end
    d.undo_stack[#d.undo_stack+1]={items=copy(d.items),crop=copy(d.crop)}
    d.items,d.crop=next.items,next.crop d.draft=nil d.moving=nil d.origin=nil d.preview_crop=nil d.selected=nil d.changed() return true
  end
  function d.begin(x,y)
    x,y=clamp(x,0,width),clamp(y,0,height)
    if d.tool=="select" then
      d.selected=d.hit(x,y) d.origin={x=x,y=y}
      d.moving=d.selected and d.items[d.selected] or nil
      d.changed() return
    end
    local a=copy(d.styles[d.tool]) a.type=d.tool a.points={{x=x,y=y},{x=x,y=y}}
    a.strength=d.tool=="blur" and (options.blur or 24) or d.tool=="pixelate" and (options.pixelate or 14) or options.zoom or 2
    a.font=options.font or "sans-serif"
    if d.tool=="step" then
      a.number=1 for _,item in ipairs(d.items) do if item.type=="step" then a.number=math.max(a.number,item.number+1) end end
      d.add(a) return
    end
    if d.tool=="text" then a.text="" end
    d.draft=a d.changed()
  end
  function d.update(x,y)
    x,y=clamp(x,0,width),clamp(y,0,height)
    if d.moving then d.draft=shifted(d.moving,x-d.origin.x,y-d.origin.y)
    elseif d.draft and d.draft.type~="text" then
      if d.draft.type=="pen" then
        local p=d.draft.points[#d.draft.points]
        if #d.draft.points<4096 and (x-p.x)^2+(y-p.y)^2>=1 then d.draft.points[#d.draft.points+1]={x=x,y=y} end
      else d.draft.points[2]={x=x,y=y} end
    elseif d.tool=="select" and d.origin then
      d.preview_crop={x=math.min(x,d.origin.x),y=math.min(y,d.origin.y),w=math.abs(x-d.origin.x),h=math.abs(y-d.origin.y)}
    else return end
    d.changed()
  end
  function d.finish(text)
    if d.moving and d.draft then d.remember() d.items[d.selected]=d.draft
    elseif d.preview_crop and d.preview_crop.w>=2 and d.preview_crop.h>=2 then
      local b=d.preview_crop d.set_crop(b.x,b.y,b.w,b.h)
    elseif d.draft then
      if d.draft.type=="text" then
        if text==nil then return end
        d.draft.text=tostring(text):sub(1,4096)
        if d.draft.text~="" then d.add(d.draft) end
      else
        local b=M.bounds(d.draft)
        if b.w>=1 or b.h>=1 then d.add(d.draft) end
      end
    end
    d.draft=nil d.moving=nil d.origin=nil d.preview_crop=nil d.changed()
  end
  function d.abort() d.draft=nil d.moving=nil d.origin=nil d.preview_crop=nil d.changed() end
  function d.resize(handle,x,y,base)
    local b=base or d.crop local l,t,r,bot=b.x,b.y,b.x+b.w,b.y+b.h
    if handle:find("w",1,true) then l=clamp(x,0,r-1) end
    if handle:find("e",1,true) then r=clamp(x,l+1,width) end
    if handle:find("n",1,true) then t=clamp(y,0,bot-1) end
    if handle:find("s",1,true) then bot=clamp(y,t+1,height) end
    d.set_crop(l,t,r-l,bot-t,false)
  end
  return d
end

-- Ordered compositing: redactions and zoom operate on preceding annotations,
-- later marks stay crisp. Cropping is last, so moving the selection never
-- changes the coordinates of its existing annotations.
function M.ops(document,crop,include_draft,omit_moving,view)
  local ox,oy=view and view.x or 0,view and view.y or 0
  local width,height=view and view.w or document.width,view and view.h or document.height
  local ops,vectors={},{}
  local function flush()
    if #vectors>0 then
      ops[#ops+1]={"annotations",vectors,ox,oy}
      vectors={}
    end
  end
  local items=copy(document.items)
  if omit_moving and document.moving and document.draft then table.remove(items,document.selected) end
  if include_draft and document.draft then
    if document.moving then items[document.selected]=document.draft else items[#items+1]=document.draft end
  end
  for _,a in ipairs(items) do
    if a.type=="blur" or a.type=="pixelate" or a.type=="zoom" then
      flush() local b=M.bounds(a) b.x,b.y=b.x-ox,b.y-oy
      local x,y=math.max(0,math.floor(b.x)),math.max(0,math.floor(b.y))
      local right,bottom=math.min(width,math.ceil(b.x+b.w)),math.min(height,math.ceil(b.y+b.h))
      if right>x and bottom>y then
        ops[#ops+1]={a.type=="blur" and "blur_region" or a.type=="zoom" and "zoom_region" or "pixelate",x,y,right-x,bottom-y,a.strength}
      end
      if a.type=="zoom" then vectors[#vectors+1]=a end
    else vectors[#vectors+1]=a end
  end
  flush()
  if crop then local b=document.crop ops[#ops+1]={"crop",b.x,b.y,b.w,b.h} end
  return ops
end
-- Preview coordinates are local to one output; the export document stays in
-- desktop pixels. Share immutable styles and shift only the point arrays.
function M.viewport_ops(document,view,include_draft,omit_moving)
  return M.ops(document,false,include_draft,omit_moving,view)
end

return M
