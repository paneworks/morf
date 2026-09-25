-- The active palette: the wallpaper's own, or one of the fixed schemes.
--
-- Port of ThemeService.qml over examples/lib/palette.lua (which does what
-- theme_manager.py did). "adaptive" derives the palette from whatever
-- picture is up and derives it again when the picture changes; any other
-- id is one of the nine fixed schemes. A change blends from the old
-- palette to the new one over a quarter of a second, as the original's
-- colour behaviours did.
--
-- The original also rewrote kitty, btop, cava, GTK and other programs'
-- colours. That writes into the user's own dotfiles, so here it happens
-- only with `writeAppThemes` on, and only into the files it names.

local palette = require("lib.palette")
local theme = require("theme")
local settings = require("services.settings")
local wallpaper = require("services.wallpaper")

local M = {}

M.TRANSITION_MS = 260
M.STEPS = 8
M.active_id = morf.signal("impasto.theme.active", settings.theme)
M.derived = nil          -- the adaptive palette, once there is one
M.current = nil          -- what is on screen now, as a palette table

local blend_generation = 0

local function show(target)
  local from = M.current
  M.current = target
  blend_generation = blend_generation + 1
  local mine = blend_generation
  if not from or theme.motion() == 0 then
    theme.apply_palette(palette.to_hex(target))
    return
  end
  local step = 0
  local function tick()
    if mine ~= blend_generation then return end
    step = step + 1
    local t = step / M.STEPS
    theme.apply_palette(palette.to_hex(palette.blend(from, target, t)))
    if step < M.STEPS then morf.timer(math.floor(M.TRANSITION_MS / M.STEPS), tick, false) end
  end
  tick()
end

local function write_app_themes(p)
  if not settings.writeAppThemes then return end
  local dir = morf.fs.join(settings.dir, "colors")
  for _, name in ipairs { "kitty", "foot", "alacritty", "btop", "cava", "gtk", "pywal", "json" } do
    local writer = palette.write[name]
    if writer then
      local ok, err = writer(p, morf.fs.join(dir, name == "pywal" and "colors.json" or name))
      if not ok then morf.log("warn", "impasto: theme for " .. name .. ": " .. tostring(err)) end
    end
  end
end

--- Derives the adaptive palette from the picture that is up.
function M.refresh_adaptive()
  local path = wallpaper.current:get()
  if path == "" then return end
  palette.from_image(path, { mode = "dark", rule = "impasto", count = 24 }, function(ok, p)
    if not ok then
      morf.log("warn", "impasto: no palette from " .. path .. ": " .. tostring(p))
      return
    end
    M.derived = p
    if M.active_id:get() == "adaptive" then
      show(p)
      write_app_themes(p)
    end
  end)
end

--- Chooses a palette: "adaptive" or a preset id.
function M.set_theme(id, persist)
  M.active_id:set(id)
  if persist ~= false then settings.set("theme", id) end
  if id == "adaptive" then
    if M.derived then show(M.derived) else M.refresh_adaptive() end
    return
  end
  local ok, p = pcall(palette.preset, id)
  if not ok or not p then
    morf.log("warn", "impasto: unknown palette " .. tostring(id))
    return
  end
  show(p)
  write_app_themes(p)
end

--- The themes Settings offers: adaptive first, then the presets.
function M.available()
  local out = { { id = "adaptive", name = "Adaptive (Wallpaper)" } }
  for _, preset in ipairs(palette.presets) do
    out[#out + 1] = { id = preset.id, name = preset.name or preset.id }
  end
  return out
end

-- The picture changing is the palette changing, when it is adaptive.
local last_picture = nil
morf.effect("impasto.theme.follow", function()
  local path = wallpaper.current:get()
  if path ~= last_picture then
    last_picture = path
    M.derived = nil
    if M.active_id:get() == "adaptive" then
      morf.timer(1, M.refresh_adaptive, false)
    end
  end
end)

M.set_theme(settings.theme, false)

return M
