-- What gets launched often and lately, remembered, for ranking a launcher.
--
-- Each id (a desktop entry, a command, a file) has visits: every launch
-- adds one, and every visit's weight halves each `half_life` days, so what
-- was used a lot last year does not outrank what is used every day now.
-- The store is a small JSON file, written a moment after a launch.
--
--   local frecency = require("lib.util.frecency")
--   local used = frecency.open { path = morf.state_path("launches.json") }
--   used.record(app.id)                            -- on launch
--   used.score(app.id)                             -- 0 for never
--   local hits = used.rank(query, apps, { key = "name", id = "id" })
--
-- `rank` is `morf.text.fuzzy` with the frecency added: with an empty query
-- the most used come first; with one, a small lift among matches that are
-- about as good, never enough to put a poor match above a good one.

local morf = require("morf")

local frecency = {}

local DAY = 86400

--- Opens a store. Options: `path` (required), `half_life` (days, 14),
--- `limit` (ids kept, 500: the least used go), `debounce_ms` (500),
--- `now` (a function giving seconds; for tests).
function frecency.open(opts)
  assert(type(opts) == "table" and opts.path, "frecency.open needs a path")
  local half_life = (opts.half_life or 14) * DAY
  local now = opts.now or morf.time.now
  local entries = {} -- id -> { weight, at }
  local pending
  local version = morf.signal((opts.name or "frecency") .. ".version", 0)
  local store = {}

  local ok, text = pcall(morf.fs.read, opts.path)
  if ok and type(text) == "string" then
    local fine, decoded = pcall(morf.json.decode, text)
    if fine and type(decoded) == "table" and type(decoded.entries) == "table" then
      for id, e in pairs(decoded.entries) do
        if type(e) == "table" and tonumber(e.weight) and tonumber(e.at) then
          entries[id] = { weight = tonumber(e.weight), at = tonumber(e.at) }
        end
      end
    end
  end

  local function decayed(e, t)
    return e.weight * 0.5 ^ (math.max(0, t - e.at) / half_life)
  end

  local function save()
    pending = nil
    local out = {}
    for id, e in pairs(entries) do out[id] = { weight = e.weight, at = e.at } end
    local wrote, err = morf.fs.write(opts.path, morf.json.encode { version = 1, entries = out })
    if not wrote then morf.log("warn", "frecency: could not save " .. opts.path .. ": " .. tostring(err)) end
  end

  --- One launch of `id`, now.
  function store.record(id)
    local t = now()
    local e = entries[id]
    entries[id] = { weight = (e and decayed(e, t) or 0) + 1, at = t }
    -- Past the limit, the least used go.
    local limit = opts.limit or 500
    local count, weakest, weakest_score = 0, nil, math.huge
    for other, o in pairs(entries) do
      count = count + 1
      local s = decayed(o, t)
      if other ~= id and s < weakest_score then weakest, weakest_score = other, s end
    end
    if count > limit and weakest then entries[weakest] = nil end
    version:set(version:get() + 1)
    if not pending then pending = morf.timer(opts.debounce_ms or 500, save, false) end
  end

  --- How much `id` is used, now: about the launches of the last half-life.
  --- Tracked in a binding (it changes with every launch).
  function store.score(id)
    version:get()
    local e = entries[id]
    return e and decayed(e, now()) or 0
  end

  --- Forgets `id`.
  function store.forget(id)
    if entries[id] then
      entries[id] = nil
      version:set(version:get() + 1)
      if not pending then pending = morf.timer(opts.debounce_ms or 500, save, false) end
    end
  end

  --- Writes now instead of a moment later.
  function store.flush()
    if pending then pending:cancel() save() end
  end

  --- `morf.text.fuzzy` results (`{ item, index, score, key, positions }`),
  --- reordered with frecency. `opts`: `key` (as fuzzy's), `id` (the field
  --- naming an item's id, or a function of the item; the item itself when
  --- it is a string), `limit`, `weight` (how much frecency counts against
  --- match quality, 0.15).
  function store.rank(query, items, rank_opts)
    rank_opts = rank_opts or {}
    local id_of = rank_opts.id
    local function ident(item)
      if type(id_of) == "function" then return id_of(item) end
      if id_of then return item[id_of] end
      return item
    end
    local hits = morf.text.fuzzy(query or "", items, { key = rank_opts.key })
    local t = now()
    version:get()
    local blend = rank_opts.weight or 0.15
    local best = 0
    for _, hit in ipairs(hits) do best = math.max(best, hit.score or 0) end
    for _, hit in ipairs(hits) do
      local e = entries[ident(hit.item)]
      hit.frecency = e and decayed(e, t) or 0
      if (query or "") == "" then
        hit.rank = hit.frecency
      else
        -- The match's quality relative to the best, lifted by a share of
        -- the frecency that levels off: a few launches matter, hundreds
        -- do not matter a hundred times more.
        local quality = best > 0 and (hit.score or 0) / best or 0
        hit.rank = quality + blend * (1 - 1 / (1 + hit.frecency))
      end
    end
    table.sort(hits, function(a, b)
      if a.rank ~= b.rank then return a.rank > b.rank end
      return a.index < b.index
    end)
    if rank_opts.limit and #hits > rank_opts.limit then
      for i = #hits, rank_opts.limit + 1, -1 do hits[i] = nil end
    end
    return hits
  end

  return store
end

return frecency
