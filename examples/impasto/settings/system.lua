-- Settings, System: the profiles with Reset under them, and this machine
-- (SystemSection).
--
-- The original's updater (a git checkout pulled and installed again) and
-- its list of changed files belong to impasto's installer, which this port
-- does not have; the page says which morf is running instead. The night
-- light switch is here as well as under Displays, through services/night.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local night = require("services.night")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local palette_board = require("components.palette_board")
local profiles_part = require("settings.profiles_part")

local C = theme.color

local M = {}

local fs = morf.fs

-- ------------------------------------------------------------- machine --

local function processor()
  local text = fs.read("/proc/cpuinfo") or ""
  local model = text:match("model name%s*:%s*([^\n]+)") or text:match("Hardware%s*:%s*([^\n]+)") or "—"
  local threads = 0
  for _ in text:gmatch("\nprocessor%s*:") do threads = threads + 1 end
  if text:match("^processor%s*:") then threads = threads + 1 end
  model = model:gsub("%(R%)", ""):gsub("%(TM%)", ""):gsub("%s+", " ")
  return model, threads
end

local function memory()
  local text = fs.read("/proc/meminfo") or ""
  local total = tonumber(text:match("MemTotal:%s*(%d+)")) or 0
  local available = tonumber(text:match("MemAvailable:%s*(%d+)")) or 0
  return total * 1024, total > 0 and (1 - available / total) or 0
end

local function uptime()
  local text = fs.read("/proc/uptime") or ""
  return math.floor(tonumber(text:match("^(%S+)")) or 0)
end

local function spell(seconds)
  local days = seconds // 86400
  local hours = (seconds % 86400) // 3600
  local minutes = (seconds % 3600) // 60
  if days > 0 then return days .. "d " .. hours .. "h" end
  if hours > 0 then return hours .. "h " .. minutes .. "m" end
  return minutes .. "m"
end

local function profiles_tab(W)
  return {
    profiles_part.build(W),
    setting.group {
      width = W, title = "Reset",
      note = "Only the profile in use. The others are left as they were.",
      hint = "Reset returns every setting in this profile to its default. Your name, picture, screens and keyboard are kept.",
      setting.row { width = W, label = "Where the settings live",
        control = kit.text { text = settings.path:gsub("^" .. (fs.home() or "~"):gsub("%p", "%%%0"), "~"),
          mono = true, size = theme.size.label, color = C.textMuted } },
      setting.row { width = W, label = "Reset this profile", reading = "Back to the defaults",
        control = controls.pill { text = "Reset", icon = "󰜉", height = 30, width = 92,
          on_click = function() settings.reset_all() end } },
    },
  }
end

local function machine_tab(W)
  local model, threads = processor()
  local total, used = memory()
  local cell = math.floor((W - 28 - 3 * 14) / 4)
  return {
    setting.group {
      width = W, title = "This window", note = "The language of this window.",
      hint = "Only English is written for this port; the original also had Spanish.",
      setting.row { width = W, label = "Language",
        control = controls.segmented { options = { { id = "en", label = "English" } },
          current = function() return settings.language end,
          on_selected = function(id) settings.set("language", id) end } },
    },
    setting.group {
      width = W, title = "This machine",
      setting.block { width = W,
        ui.Row { gap = 14,
          controls.figure { width = cell, label = "PROCESSOR", value = model, note = threads .. " threads" },
          controls.figure { width = cell, label = "MEMORY",
            value = total > 0 and string.format("%.1f GiB", total / 1073741824) or "—",
            note = math.floor(used * 100 + 0.5) .. "% in use" },
          controls.figure { width = cell, label = "UPTIME",
            value = function() morf.clock:get() return spell(uptime()) end, note = "since boot" },
          controls.figure { width = cell, label = "VERSION", value = "morf " .. tostring(morf.version or "?"),
            note = "impasto, ported" },
        },
      },
    },
    setting.group {
      width = W, title = "Night light", note = "Warmer colours for the evening.",
      hint = "The temperature is set under Displays.",
      setting.switch_row { width = W, label = "Warm the screen",
        reading = function()
          if not night.available() then return "Needs hyprsunset, which is not installed" end
          return settings.nightLight and (settings.nightTemperature .. " K") or "Off"
        end,
        checked = function() return settings.nightLight end,
        on_toggled = function(on) night.set(on) end },
    },
    -- The colophon: the board over the name in the signature face.
    ui.Column {
      gap = 2, align = "start",
      ui.Item { width = 1, height = 8 },
      palette_board { size = 76 },
      kit.text { text = "impasto", size = 34,
        font_family = function() return theme.font_signature() end,
        font_source = fs.is_file(theme.hand_file) and theme.hand_file or "" },
      kit.text { text = "A Hyprland shell, and the desk around it — on morf", size = theme.size.small,
        color = C.textMuted },
      kit.text { text = "github.com/andreumassanet/impasto", size = theme.size.label,
        color = C.textMuted, opacity = 0.6 },
    },
  }
end

function M.build(page)
  return setting.parts(page, {
    { id = "profiles", build = profiles_tab },
    { id = "machine", build = machine_tab },
  })
end

return M
