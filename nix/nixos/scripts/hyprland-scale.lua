-- Loaded before the user's monitor rules so changing scale preserves their
-- resolution, placement, transform and other display settings.
return function(path, fallback)
    local monitor, rules = hl.monitor, {}
    local function copy(value)
        local result = {}
        for k, v in pairs(value) do result[k] = v end
        return result
    end
    hl.monitor = function(rule)
        local merged = copy(rules[rule.output] or rules[""] or {})
        for k, v in pairs(rule) do merged[k] = v end
        rules[rule.output] = merged
        monitor(merged)
    end
    local function read()
        local file = io.open(path, "r")
        if not file then return fallback, "" end
        local text = file:read("*a")
        file:close()
        local scale = tonumber(text:match('"scale"%s*:%s*([%d.]+)'))
        local zoom = tonumber(text:match('"zoom"%s*:%s*([%d.eE+%-]+)'))
        if not scale and zoom then scale = 2 ^ zoom end
        if not scale or scale ~= scale or scale < .5 or scale > 2 then scale = fallback end
        return scale, text:match('"output"%s*:%s*"([%w_.%-]+)"') or ""
    end
    local function set_display_scale(value, output)
        if type(value) ~= "number" or value ~= value or value < .5 or value > 2 then return end
        local base = copy(rules[""] or { output="", mode="preferred", position="auto" })
        base.scale = value
        -- The fallback also covers the first greeter frame, before outputs
        -- have appeared in hl.get_monitors().
        monitor(base)
        for _, screen in ipairs(hl.get_monitors()) do
            if not output or output == "" or screen.name == output then
                local rule = copy(rules[screen.name] or rules["desc:" .. screen.description] or base)
                rule.output, rule.scale = screen.name, value
                monitor(rule)
            end
        end
    end
    local function apply() set_display_scale(read()) end
    hl.on("hyprland.start", apply)
    local pending
    hl.on("monitor.added", function()
        if pending and pending:is_enabled() then pending:set_enabled(false) end
        -- User monitor layout hooks finish first.
        pending = hl.timer(apply, { timeout=400, type="oneshot" })
    end)
    return apply
end
