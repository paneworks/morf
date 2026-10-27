-- The widget contract: what every theme answers to, as data.
--
-- The list is libadwaita's, with the GTK 4 widgets libadwaita styles: each
-- widget named after its Adw or GTK class in snake case without the
-- prefix (`AdwActionRow` is `action_row`), its variants the Adwaita style
-- classes it takes (`suggested`, `flat`, `pill`, ...). A theme defines the
-- tokens and a skin for each archetype and display widget; the widgets
-- themselves are built by `lib.kit.widgets` for every theme alike.
--
--   tokens      the named colours, type, shape and motion a theme sets
--   archetypes  behaviours with no look (crates/engine/morf-kit), each
--               with the widgets made from it
--   display     widgets that take no input
--   composites  widgets built from archetypes (lib.kit.composites)
--   shell       widgets a desktop shell needs that libadwaita lacks, each
--               for the surface that needs it
--   editor      the Canvas and Dock archetypes and their widgets, for
--               editor-like applications; a theme may skip the group

local M = {}

-- ----------------------------------------------------------------- tokens --
--
-- libadwaita's named colours, snake case without `_color`; its type
-- classes; the radii and opacities its stylesheet is written in; its
-- spring presets and easing names.

M.tokens = {
  colors = {
    "accent_bg", "accent_fg", "accent",
    "destructive_bg", "destructive_fg", "destructive",
    "success_bg", "success_fg", "success",
    "warning_bg", "warning_fg", "warning",
    "error_bg", "error_fg", "error",
    "window_bg", "window_fg",
    "view_bg", "view_fg",
    "headerbar_bg", "headerbar_fg", "headerbar_border", "headerbar_backdrop", "headerbar_shade",
    "sidebar_bg", "sidebar_fg", "sidebar_backdrop", "sidebar_border", "sidebar_shade",
    "secondary_sidebar_bg", "secondary_sidebar_fg", "secondary_sidebar_backdrop", "secondary_sidebar_border",
    "secondary_sidebar_shade",
    "card_bg", "card_fg", "card_shade",
    "dialog_bg", "dialog_fg",
    "popover_bg", "popover_fg", "popover_shade",
    "thumbnail_bg", "thumbnail_fg",
    "shade", "border", "scrollbar_outline",
  },
  type = { "title_1", "title_2", "title_3", "title_4", "heading", "body", "caption_heading", "caption",
    "monospace", "numeric" },
  shape = { "window_radius", "dialog_radius", "card_radius", "button_radius", "popover_radius",
    "border_opacity", "dim_opacity", "focus_ring_width" },
  motion = { "spring", "spring_snappy", "spring_gentle", "duration_short", "duration_normal", "duration_long",
    "easing" },
}

-- ------------------------------------------------------------- archetypes --
--
-- Each archetype: its Adw or GTK model, the accessible roles it plays, its
-- live state, signals, keys and slots, and its widgets -- `name`, the
-- class it is, and `variants` (style classes) where it takes any.

local function w(name, adw, variants) return { name = name, adw = adw, variants = variants } end

M.archetypes = {
  Press = { adw = "GtkButton",
    role = { "button", "toggle_button", "check_box", "radio_button", "switch", "link", "menu_item" },
    state = { "checkable", "checked", "tristate", "partial", "auto_repeat", "delay", "interval", "pressed_at", "group" },
    signals = { "on_clicked", "on_pressed", "on_released", "on_toggled", "on_long_pressed", "on_double_clicked" },
    keys = { "space", "return", "arrows_in_group" },
    slots = { "indicator", "icon", "label", "badge" },
    widgets = {
      w("button", "GtkButton", { "suggested", "destructive", "flat", "raised", "opaque", "pill", "circular", "osd" }),
      w("toggle_button", "GtkToggleButton", { "flat" }),
      w("check_button", "GtkCheckButton", { "selection_mode" }),
      w("switch", "GtkSwitch"),
      w("link_button", "GtkLinkButton"),
      w("menu_item", "GtkPopoverMenu"),
      w("tab_button", "AdwTabButton"),
    } },
  Range = { adw = "GtkRange",
    role = { "slider", "spin_button", "scroll_bar" },
    state = { "from", "to", "value", "step", "page_step", "snap", "live", "orientation", "inverted", "logarithmic",
      "position", "visual_position", "wrap", "range", "first", "second" },
    signals = { "on_moved", "on_value_changed", "increase", "decrease" },
    keys = { "arrows", "page_up", "page_down", "home", "end" },
    slots = { "track", "fill", "handle", "second_handle", "ticks", "value_label", "increase", "decrease" },
    widgets = {
      w("scale", "GtkScale", { "osd" }),
      w("spin_button", "GtkSpinButton"),
      w("scrollbar", "GtkScrollbar", { "overlay" }),
    } },
  -- (Only inside color_chooser: its colour plane.)
  Plane = { adw = "GtkColorChooserWidget",
    role = { "slider" },
    state = { "x_from", "x_to", "y_from", "y_to", "x", "y", "step_x", "step_y", "visual_x", "visual_y", "constraint" },
    signals = { "on_moved", "on_value_changed" },
    keys = { "arrows", "page_up", "page_down" },
    slots = { "field", "handle", "crosshair" },
    widgets = {} },
  Selection = { adw = "AdwToggleGroup",
    role = { "tab_list", "list_box", "radio_group", "grid" },
    state = { "model", "current", "selected", "mode", "wrap", "orientation", "follow_focus" },
    signals = { "on_current_changed", "on_selection_changed", "on_activated" },
    keys = { "arrows", "home", "end", "typeahead", "space_toggles", "ctrl_a" },
    slots = { "item", "indicator", "separator" },
    widgets = {
      w("toggle_group", "AdwToggleGroup"),
      w("view_switcher", "AdwViewSwitcher"),
      w("inline_view_switcher", "AdwInlineViewSwitcher"),
      w("view_switcher_bar", "AdwViewSwitcherBar"),
      w("stack_switcher", "GtkStackSwitcher"),
      w("carousel_indicator_dots", "AdwCarouselIndicatorDots"),
      w("carousel_indicator_lines", "AdwCarouselIndicatorLines"),
    } },
  Popup = { adw = "GtkPopover",
    role = { "dialog", "alert_dialog", "menu", "tooltip", "list_box_popup" },
    state = { "open", "modal", "dim", "close_policy", "placement", "anchor", "side", "align", "flip", "shift",
      "focus_on_open", "restore_focus" },
    signals = { "on_opened", "on_closed", "on_about_to_close" },
    keys = { "escape", "tab_trap" },
    slots = { "dim", "enter", "exit" },
    widgets = {
      w("popover", "GtkPopover"),
      w("popover_menu", "GtkPopoverMenu"),
      w("popover_menu_bar", "GtkPopoverMenuBar"),
      w("tooltip", "GtkTooltip"),
      w("dialog", "AdwDialog"),
      w("alert_dialog", "AdwAlertDialog"),
      w("bottom_sheet", "AdwBottomSheet"),
    } },
  TextField = { adw = "GtkEditable",
    role = { "text_field", "password_text", "text_area", "search_field" },
    state = { "text", "placeholder", "echo", "read_only", "max_length", "validator", "acceptable", "multiline", "wrap",
      "selected_text" },
    signals = { "on_edited", "on_accepted", "on_text_changed", "on_invalid" },
    keys = { "editing", "return_accepts", "escape_reverts" },
    slots = { "field", "leading", "trailing", "placeholder", "counter", "error" },
    widgets = {
      w("entry", "GtkEntry"),
      w("password_entry", "GtkPasswordEntry"),
      w("search_entry", "GtkSearchEntry"),
      w("text_view", "GtkTextView"),
      w("editable_label", "GtkEditableLabel"),
    } },
  Scroll = { adw = "GtkScrolledWindow",
    role = { "scroll_pane" },
    state = { "content_x", "content_y", "content_width", "content_height", "scroll_policy", "snap", "at_start",
      "at_end" },
    signals = { "on_scrolled", "on_reached_start", "on_reached_end" },
    keys = { "arrows", "page_up", "page_down", "home", "end", "space" },
    slots = { "scroll_bar_x", "scroll_bar_y", "edge_fade", "overscroll" },
    widgets = {
      w("scrolled_window", "GtkScrolledWindow", { "undershoot_top", "undershoot_bottom" }),
    } },
  Collection = { adw = "GtkListView",
    role = { "list", "grid", "table", "tree" },
    state = { "model", "delegate", "layout", "columns", "expanded", "section", "item_size" },
    signals = { "on_row_activated", "on_sort_changed", "on_expanded_changed", "on_end_reached" },
    keys = { "selection_keys", "left_right_tree" },
    slots = { "row", "header", "section_header", "footer", "empty", "loading", "placeholder_row" },
    widgets = {
      w("list_view", "GtkListView", { "navigation_sidebar", "rich_list" }),
      w("grid_view", "GtkGridView"),
      w("column_view", "GtkColumnView", { "data_table" }),
      w("list_box", "GtkListBox", { "boxed_list", "boxed_list_separate" }),
      w("flow_box", "GtkFlowBox"),
      w("tree_expander", "GtkTreeExpander"),
    } },
  Disclosure = { adw = "GtkExpander",
    role = { "button_expanded", "group" },
    state = { "expanded", "group", "animated" },
    signals = { "on_expanded", "on_collapsed" },
    keys = { "space", "return", "left_right_tree" },
    slots = { "header", "indicator", "content" },
    widgets = {
      w("expander", "GtkExpander"),
      w("revealer", "GtkRevealer"),
    } },
  Drag = { adw = "GtkGesture",
    role = { "splitter", "grip", "draggable" },
    state = { "axis", "bounds", "threshold", "active", "delta", "target", "mode" },
    modes = { "move", "resize", "split", "reorder", "swipe", "transfer" },
    signals = { "on_drag_started", "on_dragged", "on_dropped", "on_swiped" },
    keys = { "arrows", "alt_arrows_reorder" },
    slots = { "handle", "ghost", "drop_indicator" },
    widgets = {
      w("paned", "GtkPaned", { "wide" }),
      w("window_handle", "GtkWindowHandle"),
      w("drag_source", "GtkDragSource"),
      w("drop_target", "GtkDropTarget"),
    } },
  Navigation = { adw = "AdwNavigationView",
    role = { "tab_panel", "group" },
    state = { "pages", "current", "mode", "can_go_back", "history" },
    signals = { "on_pushed", "on_popped", "on_current_changed" },
    keys = { "alt_left", "back", "ctrl_tab", "arrows_carousel" },
    slots = { "page", "transition", "back", "indicator" },
    widgets = {
      w("navigation_view", "AdwNavigationView"),
      w("view_stack", "AdwViewStack"),
      w("stack", "GtkStack"),
      w("carousel", "AdwCarousel"),
      w("tab_view", "AdwTabView"),
    } },
  Shell = { adw = "AdwApplicationWindow",
    role = { "application", "navigation", "main", "complementary" },
    state = { "breakpoints", "collapsed", "layout", "sidebar_width", "content_width", "toolbar_style" },
    signals = { "on_breakpoint", "on_collapsed" },
    keys = { "f9", "ctrl_b", "f6" },
    slots = { "header_bar", "sidebar", "content", "inspector", "bottom_bar", "toolbar_top", "toolbar_bottom", "banner",
      "toasts" },
    widgets = {
      w("application_window", "AdwApplicationWindow"),
      w("header_bar", "AdwHeaderBar", { "flat" }),
      w("toolbar_view", "AdwToolbarView", { "raised", "flat" }),
      w("navigation_split_view", "AdwNavigationSplitView"),
      w("overlay_split_view", "AdwOverlaySplitView"),
      w("breakpoint_bin", "AdwBreakpointBin"),
      w("clamp", "AdwClamp"),
      w("multi_layout_view", "AdwMultiLayoutView"),
    } },
}

-- ---------------------------------------------------------------- display --
--
-- Widgets that take no input, each skinned by name like everything else.

M.display = {
  w("label", "GtkLabel", { "title_1", "title_2", "title_3", "title_4", "heading", "body", "caption",
    "caption_heading", "monospace", "numeric", "dim", "accent", "success", "warning", "error" }),
  w("image", "GtkImage"),
  w("avatar", "AdwAvatar"),
  w("spinner", "AdwSpinner"),
  w("progress_bar", "GtkProgressBar", { "osd" }),
  w("level_bar", "GtkLevelBar"),
  w("separator", "GtkSeparator", { "spacer" }),
  w("status_page", "AdwStatusPage", { "compact" }),
  w("shortcut_label", "GtkShortcutLabel"),
  w("window_title", "AdwWindowTitle"),
  w("button_content", "AdwButtonContent"),
  w("box", "GtkBox", { "card", "linked", "toolbar", "osd", "boxed_list", "view" }),
  w("frame", "GtkFrame"),
}

-- ------------------------------------------------------------- composites --
--
-- Built from archetypes (library/lib/kit/composites/), shared by every
-- theme; `parts` are the archetypes each is made of.

local function c(name, adw, parts, variants) return { name = name, adw = adw, parts = parts, variants = variants } end

M.composites = {
  c("about_dialog", "AdwAboutDialog", { "Popup", "Navigation" }),
  c("action_row", "AdwActionRow", { "Press" }, { "property" }),
  c("banner", "AdwBanner", { "Disclosure", "Press" }),
  c("button_row", "AdwButtonRow", { "Press" }),
  c("calendar", "GtkCalendar", { "Selection", "Press" }),
  c("color_chooser", "GtkColorChooserWidget", { "Plane", "Range", "Selection", "TextField" }),
  c("combo_row", "AdwComboRow", { "Press", "Popup", "Selection" }),
  c("drop_down", "GtkDropDown", { "Press", "Popup", "Selection" }),
  c("emoji_chooser", "GtkEmojiChooser", { "TextField", "Selection", "Popup" }),
  c("entry_row", "AdwEntryRow", { "TextField" }),
  c("expander_row", "AdwExpanderRow", { "Disclosure", "Press" }),
  c("file_chooser", "GtkFileChooserWidget", { "Shell", "Collection", "Navigation", "TextField" }),
  c("font_chooser", "GtkFontChooserWidget", { "TextField", "Collection" }),
  c("media_controls", "GtkMediaControls", { "Press", "Range" }),
  c("menu_button", "GtkMenuButton", { "Press", "Popup" }, { "flat", "circular", "pill", "suggested" }),
  c("password_entry_row", "AdwPasswordEntryRow", { "TextField" }),
  c("preferences_dialog", "AdwPreferencesDialog", { "Popup", "Navigation", "Selection" }),
  c("preferences_group", "AdwPreferencesGroup", { "Collection" }),
  c("preferences_page", "AdwPreferencesPage", { "Scroll" }),
  c("search_bar", "GtkSearchBar", { "TextField", "Disclosure" }),
  c("shortcuts_dialog", "AdwShortcutsDialog", { "Popup", "Collection", "TextField" }),
  c("sidebar", "AdwSidebar", { "Selection", "Disclosure" }),
  c("spin_row", "AdwSpinRow", { "Range" }),
  c("split_button", "AdwSplitButton", { "Press", "Popup" }, { "suggested", "destructive", "flat" }),
  c("switch_row", "AdwSwitchRow", { "Press" }),
  c("tab_bar", "AdwTabBar", { "Selection", "Drag", "Press" }),
  c("tab_overview", "AdwTabOverview", { "Collection", "Popup" }),
  c("toast_overlay", "AdwToastOverlay", { "Popup", "Press" }),
}

-- ------------------------------------------------------------------ shell --
--
-- Nothing joins without a surface that needs it.

M.shell = {
  { name = "notification", surface = "the notification popups and centre", parts = { "Popup", "Press" } },
  { name = "osd", surface = "volume and brightness pop-ups", parts = { "Popup" } },
  { name = "tray_item", surface = "a StatusNotifier item in a bar", parts = { "Press", "Popup" } },
}

-- ----------------------------------------------------------------- editor --
--
-- A world a viewport looks into, and panels the user rearranges: mara's
-- graph, canvas, board, map and image views, its shelves and tabbed panes.

M.editor = {
  Canvas = {
    role = { "group", "image" },
    state = { "view_x", "view_y", "zoom", "tool", "selection", "hovered", "pointer_x", "pointer_y", "gesture",
      "band", "draft", "connect_from", "connect_to" },
    tools = { "select", "pan", "point", "line", "rect", "ellipse", "polyline", "polygon", "freehand", "connect",
      "brush", "zoom" },
    signals = { "on_view_changed", "on_selection_changed", "on_moved", "on_drawn", "on_connected", "on_activated",
      "on_context", "on_brushed", "on_deleted" },
    keys = { "arrows_nudge", "plus_minus_zoom", "zero_reset", "home_fit", "ctrl_a", "delete", "escape", "tab_items" },
    slots = { "grid", "item", "wires", "selection", "draft", "band", "crosshair", "overlay" },
    widgets = { w("zoomable_canvas"), w("node_graph"), w("whiteboard"), w("diagram"), w("map_view"),
      w("image_viewer"), w("chart_inspector"), w("timeline_track"), w("drawing_board") } },
  Dock = {
    role = { "group", "tab_list", "tab", "tab_panel", "splitter" },
    state = { "focused", "focused_panel", "maximized", "dragging", "drop_target", "drop_zone", "panel_count" },
    signals = { "on_layout_changed", "on_activated", "on_closed", "on_maximized", "on_focus_changed" },
    keys = { "ctrl_page", "ctrl_w", "ctrl_shift_m", "f6", "escape" },
    slots = { "tab", "stack", "divider", "floating", "drop_indicator" },
    widgets = { w("dock_area"), w("shelf_dock"), w("tabbed_container"), w("document_tabs"), w("tool_windows") } },
}

--- Every widget of a group (`archetypes`, `editor`), with its archetype.
function M.each(group)
  local out = {}
  local names = {}
  for archetype in pairs(M[group]) do names[#names + 1] = archetype end
  table.sort(names)
  for _, archetype in ipairs(names) do
    for _, entry in ipairs(M[group][archetype].widgets) do out[#out + 1] = { archetype = archetype, entry = entry } end
  end
  return out
end

return M
