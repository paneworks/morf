-- Gallery samples for the hud_game instruments: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local morf = require("morf")
local ui = require("morf.ui")

local function k(x) return function() return x end end

-- A list that a timer keeps adding to (the newest `keep`), for the
-- instruments that flash and fade.
local function stream(name, period, keep, make)
  local list = morf.signal("gallery.hud." .. name, {})
  local n = 0
  local function tick()
    n = n + 1
    local t = {}
    for _, x in ipairs(list:get()) do t[#t + 1] = x end
    t[#t + 1] = make(n)
    while #t > keep do table.remove(t, 1) end
    list:set(t)
  end
  return function() return list:get() end, function(node)
    tick()
    local timer = morf.timer(period, tick, true)
    return ui.Item { on_destroyed = function() timer:cancel() end, node }
  end
end

return {
  health_bar = function(kit)
    return ui.Column { gap = 18,
      kit.health_bar { width = 250, value = k(72), max = 100, segments = 4 },
      kit.health_bar { width = 250, value = k(12), max = 100, label = "Low" } }
  end,
  damage_trail_bar = function(kit)
    local hp = morf.signal("gallery.hud.trail", 90)
    local node = ui.Column { gap = 18,
      kit.damage_trail_bar { width = 250, value = function() return hp:get() end, max = 100, delay = 900 },
      kit.damage_trail_bar { width = 250, value = k(40), max = 100, label = "Ally" } }
    morf.timer(300, function() hp:set(58) end, false)
    return node
  end,
  shield_bar = function(kit)
    return kit.shield_bar { width = 250, health = k(80), shield = k(55), max = 100, segments = 4 }
  end,
  boss_bar = function(kit)
    return kit.boss_bar { width = 250, name = "The Hollow King", title = "Warden of Ash", value = k(.58),
      phases = { .66, .33 } }
  end,
  nameplate = function(kit)
    return ui.Column { gap = 16,
      kit.nameplate { width = 200, name = "Cinder Wraith", level = 34, value = k(.64), kind = "enemy" },
      kit.nameplate { width = 200, name = "Mira", level = 31, value = k(.9), kind = "ally", title = "Ashen Guard" } }
  end,
  pip_container = function(kit)
    return ui.Column { gap = 16,
      kit.pip_container { value = k(13), max = 10 },
      kit.pip_container { value = k(5), max = 6, size = 26, color = "info" } }
  end,
  charge_ring = function(kit)
    return ui.Row { gap = 24, align = "center",
      kit.charge_ring { size = 96, value = k(.64), icon = "bolt" },
      kit.charge_ring { size = 80, value = k(1), icon = "local_fire_department", label = "Blast" } }
  end,
  stamina_ring = function(kit)
    return ui.Row { gap = 18, align = "center",
      kit.stamina_ring { size = 80, value = k(.7) }, kit.stamina_ring { size = 64, value = k(.22) },
      kit.stamina_ring { size = 64, value = k(1) } }
  end,
  cooldown_sweep = function(kit)
    return ui.Row { gap = 14, align = "center",
      kit.cooldown_sweep { size = 64, icon = "local_fire_department", value = k(.62), seconds = k(7.4), key = "Q" },
      kit.cooldown_sweep { size = 64, icon = "shield", value = k(.2), seconds = k(2.3), key = "E" },
      kit.cooldown_sweep { size = 64, icon = "bolt", value = k(0), key = "R" } }
  end,
  hotbar = function(kit)
    return kit.hotbar { size = 44, active = k(2), slots = {
      { icon = "swords", key = "1" }, { icon = "local_fire_department", key = "2", cooldown = .55 },
      { icon = "healing", key = "3", count = 4 }, { icon = "shield", key = "4", cooldown = .2 },
      { icon = "science", key = "5", count = 12 } } }
  end,
  buff_row = function(kit)
    return kit.buff_row { size = 38, buffs = {
      { icon = "bolt", remaining = .8, seconds = 42 }, { icon = "shield", remaining = .45, seconds = 18, stacks = 3 },
      { icon = "favorite", remaining = .15, seconds = 4 }, { icon = "coronavirus", remaining = .6, seconds = 12, debuff = true },
      { icon = "ac_unit", remaining = .3, seconds = 6, debuff = true } } }
  end,
  pie_menu = function(kit)
    return kit.pie_menu { size = 184, highlighted = k(2), items = {
      { icon = "swords", label = "Attack" }, { icon = "shield", label = "Defend" }, { icon = "healing", label = "Heal" },
      { icon = "inventory_2", label = "Items" }, { icon = "map", label = "Map" }, { icon = "flag", label = "Ping" } } }
  end,
  minimap = function(kit)
    return kit.minimap { size = 176, heading = k(35), range = 100, blips = {
      { x = 30, y = 45, kind = "enemy" }, { x = -52, y = 20, kind = "enemy" }, { x = 10, y = -38, kind = "ally" },
      { x = -20, y = -60, kind = "ally" }, { x = 70, y = -30, kind = "objective" }, { x = 160, y = 140, kind = "enemy" },
      { x = -40, y = 70 } } }
  end,
  compass_strip = function(kit)
    return ui.Column { gap = 20,
      kit.compass_strip { width = 260, heading = k(28), markers = {
        { angle = 50, icon = "flag", kind = "objective" }, { angle = 350, icon = "person", kind = "ally" },
        { angle = 70, icon = "skull", kind = "enemy" } } },
      kit.compass_strip { width = 260, heading = k(262), fov = 160 } }
  end,
  kill_feed = function(kit)
    return kit.kill_feed { width = 260, max = 5, entries = {
      { killer = "Vex", victim = "Orrin", icon = "my_location" },
      { killer = "Mira", victim = "Grub", icon = "swords", ally = true },
      { killer = "Talon", victim = "Mira", icon = "local_fire_department" },
      { killer = "You", victim = "Talon", icon = "my_location", ally = true },
      { killer = "Kade", victim = "Brin", icon = "bomb", ally = true } } }
  end,
  damage_numbers = function(kit)
    local nums, start = stream("dmg", 260, 6, function(n)
      local crit = n % 4 == 0
      return { id = n, value = crit and 1240 or (100 + (n * 37) % 300), crit = crit, kind = n % 5 == 2 and "heal" or nil,
        x = ((n * .37) % 1), y = ((n * .61) % 1) }
    end)
    return start(kit.damage_numbers { width = 250, height = 180, numbers = nums })
  end,
  objective_tracker = function(kit)
    return kit.objective_tracker { width = 250, title = "The Ember Vault", objectives = {
      { text = "Reach the outer gate", done = true },
      { text = "Light the beacons", count = 2, total = 3 },
      { text = "Recover the relic", value = .4 },
      { text = "Escape the vault" } } }
  end,
  resource_orb = function(kit)
    return ui.Row { gap = 20, align = "center",
      kit.resource_orb { size = 120, value = k(.62) },
      kit.resource_orb { size = 80, value = k(.3), color = "warn", label = "Rage" } }
  end,
  xp_bar = function(kit)
    local xp = morf.signal("gallery.hud.xp", .48)
    local node = ui.Column { gap = 24,
      kit.xp_bar { width = 250, level = 12, value = function() return xp:get() end, gain = k(120) },
      kit.xp_bar { width = 250, level = 13, value = k(.08) } }
    morf.timer(1500, function() xp:set(.66) end, false)
    return node
  end,
  achievement_banner = function(kit)
    return kit.achievement_banner { width = 260, title = "Into the Fire", description = "Defeat the Hollow King",
      points = 50 }
  end,
  subtitle_box = function(kit)
    return kit.subtitle_box { width = 260, speaker = "Mira", kind = "info", max_lines = 4,
      line = "Keep your torch low. Whatever lives down here hates the light more than it hates us." }
  end,
  crosshair = function(kit)
    return ui.Row { gap = 18, align = "center",
      kit.crosshair { style = "cross", spread = k(.3), size = 70 },
      kit.crosshair { style = "dot", spread = k(.5), size = 70 },
      kit.crosshair { style = "circle", spread = k(.2), size = 70 } }
  end,
  hit_marker = function(kit)
    local hits = morf.signal("gallery.hud.hits", 0)
    local node = ui.Row { gap = 30, align = "center",
      kit.hit_marker { size = 60, hit = function() return hits:get() end },
      kit.hit_marker { size = 60, hit = function() return hits:get() end, kill = k(true), linger = 900 } }
    local timer = morf.timer(320, function() hits:set(hits:get() + 1) end, true)
    return ui.Item { on_destroyed = function() timer:cancel() end, node }
  end,
  damage_direction = function(kit)
    return kit.damage_direction { size = 176, hits = { { angle = 60, strength = 1 }, { angle = 200, strength = .5 } } }
  end,
  ammo_counter = function(kit)
    return ui.Column { gap = 16,
      kit.ammo_counter { width = 240, mag = k(22), capacity = 30, total = k(90), weapon = "Carbine" },
      kit.ammo_counter { width = 240, mag = k(2), capacity = 8, total = k(16), weapon = "Shotgun" } }
  end,
  combo_counter = function(kit)
    local n = morf.signal("gallery.hud.combo", 23)
    local node = kit.combo_counter { width = 220, count = function() return n:get() end, multiplier = k(2.5), decay = k(.64) }
    local timer = morf.timer(700, function() n:set(n:get() + 1) end, true)
    return ui.Item { on_destroyed = function() timer:cancel() end, node }
  end,
  offscreen_arrow = function(kit)
    return kit.offscreen_arrow { width = 250, height = 180, angle = k(62), distance = k(248), label = "Relic" }
  end,
  waypoint_marker = function(kit)
    return ui.Row { gap = 10, align = "center",
      kit.waypoint_marker { label = "Vault", distance = k(1420), icon = "flag" },
      kit.waypoint_marker { label = "Mira", distance = k(38), icon = "person", kind = "ally", size = 36 } }
  end,
  lap_tracker = function(kit)
    return kit.lap_tracker { width = 250, lap = k(2), laps = 3, position = k(3), racers = 12, current = k(41.27),
      last = k(74.03), best = k(73.15) }
  end,
  racing_hud = function(kit)
    return kit.racing_hud { size = 180, speed = k(214), rpm = k(.88), gear = k(5) }
  end,
  interaction_prompt = function(kit)
    return ui.Column { gap = 18,
      kit.interaction_prompt { width = 240, key = "E", action = "Open the vault", hold = k(.6) },
      kit.interaction_prompt { width = 240, key = "F", action = "Pick up relic" } }
  end,
  inventory_grid = function(kit)
    return kit.inventory_grid { columns = 5, rows = 3, slot = 44, selected = k(7), items = {
      { icon = "swords", rarity = "epic" }, { icon = "healing", count = 4, rarity = "common" },
      { icon = "science", count = 12, rarity = "uncommon" }, nil, { icon = "diamond", rarity = "legendary" },
      { icon = "shield", rarity = "rare" }, { icon = "key", rarity = "rare" }, nil,
      { icon = "local_fire_department", count = 3, rarity = "uncommon" }, nil,
      nil, { icon = "map", rarity = "common" } } }
  end,
  scoreboard = function(kit)
    return kit.scoreboard { width = 260, teams = {
      { name = "Embers", score = 42, kind = "info", players = {
        { name = "You", kills = 14, deaths = 3, self = true }, { name = "Mira", kills = 11, deaths = 5 },
        { name = "Kade", kills = 9, deaths = 7 } } },
      { name = "Ashen", score = 37, kind = "alert", players = {
        { name = "Talon", kills = 13, deaths = 8 }, { name = "Vex", kills = 10, deaths = 9 },
        { name = "Orrin", kills = 6, deaths = 11 } } } } }
  end,
}
