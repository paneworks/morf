-- A GitHub user's contribution calendar: a year of days, each with a count
-- and the 0-4 shade GitHub draws it in.
--
-- Without a token the calendar comes from the page GitHub renders it in,
-- `github.com/users/<user>/contributions`, which is public and needs no
-- account. That page is HTML, not an API, so it is read by what identifies a
-- day -- a cell with a `data-date`, its `data-level`, and the tooltip that
-- names it with the count -- and not by where those sit, so a reshuffled page
-- still reads. With a token the GraphQL API answers the same question in a
-- documented shape.
--
--   local github = require("lib.integrations.github")
--   local me = github.new { user = "torvalds" }
--   ui.Text { text = function()
--     local calendar = me:get()
--     return calendar.available and (calendar.total .. " this year") or ""
--   end }

local morf = require("morf")
local poll = require("lib.util.poll")

local github = {}

-- ---------------------------------------------------------------------------
-- Dates, as day numbers: the calendar is a run of consecutive days, and
-- counting them needs no time zone.

--- Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
function github.day_number(year, month, day)
  if month <= 2 then year = year - 1 end
  local era = (year >= 0 and year or year - 399) // 400
  local yoe = year - era * 400
  local mp = (month + 9) % 12
  local doy = (153 * mp + 2) // 5 + day - 1
  local doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
  return era * 146097 + doe - 719468
end

local function day_of(date)
  local y, m, d = date:match("^(%d+)-(%d+)-(%d+)")
  if not y then return nil end
  return github.day_number(tonumber(y), tonumber(m), tonumber(d))
end

local LEVELS = { NONE = 0, FIRST_QUARTILE = 1, SECOND_QUARTILE = 2, THIRD_QUARTILE = 3, FOURTH_QUARTILE = 4 }

--- Totals and streaks over `days` (sorted `{ date, count, level }`). `today`
--- is a "YYYY-MM-DD"; a streak still counts when today has nothing *yet*.
function github.summarise(days, today, spend)
  spend = spend or function() end
  local total, longest, run, max = 0, 0, 0, 0
  for index, day in ipairs(days) do
    if not day.number then
      spend(6)
      day.number = day_of(day.date)
    end
    -- 1970-01-04 was a Sunday, and GitHub's weeks start on Sunday.
    day.weekday = (day.number - 3) % 7
    total = total + day.count
    if day.count > max then max = day.count end
    local previous = days[index - 1]
    if day.count > 0 then
      if previous and previous.count > 0 and previous.number == day.number - 1 then
        run = run + 1
      else
        run = 1
      end
      if run > longest then longest = run end
    else
      run = 0
    end
  end
  -- The current streak runs back from today, or from yesterday when today
  -- is still empty.
  local current = 0
  local last = #days
  local today_number = today and day_of(today)
  while last > 0 and today_number and days[last].number > today_number do last = last - 1 end
  local today_count = (last > 0 and today_number and days[last].number == today_number) and days[last].count or 0
  if last > 0 and days[last].count == 0 and days[last].number == today_number then last = last - 1 end
  -- A streak that ended before yesterday is over, whatever it was.
  if last > 0 and today_number and days[last].number < today_number - 1 then last = 0 end
  while last > 0 and days[last].count > 0 do
    current = current + 1
    if last > 1 and days[last - 1].number ~= days[last].number - 1 then break end
    last = last - 1
  end
  -- Columns of seven, Sunday first, the way the calendar is drawn: the first
  -- column is padded with nils up to the first day's weekday.
  local weeks, week = {}, nil
  for _, day in ipairs(days) do
    if not week or day.weekday == 0 then
      week = {}
      weeks[#weeks + 1] = week
    end
    week[day.weekday + 1] = day
  end
  return {
    total = total,
    longest_streak = longest,
    current_streak = current,
    today = today_count,
    max = max,
    weeks = weeks,
  }
end

--- Reads the contributions page. Returns the days, sorted, and the total the
--- page states (nil when it does not). Work is counted with `spend`.
function github.parse_html(html, spend)
  spend = spend or function() end
  -- The tooltips first: "5 contributions on October 19th." for the cell whose
  -- id they name, "No contributions on ..." for none.
  local counts = {}
  for target, text in html:gmatch('<tool%-tip[^>]-for="([^"]+)"[^>]*>([^<]*)</tool%-tip>') do
    spend(5)
    local count = text:match("^%s*(%d[%d,]*)")
    counts[target] = count and tonumber((count:gsub(",", ""))) or 0
  end
  local days = {}
  for attributes in html:gmatch("<td([^>]*data%-date[^>]*)>") do
    spend(14)
    local year, month, day = attributes:match('data%-date="(%d+)%-(%d+)%-(%d+)"')
    if year then
      local id = attributes:match('id="([^"]+)"')
      local level = tonumber(attributes:match('data%-level="(%d)"')) or 0
      -- Older pages carried the count on the cell itself.
      local count = tonumber(attributes:match('data%-count="(%d+)"')) or (id and counts[id])
      -- A cell without a count still has a shade; "some" is at least one.
      if not count then count = level > 0 and 1 or 0 end
      days[#days + 1] = {
        date = year .. "-" .. month .. "-" .. day, count = count, level = level,
        number = github.day_number(tonumber(year), tonumber(month), tonumber(day)),
      }
    end
  end
  -- Cells come row by row -- all the Sundays, then all the Mondays -- so
  -- they are put in date order by their day number: one pass, where a sort
  -- with a Lua comparison would spend a handler's budget on its own.
  local first
  for _, entry in ipairs(days) do
    if not first or entry.number < first then first = entry.number end
  end
  local slots, last = {}, 0
  for _, entry in ipairs(days) do
    local slot = entry.number - first + 1
    slots[slot] = entry
    if slot > last then last = slot end
  end
  spend(#days // 4)
  local ordered = {}
  for slot = 1, last do
    if slots[slot] then ordered[#ordered + 1] = slots[slot] end
  end
  days = ordered
  local stated = html:match("([%d,]+)%s+contributions?%s+in the last year")
  return days, stated and tonumber((stated:gsub(",", ""))) or nil
end

--- Reads a GraphQL `contributionCalendar`.
function github.parse_graphql(data)
  local calendar = data and data.data and data.data.user and data.data.user.contributionsCollection
    and data.data.user.contributionsCollection.contributionCalendar
  if not calendar then return nil end
  local days = {}
  for _, week in ipairs(calendar.weeks or {}) do
    for _, day in ipairs(week.contributionDays or {}) do
      days[#days + 1] = {
        date = day.date,
        count = tonumber(day.contributionCount) or 0,
        level = LEVELS[day.contributionLevel] or 0,
        number = day_of(day.date),
      }
    end
  end
  return days, tonumber(calendar.totalContributions)
end

local QUERY = [[
query($login: String!) {
  user(login: $login) {
    contributionsCollection {
      contributionCalendar {
        totalContributions
        weeks { contributionDays { date contributionCount contributionLevel } }
      }
    }
  }
}]]

-- ---------------------------------------------------------------------------
-- A user's calendar

local Calendar = {}
Calendar.__index = Calendar

--- Options: `user` (required); `token` (a GitHub token: the GraphQL API
--- instead of the page); `interval` (ms between refreshes while read, default
--- an hour); `ttl` (seconds an answer is fresh, default an hour);
--- `cache_dir`; `today` (a function returning "YYYY-MM-DD"); `base_url` (default https://github.com) and `api_url`
--- (default https://api.github.com/graphql), for tests and mirrors.
function github.new(options)
  assert(options and options.user, "github.new needs a user")
  local self = setmetatable({
    user = options.user,
    token = options.token,
    ttl = options.ttl or 3600,
    cache_dir = options.cache_dir,
    base_url = options.base_url or "https://github.com",
    api_url = options.api_url or "https://api.github.com/graphql",
    -- Today, as "YYYY-MM-DD": the local date unless told otherwise.
    today = options.today or function() return morf.time.format("%Y-%m-%d") end,
  }, Calendar)
  self.source = poll.source {
    name = "github." .. options.user,
    interval = options.interval or 60 * 60 * 1000,
    linger = 1,
    initial = { available = false, user = options.user, days = {}, weeks = {}, total = 0,
      current_streak = 0, longest_streak = 0, today = 0, max = 0 },
    sample = function(done) self:_fetch(done) end,
  }
  return self
end

--- The calendar, read so a binding follows it: `{ available, user, days =
--- { {date, count, level, weekday} }, weeks (columns of seven, Sunday first),
--- total, current_streak, longest_streak, today, max, source, updated, stale }`.
function Calendar:get() return self.source:get() end

--- Asks again now, past the cache.
function Calendar:refresh()
  self._force = true
  self.source:refresh()
end

function Calendar:_cache_path()
  return (self.cache_dir or poll.cache_dir()) .. "/github-" .. self.user:gsub("[^%w_-]", "_") .. ".json"
end

function Calendar:_finish(done, days, stated, source, spend)
  local summary = github.summarise(days, self.today(), spend)
  for _, day in ipairs(days) do day.number = nil end
  local value = {
    available = #days > 0,
    user = self.user,
    source = source,
    days = days,
    weeks = summary.weeks,
    -- What GitHub says, when it says, over what the days add up to: private
    -- contributions are in its number and not in the cells.
    total = stated or summary.total,
    current_streak = summary.current_streak,
    longest_streak = summary.longest_streak,
    today = summary.today,
    max = summary.max,
    updated = morf.time.now(),
  }
  poll.cache_write(self:_cache_path(), { user = value.user, source = source, days = days, total = value.total })
  done(value)
end

function Calendar:_from_cache(entry, stale, spend)
  local days = entry.days or {}
  local copy = {}
  for index, day in ipairs(days) do
    copy[index] = { date = day.date, count = day.count, level = day.level }
  end
  if spend then spend(#days) end
  local summary = github.summarise(copy, self.today(), spend)
  for _, day in ipairs(copy) do day.number = nil end
  return {
    available = #copy > 0, user = self.user, source = entry.source, days = copy,
    weeks = summary.weeks, total = entry.total or summary.total,
    current_streak = summary.current_streak, longest_streak = summary.longest_streak,
    today = summary.today, max = summary.max, stale = stale or nil,
  }
end

function Calendar:_failed(done, why)
  local _, _, old = poll.cache_read(self:_cache_path(), 0)
  if old then done(self:_from_cache(old, true)) else done(nil, why) end
end

function Calendar:_fetch(done)
  if not self._force then
    local cached = poll.cache_read(self:_cache_path(), self.ttl)
    if cached then
      -- Rebuilding the summary is a pass over a year of days; a job, like
      -- the parse, so it cannot run a handler out of instructions.
      poll.job(function(spend) return self:_from_cache(cached, nil, spend) end, done)
      return
    end
  end
  self._force = false
  if self.token then
    morf.http.post(self.api_url, { query = QUERY, variables = { login = self.user } }, {
      headers = { authorization = "bearer " .. self.token, ["user-agent"] = "morf" },
      timeout_ms = 20000,
    }, function(response)
      local data = response.ok and response.json() or nil
      local days, total = github.parse_graphql(data)
      if not days then
        self:_failed(done, response.error or ("github answered " .. response.status))
        return
      end
      poll.job(function(spend) self:_finish(done, days, total, "graphql", spend) end)
    end)
    return
  end
  local url = self.base_url .. "/users/" .. morf.http.url_encode(self.user) .. "/contributions"
  morf.http.get(url, { timeout_ms = 20000, max_bytes = 4 * 1024 * 1024 }, function(response)
    if not response.ok then
      self:_failed(done, response.error or ("github answered " .. response.status))
      return
    end
    local body = response.body
    poll.job(function(spend)
      local days, stated = github.parse_html(body, spend)
      if #days == 0 then
        self:_failed(done, "no calendar in the page")
        return
      end
      self:_finish(done, days, stated, "page", spend)
    end, function(_, err)
      if err then self:_failed(done, err) end
    end)
  end)
end

return github
