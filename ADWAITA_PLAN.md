# PLAN: libadwaita parity in morf-kit

Every non-deprecated libadwaita widget and helper, rebuilt in morf. The
behaviour goes in Rust (`crates/engine/morf-kit`) and the look in Lua (a
theme's skin), so any theme can draw all of it. Each item says where the
work lives, what to build, how to prove it landed, and where to stop. Each
item lands as its own commits and keeps the tree green. The testing rules of
`PLAN.md` §0 apply unchanged.

Target: the libadwaita API reference at
`gnome.pages.gitlab.gnome.org/libadwaita/doc/main` (1.11.alpha when read,
2026-10-25).

## 0. What parity means here

The layers are the ones morf already has:

| Layer | Lives in | Owns |
| --- | --- | --- |
| archetype | `crates/engine/morf-kit` (Rust) | state, keys, pointer and gesture rules, signals, the slots a skin fills; no look, no Lua |
| widget / composite | `library/lib/kit` (Lua) | an archetype with settings (`widgets.lua`), or several wired together (`composites/`); layout only |
| skin | a theme (Lua) | every pixel: slots, tokens, transitions |

Rules:

1. **Names.** An Adw class is reachable by its name in snake_case, without
   the prefix: `AdwNavigationView` becomes `navigation_view` and
   `AdwToastOverlay` becomes `toast_overlay`. Where morf already has the
   thing under another name, the Adw name becomes the primary one and the
   old name stays as an alias. Archetype names (`Navigation`, `Shell`,
   `Popup`) stay as they are, since Adw names live at the widget level.
2. **API shape.** A property becomes a spec field, and also a state field
   when a skin must follow its changes. A signal becomes an `on_*` handler.
   A method becomes a call on the handle the constructor returns, so
   `adw_navigation_view_push_by_tag (view, "x")` becomes
   `view.push_by_tag("x")`. No GObject, no CSS engine, no `.ui` files.
3. **Logic in Rust.** Anything that makes a decision belongs to an
   archetype: a queue, a page stack, a collapse, a breakpoint, a snap
   point, which response is the default. A composite may lay parts out and
   forward events, but it holds no `morf.state` that a decision reads.
   Timers stay on the Lua side as they do today (`Press`'s `"repeat"`):
   the archetype asks for a timer, and the Lua timer answers with an event.
4. **Style classes are variants.** `.suggested-action`,
   `.destructive-action`, `.flat`, `.pill`, `.circular`, `.boxed-list`,
   `.card`, `.heading`, `.title-1`…`.title-4`, `.caption`, `.monospace`,
   `.numeric`, `.dimmed`, `.osd`, `.toolbar`, `.navigation-sidebar`,
   `.linked`, `.accent`, `.success`, `.warning` and `.error` map to widget
   variants or kit functions of the same name, in snake_case.
5. **Themeable means the contract.** Every new entry goes into
   `library/lib/kit/contract.lua` with an `adw` field naming its class.
   That way `lib.kit.check` holds every theme to it.
6. **Out of scope:**
   - deprecated classes and their enums: AboutWindow, Flap, Leaflet,
     LeafletPage, MessageDialog, PreferencesWindow, Squeezer, SqueezerPage,
     ViewSwitcherTitle, EnumListModel and EnumListItem;
   - GTK itself, since morf-kit's archetypes stand in for the GTK widgets
     Adw builds on;
   - CssClassBinding and `bind_property_to_css_class*`, since a reactive
     binding on a variant already does this;
   - pixel-identical output with GTK.

Effort: S ≈ days, M ≈ 1–3 weeks, L ≈ a month or more, for one engineer who
knows the tree. Risk is the risk of the change breaking morf or a shell.

---

## 1. Inventory and coverage gate

**Why.** "1:1" needs a count that can be checked, not an impression.

**Build.**

1. `library/lib/kit/adwaita.lua` holds one row per Adw class and enum:
   `{ class = "AdwNavigationView", since = "1.4", name = "navigation_view",
   archetype = "Navigation", status = "done" | "partial" | "missing",
   gaps = { ... } }`.
2. `lib.kit.check` gains an Adwaita report listing what is covered, what is
   partial (with each gap) and what is missing. A spec in `library/tests/`
   fails when a `done` row's name is missing from the contract or from the
   default skin.
3. Fill each row by reading the code, starting from the first reading
   below. Record `since` from the reference so that classes newer than the
   last stable release are visible.

**Proof.** The report runs in `oslo make test`, and its counts go into
§Progress.

### First reading

Read on 2026-10-25 from `contract.lua`, the morf-kit sources and the
composites. Step 1 confirms each row. "R" means the logic is in Rust, "L"
means it sits in a Lua composite.

**Windows and chrome**

| Adw | morf today | Gap |
| --- | --- | --- |
| Application, ApplicationWindow, Window | `lib.kit.app` `application` under `morf app`, `morf.window.floating` (L + host) | Adw names; breakpoints on the window (§5) |
| HeaderBar | `header_bar` composite (L) | centering policy, start/end title buttons, decoration layout, back button tied to `can_pop` (§6) |
| WindowTitle | inside `header_bar` | a display widget of its own |
| ToolbarView | Shell widget `toolbar_view` (R) | ToolbarStyle (flat, raised, raised border), revealing the top and bottom bars, content extending to the edges |
| Bin | any `ui.Item` | alias only |

**Adaptive layout**

| Adw | morf today | Gap |
| --- | --- | --- |
| Breakpoint, BreakpointBin, BreakpointCondition | Shell `breakpoints`: ascending widths only (R) | height and aspect-ratio conditions, `and`/`or`, px/pt/sp units, any container, setters applied and reverted (§5) |
| Clamp, ClampLayout, ClampScrollable | Shell widget `clamp` | maximum size and tightening threshold as a layout, scrollable form |
| MultiLayoutView, Layout, LayoutSlot | missing | §5 |
| WrapBox, WrapLayout | flex wrap in morf-layout, Collection `flow` layout | child and line spacing, JustifyMode, WrapPolicy, natural line length |
| NavigationSplitView | Shell widget (R) | sidebar width fraction with min and max, `show_content`, collapsing into navigation pages (§6) |
| OverlaySplitView | Shell widget (R) | `pin_sidebar`, sidebar position, show and hide swipes (§4, §6) |

**Navigation**

| Adw | morf today | Gap |
| --- | --- | --- |
| NavigationView, NavigationPage | Navigation `stack` mode (R) | pages with tag, title and `can_pop`; push and pop by tag, replace, pop on Escape, back swipe, `get_next_page` (§6) |
| ViewStack, ViewStackPage, ViewStackPages | Navigation `switcher` mode (R) | page title, icon, badge number, `needs_attention`, `visible`, homogeneous sizing |
| ViewSwitcher, InlineViewSwitcher | Selection widgets (R) | ViewSwitcherPolicy (narrow/wide), InlineViewSwitcherDisplayMode |
| ViewSwitcherBar, ViewSwitcherSidebar | missing | revealing on narrow, sidebar form |
| Sidebar, SidebarItem, SidebarSection | `sidebar` composite (L: Selection + Disclosure) | SidebarMode; logic into Rust (§6) |
| Carousel, CarouselIndicatorDots, CarouselIndicatorLines | `carousel` composite (L state) over Navigation `carousel` (R) and Drag swipe; `carousel_dots` | fractional position, spacing, wheel, long swipes, reveal duration; lines indicator (§8) |
| TabView, TabPage, TabBar, TabButton, TabOverview | `tab_view` composite (L) with reorder and an overview | pinned pages, close confirmation, indicator/loading/attention, TabViewShortcuts, transfer between views, TabButton (§9) |

**Rows and preferences**

| Adw | morf today | Gap |
| --- | --- | --- |
| PreferencesRow, ActionRow, SwitchRow, ComboRow, EntryRow, SpinRow, ExpanderRow, ButtonRow | `rows` variants (L layout over Press, Range, TextField, Disclosure and Selection, R) | per-row property parity, EntryRow apply state, ExpanderRow enable switch (§11) |
| PasswordEntryRow | TextField `echo = "password"` | the row under its name |
| PreferencesGroup, PreferencesPage | `rows.preferences_group`, `rows.preferences_page` | header suffix, description, separate rows, page icon and name for a dialog's switcher |
| PreferencesDialog | Popup widget `preferences_dialog` | search over rows, subpages, toasts inside it (§10) |

**Dialogs and sheets**

| Adw | morf today | Gap |
| --- | --- | --- |
| Dialog | Popup `dialog` (R) | DialogPresentationMode (auto, floating, bottom sheet by size), `can_close` with `close_attempt`, content size, `follows_content_size` |
| AlertDialog | Popup `alert_dialog` skin | responses (id, label, ResponseAppearance, enabled), default and close response, `choose` with a callback, extra child |
| AboutDialog | `about_dialog` composite (L) | release notes, credit and legal sections, troubleshooting, `show_about_dialog`, AppStream input |
| ShortcutsDialog, ShortcutsSection, ShortcutsItem, ShortcutLabel | `shortcuts_window` composite (L), `keycap` display | Adw names |
| BottomSheet | Popup `bottom_sheet`, modal only | a non-modal sheet with a bottom bar and drag handle, opening and closing by swipe, full width, align |

**Feedback**

| Adw | morf today | Gap |
| --- | --- | --- |
| Toast, ToastOverlay | `notification_stack` with `mode = "toast"` (L: timeouts, actions) | queue in Rust with ToastPriority, one toast shown at a time, timeout 0 meaning until dismissed, `dismissed` and `button_clicked` (§7) |
| Banner | display `banner` | `revealed` through Disclosure, button label, BannerButtonStyle |
| StatusPage | display `empty_state`, `result_page` | the Adw name and fields |
| Spinner, SpinnerPaintable | display `spinner` | the paintable form (a spinner as an image) |
| Avatar | display `avatar` with initials | colour from the text, custom image, rendering to an image |

**Buttons**

| Adw | morf today | Gap |
| --- | --- | --- |
| ButtonContent | Press slots `icon` and `label` | the name, `can_shrink`, `use_underline` |
| SplitButton | missing (`menu_button` composite exists) | two presses and one menu with shared focus and keys (§12) |
| Toggle, ToggleGroup | Selection `toggle_group` with Press `toggle_group_member` (R) | active by name, homogeneous, `can_shrink`, a Toggle's icon, label and tooltip |

**Style and motion**

| Adw | morf today | Gap |
| --- | --- | --- |
| StyleManager, ColorScheme, AccentColor | portal preferences in `morf-system/src/prefers.rs` (color scheme, contrast, reduced motion, accent colour); `skin.use` | one owner with an app override of the scheme, `dark`, `high_contrast`, accent as a named colour (§3) |
| Animation, TimedAnimation, SpringAnimation, SpringParams, Easing, AnimationTarget family | `morf.animation` tweens and springs (morf-scene, animato) | Adw parameter names, the full Easing set by Adw name, following reduced motion (§4) |
| SwipeTracker, Swipeable | Drag `swipe` mode, scroll fling | one tracker with snap points, overshoot, long swipes and mouse drag, shared by navigation, split views, carousel and sheets (§4) |
| LengthUnit, lerp, rgba_to_standalone | morf-value density, Color and HCT | sp units in breakpoints, standalone accent |

---

## 2. Names and aliases

**Why.** Several rows lack only the name. These are cheap, and they shrink
the inventory before any real work starts.

**Build.** For each one, add a contract entry with `adw`, an alias in
`widgets.lua` or `composites/`, and a default-skin entry that falls back to
the existing look:

- `bin`, `window_title`, `status_page`, `password_entry_row`;
- `shortcuts_dialog`, `shortcuts_section`, `shortcuts_item`,
  `shortcut_label`;
- `button_content`, and `application_window` and `window` over
  `lib.kit.app`.

**Proof.** Those rows turn `done`. Existing specs pass unchanged, and one
spec builds each widget by its Adw name.

## 3. Style manager

**Where.** `morf-kit/src/style.rs` decides; `morf-system/src/prefers.rs`
already reads the portal. The Lua side gets `lib.kit.style`.

**Build.**

1. **ColorScheme:** `default`, `force_light`, `prefer_light`,
   `prefer_dark`, `force_dark`. The system preference and the
   configuration's override resolve to `dark`, as AdwStyleManager does.
   Contrast resolves to `high_contrast`.
2. **AccentColor:** the nine named accents (blue, teal, green, yellow,
   orange, red, pink, purple, slate) with libadwaita's values, plus the
   standalone form for text on a plain background.
3. **Skins:** they read `style.dark()`, `style.high_contrast()` and
   `style.accent()` instead of flags of their own. A shell that keeps its
   own appearance setting (eva's `EVA_APPEARANCE`) sets the override.

**Proof.**
- morf-kit tests cover the resolution table.
- A spec on a private bus changes the fake portal's scheme and watches a
  skin rebuild.

## 4. Motion and the swipe tracker

**Why.** NavigationView, the split views, Carousel, BottomSheet and
TabOverview all ride one swipe model in libadwaita. Building it once, before
them, keeps their behaviour consistent.

**Where.** Animations already live in morf-scene and morf-runtime. The new
swipe tracker goes in `morf-kit/src/swipe.rs`, a state machine fed pointer,
touch and scroll events.

**Build.**

1. **Animation names:**
   - TimedAnimation: value from and to, duration, easing, repeat count,
     reverse, alternate.
   - SpringAnimation: damping ratio, mass, stiffness, epsilon, clamp,
     initial velocity, estimated duration.
   - Shared: play, pause, resume, reset, skip; AnimationState and `done`.
   - Map the parameters onto `morf.animation` and add the missing parts.
2. **Easing:** every libadwaita easing, by its Adw name.
3. **Reduced motion:** when it is on, an animation skips to its end, which
   is what `get_enable_animations` reports.
4. **SwipeTracker:**
   - Settings: snap points, distance, progress, orientation, `reversed`,
     `allow_mouse_drag`, `allow_long_swipes`, lower and upper overshoot.
   - Phases: begin, prepare, update, and end with a velocity, which picks
     the target snap point.

**Proof.**
- Rust tests pick the right snap point from a velocity.
- A spec swipes a stack back on the default skin.

## 5. Adaptive layout

**Where.**
- A new archetype in `morf-kit/src/breakpoint.rs` for breakpoints.
- Shell's width-only `breakpoints` rebuilt on top of it.
- Clamp and WrapBox as morf-layout behaviours, since they are layout and
  layout is already Rust.

**Build.**

1. **Conditions:** parsed in Adw's syntax (`max-width: 400sp`,
   `min-aspect-ratio: 4/3`, `and`, `or`, parentheses). Given the
   container's size and the text scale, the archetype answers which
   breakpoint applies; the last match wins, as in Adw.
2. **`breakpoint_bin`, and breakpoints on a window:** Lua passes
   `{ condition, set = { ... }, on_apply, on_unapply }`. Rust picks the
   breakpoint, and Lua applies and reverts the setters, which are bindings.
3. **`multi_layout_view` with `layout_slot`:** the active layout is
   selected by breakpoint, and slot children move between layouts through
   `ui.reparent`.
4. **Clamp:** maximum size and tightening threshold, plus the scrollable
   form.
5. **WrapBox:** child and line spacing, JustifyMode, WrapPolicy, natural
   line length.

**Proof.**
- Rust tests cover condition parsing and match order.
- A spec resizes a surface across breakpoints and checks what applies.
- Shell's existing specs pass unchanged.

## 6. Navigation

**Where.** `navigation.rs` (the stack mode gains page records), `shell.rs`
(the split views), and a Rust home for Sidebar.

**Build.**

1. **NavigationView:**
   - Pages are `{ tag, title, can_pop }`.
   - Calls: `push`, `push_by_tag`, `pop`, `pop_to_tag`, `pop_to_page`,
     `replace`, `replace_with_tags`, `find_page`, `visible_page`,
     `previous_page`.
   - Signals: `pushed`, `popped`, `replaced`, `get_next_page`.
   - Popping: on Escape, on Alt+Left and the back mouse button, and with
     the back swipe from §4.
   - `header_bar`'s back button reads `can_pop`.
2. **NavigationSplitView:** sidebar width fraction with min and max, and
   `show_content`. When collapsed it behaves as a two-page NavigationView.
3. **OverlaySplitView:** `collapsed`, `show_sidebar`, `pin_sidebar`,
   sidebar position start or end, width fraction, and the show and hide
   gestures.
4. **ViewStack:** page metadata. ViewSwitcher gets its policy and
   ViewSwitcherBar its revealing. ViewSwitcherSidebar is added.
5. **Sidebar:** SidebarMode, with items and sections as Rust state. The
   `sidebar` composite becomes layout only.

**Proof.**
- Archetype tests for each.
- Specs push, pop and swipe, and collapse both split views across a
  breakpoint.

## 7. Toasts and banners

**Where.** A new archetype in `morf-kit/src/toasts.rs`. The toast mode of
`notification_stack` becomes `toast_overlay` over it. Notification mode
stays as it is.

**Build.**

1. **Queue:** libadwaita's rules. One toast shows at a time. A normal
   toast waits its turn. A high-priority toast shows at once and sends the
   shown one back to the head of the queue.
2. **Toast:** title or custom title, button label, action, timeout in
   seconds (0 means until dismissed), priority, markup; `dismiss`,
   `dismiss_all`; `dismissed` and `button_clicked`. Pausing follows what
   `adw-toast-overlay.c` does.
3. **Banner:** `revealed` through Disclosure, a button label, and
   BannerButtonStyle.

**Proof.**
- Rust tests cover queue order under both priorities.
- A spec shows, queues and dismisses toasts with virtual time.

## 8. Carousel

**Where.** Navigation's carousel mode, or an archetype of its own if the
stack mode would be stretched too far.

**Build.**
- A fractional position for skins and indicators to read.
- Settings: `spacing`, `interactive`, `allow_mouse_drag`,
  `allow_long_swipes`, `allow_scroll_wheel`, `reveal_duration`, and spring
  scroll parameters (§4).
- `page_changed` and `scroll_to(page, animate)`.
- `carousel_indicator_lines` alongside the dots.
- The composite loses its `morf.state`.

**Proof.**
- Rust tests cover position and snapping.
- The existing carousel spec passes, and a new one swipes and wheels.

## 9. Tabs

**Where.** A new archetype in `morf-kit/src/tabs.rs` holds the page model.
TabBar, TabOverview and TabButton are skins and composites over it, using
Drag reorder and the Collection grid.

**Build.**

1. **Pages:** title, tooltip, icon, indicator icon (activatable or not),
   loading, `needs_attention`, pinned, keyword.
2. **The view:**
   - `selected`, `n_pages`, `n_pinned_pages`. Pinned pages come first, and
     a reorder stays within its part.
   - Inserting, pinning and reordering.
   - Closing through the `close_page` signal and `close_page_finish`
     (confirm or keep), plus closing the others or those before or after.
3. **Transfer:** pages move between views through Drag's transfer mode;
   `create_window` asks for a new one.
4. **Shortcuts:** per the TabViewShortcuts bitfield.
5. **TabBar:** autohide, expand tabs, inverted, start and end actions.
6. **TabOverview:** open, search, a new-tab button.
7. **TabButton:** page count and attention.

**Proof.**
- Rust tests cover pinned ordering, close confirmation and shortcuts.
- The existing `tab_view` specs pass on the new model.

## 10. Dialogs and sheets

**Where.** `popup.rs` gains presentation modes, and alert-dialog responses
get a home of their own in morf-kit.

**Build.**

1. **Dialog:**
   - Presentation is `auto`, `floating` or `bottom_sheet`. `auto` switches
     at the sizes `adw-dialog.c` uses and follows §5's breakpoints.
   - `can_close` with `close_attempt`, content width and height,
     `follows_content_size`.
2. **AlertDialog:**
   - Responses carry id, label, appearance (default, suggested,
     destructive) and enabled.
   - A default response and a close response, heading and body, and an
     extra child.
   - `choose(callback)`.
3. **PreferencesDialog:** search across rows, `push_subpage` and
   `pop_subpage` (on §6), and toasts (on §7).
4. **AboutDialog:** the full field set, plus `show_about_dialog`. Reading
   AppStream data is an open question.
5. **BottomSheet:** the persistent kind, with `open`, `modal`, a drag
   handle, `can_open` and `can_close` by swipe (§4), a bottom bar that
   reveals, full width and align.

**Proof.**
- Rust tests cover response rules and presentation choice.
- Specs open each dialog at a wide and a narrow size.

## 11. Rows

**Where.** Mostly Lua layout in `rows.lua`, over archetypes that already
exist. Rust changes in two places only:
- TextField gains EntryRow's apply state: `show_apply_button`, and `apply`
  fires when the text differs from the last value applied.
- Disclosure gains ExpanderRow's `enable_expansion` and its switch.

**Build.** Property parity per row:

| Row | Properties |
| --- | --- |
| ActionRow | subtitle, title and subtitle line limits, selectable subtitle, activatable widget, prefix and suffix |
| ComboRow | model, selected, `use_subtitle`, `enable_search`, a label function in place of the GTK expression |
| SpinRow | climb rate, digits, numeric, snap to ticks, update policy, wrap |
| SwitchRow | `active` |
| ButtonRow | start and end icons |
| ExpanderRow | subtitle lines, `show_enable_switch`, prefix, suffix, child rows |
| PreferencesGroup | header suffix, description, separate rows |

**Proof.** A spec per row against the default skin, checking each property
and signal.

## 12. Buttons

**Build.**
1. **SplitButton:** two presses sharing focus and keys, opening a menu
   Popup; `clicked`, dropdown tooltip, direction.
2. **ButtonContent:** `can_shrink`, `use_underline`.
3. **ToggleGroup:** parity with Adw's (active by name or index,
   homogeneous, `can_shrink`, each Toggle's icon, label and tooltip).

**Proof.** Rust tests for SplitButton's keys, and one spec per widget.

## 13. The adwaita skin

**Where.** `library/lib/kit/skins/adwaita`, extending `default`. A gallery
app in `examples/apps/adwaita` mirrors the pages of the Adwaita Demo
(`adwaita-1-demo`).

**Build.**
- **Tokens from libadwaita's stylesheet sources:**
  - colours for light, dark and high contrast;
  - the accent from §3;
  - radii and spacing;
  - the type scale (`title_1`…`title_4`, `heading`, `body`, `caption`);
  - control heights.
- **Skins:** one for every contract entry that has an `adw` field.
- **Timing:** it grows alongside the items above, starting after §3.

**Proof.**
- `morf render` draws each gallery page in light, dark and high contrast
  into snapshot specs.
- A side-by-side check by eye against the Adwaita Demo. Differences are
  written down here, not chased pixel by pixel.

## 14. Themes downstream

The contract's `M.stage` makes every kit implement each entry at or below
it. New entries land at stage 23 or later, one stage per item.

`M.stage` rises only when all four kits pass `lib.kit.check`: `default`,
`adwaita`, and eva's material and tsugumori. eva tracks its share of the
work in its own repository.

## Sequencing and dependencies

| # | Item | Effort | Risk | Needs |
| - | --- | --- | --- | --- |
| 1 | Inventory and coverage gate | S | Low | — |
| 2 | Names and aliases | S | Low | 1 |
| 3 | Style manager | S–M | Low | 1 |
| 4 | Motion and the swipe tracker | M | Medium | 1 |
| 5 | Adaptive layout | M | Medium | 1 |
| 6 | Navigation | M–L | Medium | 4, 5 |
| 7 | Toasts and banners | S–M | Low | 1 |
| 8 | Carousel | S–M | Low | 4 |
| 9 | Tabs | M–L | Medium | 4 |
| 10 | Dialogs and sheets | M | Medium | 4, 5, 6, 7 |
| 11 | Rows | S–M | Low | 2 |
| 12 | Buttons | S | Low | 2 |
| 13 | The adwaita skin | L | Low | 3, then grows with each |
| 14 | Themes downstream | ongoing | Low | each stage |

Items 7, 11 and 12 depend on nothing but the inventory and can run in
parallel with 4–6.

## Open questions

1. **Target version.** Gate on the last stable libadwaita and track
   main's additions as `since` rows, or gate on main?
2. **Clashing names.** morf's `Sheet` archetype is a spreadsheet grid,
   while Adw's BottomSheet is a sheet. The `sidebar` composite predates
   AdwSidebar. The proposal is to keep the archetype names and use Adw
   names for widgets only.
3. **AboutDialog from AppStream.** Parse AppStream XML in Rust
   (morf-system), or leave `from_appdata` out?
4. **eva's settings pages.** Should eva's settings pages
   (`lib.util.settings_pages`) move onto NavigationView once §6 lands?

## Progress

Nothing landed yet.
