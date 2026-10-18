-- Keep file decoding and SVG rasterization off the gesture/render thread.
local morf=require("morf")
local M={}
local serial=0
function M.new()
  serial=serial+1
  local state=morf.state {source=""}
  local current,key,effect
  local revision=0
  local closed=false
  local function retire()
    revision=revision+1
    state.source=""
    if current then current.close() current=nil end
  end
  local function sync()
    if closed then return end
    local source=require("wallpaper").current:get()
    local screen=(morf.screens or {})[1]
    local _,_,w,h=require("bar").desk()
    w=math.max(1,math.ceil(screen and screen.width or w))
    h=math.max(1,math.ceil(screen and screen.height or h))
    local scale=math.max(1,math.floor((screen and screen.density_scale or 1)*120+.5))
    local next_key=source.."\n"..w.."x"..h.."@"..scale
    if key==next_key then return end
    key=next_key retire()
    if source=="" then return end
    local ok,preview=pcall(morf.image.preview,source)
    if not ok or not preview then return end
    current=preview
    local token=revision
    local queued,accepted=pcall(preview.cover,w,h,scale,function(success,result)
      if closed or token~=revision then return end
      if success and result and result.path then
        state.source=result.path
      else
        retire()
      end
    end)
    if not queued or not accepted then retire() end
  end
  effect=morf.effect("phone.workspace.wallpaper."..serial,sync)
  return {
    source=function() return state.source end,
    sync=sync,
    close=function()
      if closed then return end
      closed=true
      effect:dispose()
      retire()
    end,
  }
end
return M
