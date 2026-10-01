-- Compatibility lookup; canonical outlines live in the native geometry module.
local geometry=require("morf").geometry
local paths={}
for _,name in ipairs(geometry.shape_names) do paths[name]=geometry.shape_path(name) end
return paths
