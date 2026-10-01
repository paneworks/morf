local test=morf.test
local function check(name,body)
  test.it(name,function()
    test.load {source='local A=require("lib.annotation")\n'..body..'\nmorf.ipc.ok=function() return true end'}
    test.truthy(test.ipc("ok")) test.eq(#test.logs("error"),0)
  end)
end
check("all annotation tools commit and export; text is escaped",[[
  local d=A.new(800,600)
  for _,tool in ipairs(A.tools) do
    if tool[1]~="select" then
      d.choose(tool[1]) d.begin(10,10) d.update(90,80)
      d.finish(tool[1]=="text" and '<test & "escaped">' or nil)
    end
  end
  assert(#d.items==11)
  local ops=A.ops(d,true)
  local encoded=require("morf").json.encode(ops)
  if require("morf").image.annotation_path then
    assert(ops[1][1]=="annotations" and ops[1][2][7].text=='<test & "escaped">')
  else assert(encoded:find("&lt;test &amp;",1,true)) end
  assert(encoded:find("blur_region",1,true) and encoded:find("pixelate",1,true) and encoded:find("zoom_region",1,true))
  assert(ops[#ops][1]=="crop")
]])
check("moving and undo preserve immutable annotations and crop history",[[
  local d=A.new(400,300)
  d.choose("rect") d.begin(10,10) d.update(80,70) d.finish()
  local original=d.items[1]
  d.choose("select") d.begin(10,10) d.update(35,40) d.finish()
  assert(d.items[1].points[1].x==35 and original.points[1].x==10)
  d.undo() assert(d.items[1]==original) d.redo() assert(d.items[1].points[1].x==35)
  d.set_crop(20,20,200,100) d.undo() assert(d.crop.w==400) d.redo() assert(d.crop.w==200)
  d.selected=1 d.remove() assert(#d.items==0) d.undo() assert(#d.items==1)
]])
check("crop handles clamp to the image and undo is bounded",[[
  local d=A.new(400,300) d.set_crop(20,30,100,80)
  for _,handle in ipairs {"nw","n","ne","w","e","sw","s","se"} do
    local base=d.crop d.resize(handle,-100,999,base)
    assert(d.crop.x>=0 and d.crop.y>=0 and d.crop.w>=1 and d.crop.h>=1)
    assert(d.crop.x+d.crop.w<=400 and d.crop.y+d.crop.h<=300)
  end
  for n=1,150 do d.set_crop(0,0,100+n,100) end
  assert(#d.undo_stack==100)
]])
check("per-tool styles, step numbering and narrow line hit tests",[[
  local d=A.new(400,300)
  d.choose("rect") d.style("color","#abcdef") d.style("width",12) d.style("filled",true)
  d.choose("text") assert(d.styles.text.width==24)
  d.choose("rect") assert(d.styles.rect.color=="#abcdef" and d.styles.rect.filled)
  d.choose("step") d.begin(100,100) d.begin(200,200) assert(d.items[2].number==2)
  d.undo() d.begin(250,250) assert(d.items[2].number==2)
  d.choose("line") d.begin(10,10) d.update(100,100) d.finish()
  assert(d.hit(50,50)==3) assert(d.hit(20,80)~=3)
  assert(not pcall(d.style,"color",'red" onclick="bad'))
]])

check("changing tools aborts an unfinished move and redactions clip to image bounds",[[
  local d=A.new(100,100)
  d.choose("rect") d.begin(10,10) d.update(40,40) d.finish()
  d.choose("select") d.begin(10,10) d.update(20,20)
  d.choose("pen") d.begin(50,50) d.update(70,70) d.finish()
  assert(#d.items==2 and d.items[1].points[1].x==10 and d.items[2].type=="pen")
  d.items={{type="blur",points={{x=-20,y=-10},{x=20,y=30}},width=4,color="#ffffff",strength=4}}
  local ops=A.ops(d,false)
  assert(ops[1][2]==0 and ops[1][3]==0 and ops[1][4]==20 and ops[1][5]==30)
  d.items[1].points={{x=-20,y=0},{x=-10,y=30}}
  assert(#A.ops(d,false)==0)
]])

check("viewport previews shift vectors and effects without changing export coordinates",[[
  local d=A.new(400,200)
  d.choose("rect") d.begin(120,60) d.update(160,90) d.finish()
  d.choose("blur") d.begin(180,80) d.update(220,100) d.finish()
  d.choose("arrow") d.begin(130,70) d.update(150,90)
  local before=A.ops(d,false,true)
  local preview=A.viewport_ops(d,{x=100,y=50,w=200,h=120},true)
  if require("morf").image.annotation_path then
    assert(preview[1][1]=="annotations" and preview[1][3]==100 and preview[1][4]==50)
    assert(preview[1][2][1]==d.items[1]) -- No point-array copies per preview.
  else
    assert(preview[1][2]:find('width="200"',1,true))
    assert(preview[1][2]:find('x="20" y="10"',1,true))
  end
  assert(preview[2][1]=="blur_region" and preview[2][2]==80 and preview[2][3]==30)
  assert(d.items[1].points[1].x==120 and d.draft.points[1].x==130)
  local after=A.ops(d,false,true)
  assert(require("morf").json.encode(before)==require("morf").json.encode(after))
]])

check("deleting a moving annotation cancels its draft and preserves remaining artwork",[[
  local d=A.new(400,200)
  d.choose("rect") d.begin(10,10) d.update(100,100) d.finish()
  d.begin(200,10) d.update(300,100) d.finish()
  d.choose("select") d.begin(10,40) d.update(30,60)
  assert(d.moving and d.draft and d.remove())
  assert(#d.items==1 and not d.moving and not d.draft and not d.origin)
  assert(d.items[1].points[1].x==200)
  local ops=A.viewport_ops(d,{x=0,y=0,w=400,h=200},true,true)
  if ops[1][1]=="annotations" then assert(ops[1][2][1].points[1].x==200)
  else assert(ops[1][2]:find('x="200"',1,true)) end
  d.finish() assert(#d.items==1)
  assert(d.undo() and #d.items==2)
]])

check("restyling a selected mark preserves immutable history and tool defaults",[[
  local persisted={}
  local d=A.new(400,200,{style_changed=function(tool,style) persisted[tool]=style end})
  d.choose("rect") d.begin(10,10) d.update(100,100) d.finish()
  local original=d.items[1]
  d.choose("select") d.begin(10,40) d.finish()
  d.style("color","#66bb6a") d.style("width",12) d.style("filled",true)
  assert(d.items[1].color=="#66bb6a" and d.items[1].width==12 and d.items[1].filled)
  assert(original.color=="#ef5350" and original.width==4 and not original.filled)
  assert(persisted.rect.color=="#66bb6a" and not persisted.select)
  d.undo() d.undo() d.undo() assert(d.items[1]==original)
  d.redo() d.redo() d.redo() assert(d.items[1].filled)
  d.choose("rect") assert(d.styles.rect.color=="#66bb6a" and d.styles.rect.width==12)
]])
