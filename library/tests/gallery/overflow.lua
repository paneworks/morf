-- Gallery samples for the Overflow archetype's widgets (samples/init.lua):
-- each a line given more than its 260 px hold, so something goes behind
-- "more" -- a toolbar's actions, a tab strip keeping its chosen tab, a
-- breadcrumb trail keeping its ends, filter chips, a site's navigation.
local S = {}

local W = 260

local function chosen(name, first)
  local sig = morf.signal("kit.samples.overflow." .. name, first)
  return function() return sig:get() end, function(i) sig:set(i) end
end

function S.overflow_toolbar(_, w)
  return (w.overflow_toolbar { id = "sample-overflow-toolbar", width = W, accessible_name = "Actions", items = {
    { icon = "content_cut", label = "Cut" }, { icon = "content_copy", label = "Copy" },
    { icon = "content_paste", label = "Paste" }, { icon = "share", label = "Share" },
    { icon = "print", label = "Print" }, { icon = "delete", label = "Delete", priority = -1 },
  } })
end

function S.overflow_tabs(_, w)
  local current, set = chosen("tabs", 4)
  return (w.overflow_tabs { id = "sample-overflow-tabs", width = W, accessible_name = "Sections",
    current = current, on_current_changed = set,
    items = { { label = "Overview" }, { label = "Activity" }, { label = "Members" }, { label = "Billing" },
      { label = "Settings" } } })
end

function S.overflow_breadcrumbs(_, w)
  return (w.overflow_breadcrumbs { id = "sample-overflow-breadcrumbs", width = W, accessible_name = "Path",
    items = { { label = "Home", icon = "home" }, { label = "Documents" }, { label = "Projects" }, { label = "morf" },
      { label = "library" }, { label = "tests" } } })
end

function S.chip_overflow(_, w)
  return (w.chip_overflow { id = "sample-chip-overflow", width = W, accessible_name = "Genres",
    items = { { label = "Rock" }, { label = "Jazz" }, { label = "Ambient" }, { label = "Classical" },
      { label = "Electronic" }, { label = "Folk" } } })
end

function S.priority_nav(_, w)
  local current, set = chosen("nav", 1)
  return (w.priority_nav { id = "sample-priority-nav", width = W, accessible_name = "Site",
    current = current, on_current_changed = set,
    items = { { label = "Home" }, { label = "Products" }, { label = "Pricing" }, { label = "Docs" },
      { label = "Blog" }, { label = "About" } } })
end

return S
