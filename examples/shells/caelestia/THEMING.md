# Whole-shell visual themes — implementation and verification

The requested end state is a themeable shell, lock screen and greeter, with
Material preserved and a complete Tsugumori-inspired alternative. Wallpaper
colors remain independent of the visual theme. This document tracks unfinished
work; the existence of a theme manifest is not completion.

## Current scope correction — shared content structure

The user's clarification supersedes the earlier independent-layout work:
**themes must preserve Material's content grouping, order and controls**.
Useful titles are permitted, and belong in both themes. The approved edge
pills stay. Independent content redesign is not a theme requirement.

Both manifests now select `themes/layouts/views/*` for all content views.
The Material view paths and former Tsugumori view paths are compatibility
wrappers. Lock and greeter also use shared layouts; their skin adapter changes
constructors rather than rearranging authentication controls. Tab navigation
uses the shared container, with theme-selected faces and page motion.
Controls, Playback and Capture headings are shared additions. Tsugumori tab
icons all occupy the trailing position, including Lule's custom SVG.

The historical verification notes below describe earlier implementations;
they do **not** prove the restored shared layouts. Tests asserting the rejected
Tsugumori-specific grouping/compact arrangements need migration. Current
verification is tracked by `shared_layout_spec.lua` (builder selection,
rendered section geometry, lock/greeter geometry), Material behavior tests,
and focused motion/appearance checks. Full parity, compact behavior and the
broader authentication/package audit remain unfinished.

The four shared-layout cases now pass with GPU rendering: 14 shell states
per theme, every dashboard tab's trailing icon, and lock/greeter sheet and
field geometry. Captures: `/tmp/morf-shared-layout-final`. Fourteen focused
Material behavior cases, five palette/authentication cases and two hover/tab
motion cases also passed. The full-shell check has no errors and retains the
previously reproduced empty-layout warning. The private visible Cage preview
was refreshed and the complete trailing-icon row inspected. No installation
or real-shell restart was performed.

## Boundaries

- `shell/palette.lua`: wallpaper/Lule color roles; no layout selection.
- `themes/layouts/`: common content composition for all themes.
- `themes/<name>/`: visual tokens, widget constructors, decoration and motion.
- Shell controllers: service connections, navigation, history and actions.
- Authentication controllers: password handling, PAM/greetd, accounts and
  sessions. Visual themes receive signals and callbacks, never passwords.
- `themes/init.lua`: shared selection and package validation. Source parts
  link to this directory; installation copies dereferenced links so each part
  stays runnable independently.

## Required work

- [ ] Preserve Material's complete appearance and interactions after extraction.
- [ ] Replace hardcoded view construction across all shell surfaces with theme
  builders: frame, rail/bar, launcher, dashboards, connectivity/settings,
  notifications/OSD, session/auth dialogs, capture, planner, bottom workspace,
  Lule and on-screen keyboard.
- [ ] Separate lock and greeter controllers from their theme views, including
  multi-monitor lock, password/pattern, account/session selection and OSK.
- [ ] Implement a coherent Tsugumori-inspired visual theme: framed geometry,
  typography, control design, layout and animation, including lock and greeter.
- [ ] Both themes use wallpaper palettes, verified with contrasting accents.
- [ ] Document theme selection and adding a third theme; preserve source,
  standalone installation and bundle module resolution.
- [ ] Run behavior tests for both themes, inspect rendered views and exercise
  shell/lock/greeter in isolated Cage sessions at desktop and compact sizes.
- [ ] Audit every surface and requirement against source and runtime evidence.

## Isolation

Do not install, replace user configuration, restart the user's shell, modify
Hyprland, or invoke real power/authentication actions while developing this.
Use scratch settings/cache/state and the existing sandbox's private runtime,
bus and command stubs. Lock checks use `window preview`; greeter checks must
not connect to the user's greetd socket.

## Border and spacing refinement

Left workspace markers now use six-pixel-wide vertical pills matching the
right-edge marker dimensions. Occupied workspaces have stronger accent ink;
the selected pill slides between positions. The dot/arrow treatment is gone.
All five frame/rail cases passed with GPU captures in
`/tmp/morf-left-pills-films`, and the isolated visible preview was refreshed.

Tsugumori decoration uses `strokes.lua`: quiet and idle borders are the
wallpaper primary at 10% and 18% opacity. Focus/hover remain stronger; errors
keep their semantic color. Palette text roles are unchanged. Shared cards use
smaller one-pixel corner marks. Dashboard detail pages have no enclosing
frame; dashboard, side and bottom containers use an eight-pixel inset, and
Lule no longer adds a second page inset. Material keeps its own geometry.
The 32 dashboard/control/Lule/side/bottom/auth-desktop cases passed with GPU
captures in `/tmp/morf-soft-borders-films`; another 40 settings/planner/auth
popup/notification/keyboard cases passed headlessly. The 500×720 full-shell
Lule check reports zero errors and warnings. The isolated visible preview
was refreshed and its live Lule rendering inspected.

## Shared text roles and presentation

Use `kit.heading` for pane, section and graph titles, `kit.subtitle` for the
description beneath them, `kit.section_label` for form labels and `kit.menu_label`
for selectable row names and menu choices. Ordinary
readouts (clocks, percentages, addresses) stay `kit.text`. Button labels keep
their own feedback. Do not implement a separate title animation in a page.

Tsugumori's `typography` token table defines title/section/caption/hero sizes
(16/14/11/24), subtitles (12), labels (11) and menu names (13). All heading levels share the
Mara-inspired decode, color registration flash and finite light pulses.
Material renders these roles through its existing text styles.
Heading ink defaults to the palette's primary color; `ink` is an explicit
contrast override for filled setting tiles and urgent notifications. Menu
descriptions use the same subtitle role as panel descriptions.
Media track titles and empty-state titles also decode; entering an empty state,
changing tracks and reopening the media page each starts a fresh reveal. Menu
names retain their existing interaction feedback and share a single weight/size.
Audio, networking, power, bar, planner and wallpaper rows use these same roles.
Tab captions and action buttons also use the menu role (13px/500), while their
numbers use the label role. Their existing finite hover rolls remain; UTF-8
captions are measured by characters and clipped within the available slot.

A heading must receive `active = function() ... end` or `scope = "panel.page"`.
`presentation.lua` publishes controller visibility without importing a
partially constructed controller into a view. Drawers publish their name;
tabbed pages publish `bottom.assistant`, `bottom.drop`, `leftbar.tasks`,
`leftbar.calendar`, `sidebar.settings` and `sidebar.notifications`. Dashboard
and settings subpages publish their own scopes. Closing cancels the decode;
reopening or entering a new page starts it again. Lock/greeter supply their
authentication sheet state directly.
Nested notification groups and task editor/empty states also gate their own
headings, so hidden content does not finish decoding before it is revealed.

Scrollable sections can also supply `viewport = function() return scroll end`.
The shared heading watches resolved geometry, starts decoding when it intersects
that viewport, and cancels when it leaves. Calendar, Power and Bar sections use
this contract. It adds no polling timer at rest. Text alignment and elision are
preserved, and intrinsic heading width counts UTF-8 characters rather than bytes.
Tsugumori builders can wrap construction in `kit.with_viewport(get_view, build)`
to apply this rule to every nested heading, including graph captions. The
dashboard wraps both overview content and detail page construction; page-local
viewports add to the surrounding clipping chain. A title starts only when the
intersection of all its viewports is visible, so compact Battery, Performance,
Weather and Media pages no longer finish their offscreen titles in advance.
The construction context is restored after each builder, including errors.
`heading_spec.lua` covers nested clipping and sibling isolation; the compact
dashboard tests check Battery graph and Weather forecast decode after scrolling.
This dashboard/typography follow-up passes 17 unique cases: four heading,
eight dashboard, two shared-control and three full-shell heading cases. The
first 14 also pass with GPU snapshots in `/tmp/morf-unified-heading-films/`.
The refreshed isolated preview captures both decoding and settled Dashboard
and Assistant titles. The 500x720 Tsugumori shell check has no errors or
warnings; Material has the same single zero-size-child warning as its saved
pre-extraction baseline, with no errors. No installed-shell or user settings
were changed.
Planner descriptions and task row details use the subtitle role; form labels
remain section labels. `planner_typography_spec.lua` covers both themes at a
compact size, including scrolling to Work calendar and reopening there.
VPN section headings and notification group/summary/expanded titles also
follow their scroll viewport. Launcher sections follow its selection-driven
list offset: that list uses native translation, which is not part of the
heading's layout coordinates. Wallpaper result names use the same menu role
as other selectable rows. `network_typography_spec.lua` exercises compact VPN
sections in both themes; the history and launcher cases check scrolled title
replay and settled offscreen titles. GPU captures for this follow-up are under
`/tmp/morf-unified-title-films/`.
This follow-up passes 33 unique cases across heading, panel-heading, planner,
network typography, history, launcher and Settings suites. All 13 network,
history and launcher cases also pass with GPU snapshots (the corrected compact
launcher replay case ran separately). Both themes' full-shell Tunnel checks
report zero errors and warnings. The isolated visible preview was refreshed
and inspected with Calendar, Tunnel and Assistant open; the installed shell
and user configuration remain untouched.
The three heading cases, four planner cases and three full-shell title cases
pass. The Settings case also checks Wired, Mesh, Tunnel, Tor and Sound's nested
headings. Compact GPU captures are in `/tmp/morf-planner-typography-films/`;
the refreshed private Cage preview was inspected with Calendar and Settings
open together. Both themes' Calendar checks report zero errors and warnings.

`panel_headings_spec.lua` exercises title replay across main panels and settings
subpages. Its three cases and the five history cases pass together after the
history extraction. `heading_spec.lua` checks interrupted reveals and UTF-8 text. These
checks do not establish completion of the other theme boundaries above.

## View extraction progress

- Launcher: shared search/navigation/actions controller; independent Material
  and Tsugumori builders. The latter fits compact outputs and renders five
  nearby wallpaper previews, including selections beyond item 64.
- Media: shared audio monitor, playback actions, lyrics and player model;
  independent Material and Tsugumori layouts.
- Weather: `weather_model.lua` owns readings, units, date formatting and forecast
  ranges; both themes receive that model. Material keeps its existing cards
  and geometry (the before/after fixture PNGs are byte-for-byte identical).
  Tsugumori has separate current conditions, detail readings and a forecast
  register with low/high temperature ranges. Below 660 px it stacks vertically.
  Five focused cases cover both themes' missing/partial data, metric/imperial
  units, three-day fallback, hidden reads, title replay and compact scrolling.
  Dashboard integration also verifies fitting the available viewport and
  reaching the final forecast row. Narrow dashboard navigation now reveals
  the selected tab automatically.
  Private Cage runs verified desktop entry/interrupted reopening and actual
  wheel scrolling to the final forecast at 500×720. Captures are under
  `/tmp/morf-weather-cage-verify/out/morf-weather/` and
  `/tmp/morf-weather-compact-cage/out/morf-compact/`. The seven dashboard
  cases, four Material dashboard cases and main-panel title replay case pass
  after this change; both closed full-shell checks report zero errors/warnings.
- Battery: `battery_model.lua` owns readings, duration/limit formatting, device
  history lookup and voltage bounds. The controller owns navigation; each
  theme builds its page. Material's fixture PNG is byte-for-byte unchanged.
  Tsugumori has a segmented charge bar, readings, four instrument graphs and
  device facts. Graphs and facts reflow to one column when space is limited.
  Its graph builder supports reactive dimensions, while both graph factories
  receive the shared sample limit rather than importing the sampling service.
  Seven battery cases cover readings, source failure, missing fields, actual
  curve bounds, resizing, interrupted entry and hidden UI reads. The background
  test uses the existing ring implementation and verifies continued collection
  with exactly 60 allocated sample slots. Eight dashboard cases and main-panel
  title replay also pass. In live Cage, battery sample counters advanced from
  3 to 5 while closed and reached 10 after closing again; the limit remained 60.
  Desktop and 500×720 captures are under `/tmp/morf-battery-cage-verify/out/morf-battery/`
  and `/tmp/morf-battery-compact-cage/out/morf-compact/`. The compact run scrolled
  through all graphs to the final device fact.
- Dashboard container: shared navigation, calendar and readings model;
  independent Material overview and Tsugumori overview-to-detail composition.
  Overview cards fade out when entering a detail tab. Detail content fills
  the panel body, with no persistent summary column or duplicate back button;
  the Dashboard tab returns to the overview. Entry and page covers cancel
  cleanly on new navigation. The compact overview scrolls, and detail pages
  remain reachable through their own viewport.
  Calendar and playback actions, hidden readings, rapid navigation, resize
  geometry and compact scrolling have focused coverage. The four existing
  Material dashboard cases and three title replay cases also pass.
  Private Cage captures exercise real Performance, Weather and Lule pages in
  `/tmp/morf-dashboard-cage-verify/out/morf-dashboard/`.
  Live resizing exposed a partial-repaint artifact absent from fresh GPU
  snapshots: the stationary middle tab disappeared until hovered. Compositing
  the navigation strip together avoids it; the live before/after reproducer is
  `/tmp/morf-dashboard-live-diag/out/morf-{diag,layered}/`. The underlying
  renderer behavior remains to be isolated independently.
- Performance: `performance_model.lua` owns device discovery, bounded cached
  history reads, selection and address probes; `performance_readouts.lua`
  supplies formatted readings. The Material view retains its existing plot
  geometry. Tsugumori has independent responsive device navigation, readings,
  plots and facts. Seven focused cases cover all device kinds, stale address
  replies, removed devices, hidden reads, rapid selection and compact scrolling.
  Material image parity and a dedicated live Performance review remain pending.
- Bottom workspace: `bottom_model.lua` owns requested/displayed navigation,
  visibility and page contexts. Material preserves its dimensions, sliding tabs
  and page geometry; Assistant and Drop fixture PNGs are byte-for-byte unchanged.
  Tsugumori has independent container and page builders: a conversation workspace
  and separate message/file areas. Compact pages scroll vertically, navigation
  stays visible, and page entry stops when hidden. Five focused GPU cases cover
  navigation, covered swaps, title replay, interrupted closing, resizing, compact
  reachability and alternate wallpaper colors. These remain honest placeholders;
  no provider or Drop integration is started by a visual theme.
  Both full-shell dismissal cases and the main-panel title replay case pass.
  The hover-close assertion now allows the 120 ms grace plus Tsugumori's 780 ms
  covered departure; it still checks the controller's closed state first.
  Live Cage verified desktop layout, 500×720 scrolling to both final sections,
  interrupted reopening and outside-click dismissal. Compact captures are under
  `/tmp/morf-bottom-compact-cage/out/morf-compact/`; the visible preview is refreshed.
  Full-shell checks with Drop open report zero errors/warnings for both themes.
- Settings overview and navigation: `settings_model.lua` now owns service
  readings, toggle actions, command expansion, idle inhibition, navigation and
  presentation scopes. `utilities.lua` composes detail builders with the selected
  visual package. Material preserves the old overview and sliding details;
  before/after fixture PNGs for both are byte-for-byte identical. Tsugumori has
  independent connection/device/attention groups, a scrollable overview and
  covered detail changes. Offscreen control titles decode when scrolled into
  view. Closing retains the outgoing detail until the next presentation, avoiding
  an overview flash during departure.
  Ten focused cases cover service actions, dry-run suppression, hidden reads,
  navigation, capture handoff, title replay, compact scrolling and VPN watcher
  lifetime. Four existing Material navigation cases and Settings title replay
  pass. The shared tab controller now publishes actual page activity for both
  themes; changing a requested tab no longer starts Settings activity before its
  cover reveals it. VPN polling stops when its detail is hidden, even if the
  displayed key remains during departure. Settings volume/brightness, Tor and
  ring-mode actions now honor the existing dry-run boundary.
  Five bottom-workspace cases also pass after adding Material tab scopes.
  Both full-shell Settings checks report zero errors/warnings. The refreshed
  visible Cage preview shows the desktop overview; a 500×720 run verified the
  final attention control, Sound's final card, switching away from Mesh,
  interrupted reopening and outside-click dismissal (`sidebar state` returned
  false). Captures are under `/tmp/morf-settings-compact-cage/out/morf-compact/`.
  The remaining detail pages and sidebar container still need independent theme
  layouts and further service extraction. Sound and Microphone are now extracted
  separately, below; the temporary minimum height for Sound has been removed.
- Sound and Microphone: `sound_model.lua` owns native audio snapshots, device
  and stream identity, volume/mute/default/channel actions and app routing.
  The controller creates one model per page and injects it into the selected
  builder. Hidden pages retain a snapshot and unsubscribe from reactive audio
  reads; no polling timer or level monitor is added. Dry-run suppresses every
  audio action. Removed/reused IDs are rejected, and replacement rows receive
  fresh UI callbacks while ordinary volume/display-name updates preserve row identity.
  Material's output and input fixture PNGs are byte-for-byte unchanged. Tsugumori
  has independent level/balance, device selection and application routing
  sections. All channels, outputs, apps and routes remain reachable through its
  own viewport; routing chips wrap into a grid instead of being clipped. Titles
  decode on entering the viewport, and entering the page resets scroll position.
  Ten audio cases cover both themes' actions, filtering monitor inputs, dry run,
  hidden reads, empty/failed servers, replaced/reordered rows, stale controls,
  12 channels and the final route in a compact pane. The ten Settings cases,
  existing Material Sound navigation case and Settings title case also pass.
  Compact GPU captures are in `/tmp/morf-sound-final-films/`. Both full-shell
  checks report zero errors/warnings. Private Cage captures at 500×720 are in
  `/tmp/morf-sound-compact-cage/out/morf-compact/`; the populated review uses a
  private fake audio backend, so it cannot change real devices. That run reached
  the final route, switched to Microphone, interrupted closing with a Sound
  reopen, verified return to the level controls and dismissed by clicking outside.
  An apparent missing-label issue in image previews was investigated alongside
  Connectivity below. Do not infer missing pixels from a preview alone.
- Capture: dedicated Tsugumori builder.
- Wi-Fi and Bluetooth: `connectivity_model.lua` owns service snapshots,
  connection/strength/pairing order, unnamed-device filtering and all actions.
  Hidden pages retain cached rows without reading their services. Commands
  resolve the current row before acting; network selection follows a changed
  access-point path for the same network. Dry-run suppresses radio changes,
  discovery, connection and external settings launches. Request failures are
  exposed to the view; superseded replies and replies after closing are ignored.
  Material's Wi-Fi and Bluetooth fixture PNGs match the originals byte for byte.
  Tsugumori has separate framed radio controls, state descriptions and a
  scrollable register, with no 20-row cap. Heading reveals follow the viewport,
  and reentry returns to the controls. Nine focused cases cover both themes'
  controls, ordering, hidden reads, dry-run, failure/late replies, stale devices
  and compact reachability; the ten Settings cases and Settings heading case
  also pass. Both full-shell checks report zero errors/warnings.
  GPU captures with an alternate wallpaper accent are under
  `/tmp/morf-connectivity-final-films/`. The private 500×720 Cage review used
  fake radio services, reached both final rows beyond item 20, switched pages,
  reopened Bluetooth during departure and dismissed outside (`sidebar state`
  returned false). Captures are under
  `/tmp/morf-connectivity-compact-cage/out/morf-compact/`. The visible preview
  is refreshed. A missing RADIO label was initially reported from image
  previews; raw pixel inspection disproved that report. The original reopened
  PNG contains the complete label. Its 35×15 region at (15,82) is byte-identical
  to a fresh render under `/tmp/radio-original-repeat/`. Variants removing
  scrolling, palette changes, closing and earlier snapshots also contain the
  label. No snapshot/renderer-reuse defect was established and no renderer patch
  was made. Future visual discrepancies need pixel evidence before diagnosis.
- Session: shared command dispatch, account metadata and keyboard navigation;
  separate Material geometry/morphing and Tsugumori framed action register.
  Both views pass isolated navigation, dry-run, command dispatch and dismissal
  checks; Tsugumori also passes compact sizing and interrupted reopening.
  The live isolated preview was inspected with its wallpaper-derived cyan
  palette; GPU fixtures also cover the default pink and a purple palette.
- Authentication prompts: polkit request routing, agent registration, hidden
  password input and secret lifetime stay in `shell/polkit.lua`. Material and
  Tsugumori each build their own prompt, field, status badge and failure motion.
  Themes receive masked counts and metadata, never the input node or password.
  Authentication markers likewise keep the watcher/state in `shell/authsteps.lua`
  and use independent visual builders. Hidden marker animations stop.
  Both themes' simulated flows and lock/greeter checks pass. A separate private
  Cage run exercised the Tsugumori prompt, fingerprint marker and dismissal;
  screenshots are under `/tmp/morf-auth-cage-verify/out/morf-auth/`.
- Notification popups: shared server/history/expiry/expansion controller;
  independent Material cards and Tsugumori framed stack with bounded scrolling.
  Dismissal retains notification identity across new arrivals. Expansion uses
  string keys (the signal API does not accept sparse numeric tables) and drops
  stale expansion state when a popup expires.
- Volume/brightness OSD: shared readings and timeout controller; separate
  Material edge discs and Tsugumori framed readouts. Both cancel stale close
  completions and recover when another update arrives during dismissal.
  Notification/OSD focused suites pass 12 cases. The combined private Cage run
  verified popup placement, volume, brightness and a clean return to idle;
  captures are under `/tmp/morf-popup-cage-verify/out/morf-popups/`.
- Notification history: grouping, expansion and actions live in
  `shell/notification_history.lua`; the sidebar only manages navigation and
  popup coverage. Material preserves its grouped cards. Tsugumori uses a framed
  register, consistent text roles and a scrollable list on compact screens.
  Clear captures notification IDs before departure; arrivals during that
  animation survive. Expansion state is pruned when an application's last
  notification disappears. Five focused cases pass for both presentations,
  including title replay, copy/dismiss, interrupted clearing and compact scroll.
- Frame and workspace rail: independent Material and Tsugumori views. Material
  retains its rounded opening and morphing discs; Tsugumori supplies a straight
  opening with registration corners, workspace ticks and a finite framed
  workspace indicator. Both preserve panel stacking, edge input and bar insets.
  Five focused GPU cases cover those contracts, rapid workspace changes,
  disabling during animation and a compact viewport. A private Cage run also
  verified workspace indication, sidebar history and the bottom assistant
  together; captures are under `/tmp/morf-frame-history-cage/out/morf-history/`.
  The separate visible preview was refreshed and inspected at 3410×2114.
- Power and Bar settings: `power_model.lua` owns cached battery/profile readings,
  supported-mode checks, profile errors and graph handoff; `bar_settings_model.lua`
  owns preference choices and validated writes. Both pages now have independent
  Material and Tsugumori builders. Material's two fixture PNGs are byte-for-byte
  identical to the pre-extraction versions. Tsugumori uses vertical power modes,
  segmented charge with a limit marker, a battery facts register, and a desktop
  placement preview with two-column edge choices. Both pages scroll at compact
  sizes, reset on entry and use the shared viewport-aware headings and motion.
  Nine focused cases cover readings, missing batteries, profile availability,
  removed modes, service errors, dry-run/hidden guards, preferences and compact
  reachability. Those cases, ten Settings cases and the full-shell Settings
  heading case pass; all nine focused cases also pass with GPU captures.
  Both themes' full-shell Power and Bar checks report zero errors/warnings.
  Private Cage at 500×720 verified scrolling to the final controls, interrupted
  reopening, right/bottom bar placement and outside-click dismissal. Captures
  are in `/tmp/morf-power-bar-compact-cage/out/morf-compact/`; the visible preview
  is refreshed. Preview profile data is fake and preferences use scratch files.
- Wired, Mesh, Tunnel and Tor settings: `net_pages_model.lua` now owns normalized
  connection rows, identity checks, commands, cached readings, request feedback
  and VPN polling lifetime. The controller selects an independent Material or
  Tsugumori builder. Material preserves the four populated fixture PNGs and
  both missing-service PNGs byte-for-byte. Tsugumori has separate connection
  registers with room for details, action rows, finite entry motion and shared
  viewport-aware headings. Compact lists scroll without a fixed row limit and
  reopen at the top. A completed empty VPN scan is distinguishable from loading
  through `lib.vpns.loaded`; Tor ignores overlapping start/stop requests.
  Fourteen focused cases cover both themes, stale/removed connections, daemon
  disappearance, delayed errors, hidden reads, dry-run commands, Tor progress,
  empty services, long lists and the real library's empty-scan signal. The two
  network typography cases, ten Settings cases and full-shell Settings title
  case also pass. GPU fixtures are in `/tmp/morf-net-pages-films/` and private
  500×720 Cage captures in `/tmp/morf-net-pages-compact-cage/out/morf-compact/`.
  Cage verified the last wired/VPN rows, reopening, Mesh, Tor progress and
  outside-click dismissal with fake services; no real connections were changed.
  The visible preview was refreshed and inspected with Tunnel open. Full-shell
  Material Tor and Tsugumori Mesh/Tunnel checks report zero errors and warnings.
- Tasks and Calendar: `tasks_model.lua` and `calendar_model.lua` own filtering,
  drafts, selected dates, month/agenda data and Taskwarrior callbacks. Independent
  Material and Tsugumori builders receive these models. Material's task list,
  editor and calendar fixture PNGs remain byte-for-byte identical. Tsugumori
  uses framed task rows, grouped editor sections, a fixed Save button and a
  scrollable month/agenda layout. All headings use the shared decode and viewport
  visibility; descriptions, field labels and task rows use the shared text roles.
  The model preserves unchanged dates, checks current task identity before
  writes, ignores an old save completion after opening another draft, and blocks
  mutations in preview mode. Calendar hands the selected day to a new task at
  09:00 local time. Hidden pages stop deriving rows from the task list.
  Nine focused cases pass on CPU and GPU, including both themes' filters,
  edits, completion/start/delete actions, calendar handoff, stale tasks, hidden
  reads, dry-run guards and long compact lists/forms. The four planner typography
  cases and all three full-shell heading cases also pass. Material Tasks and
  Tsugumori Calendar full-shell checks report zero errors and warnings.
  GPU fixtures are in `/tmp/morf-planner-theme-films/`. Private 500×720 Cage
  captures verify typing into the Project field without moving focus, the final
  editor fields, long agendas, Work calendar and reopening at the top.
  These use a fake Taskwarrior client; no personal tasks are changed.
  The shared left drawer now has the missing outside-click handler, preserving
  clicks in blank panel space. Both themes have a full-shell regression case,
  including disabled edge hover. Private 800×720 Cage verifies ordinary
  outside dismissal of the editor and calendar after moving the pointer;
  captures are in `/tmp/morf-planner-dismiss-cage/out/morf-moving/`.
  A separate live-input issue remains to investigate: reopening by IPC while
  the pointer stays over the desktop can leave its next stationary click
  ineffective. Moving into the panel and back outside restores dismissal.
  The pointer-containment state is correctly false in that case; headless
  interaction tests do not reproduce it. The reproducible Cage sequence and
  state captures are in `/tmp/morf-planner-stationary-cage/`.
- Side-panel containers: `side_panel_model.lua` owns selected/displayed tabs
  and presentation scopes. `leftbar.lua` owns Taskwarrior polling lifetime;
  `sidebar.lua` owns notification coverage. The selected `side_panel` builder
  owns drawer dimensions, page composition and tab transitions. Material's
  legacy tab implementation now lives in `themes/material/tabbed.lua`, with a
  compatibility constructor in `shell/tabbed.lua`. All four Material fixture
  captures (Tasks, Calendar, Settings, Notifications) match the pre-extraction
  PNGs byte-for-byte. Tsugumori presents the next page beneath its cover and
  blocks clicks on the outgoing page immediately after selection changes.
  Switching planner tabs no longer invokes `client.watch(true)` again.
  Five focused cases pass on CPU and GPU, covering geometry, presentation,
  service lifetimes, invalid/stale selection, interrupted transitions, compact
  resizing and click suppression. Both themes' full-shell planner dismissal
  cases, the two existing Tsugumori control cases and all three full-shell
  heading cases also pass (12 unique cases overall). Full-shell
  Material Calendar and Tsugumori Notifications checks report zero errors and
  warnings. Private 800×720 Cage verifies rapid tab changes, private notification
  history, outside dismissal and interrupted reopening; screenshots are in
  `/tmp/morf-side-panels-cage/out/morf-sides/`. GPU comparison fixtures are in
  `/tmp/morf-side-panels-films/` and `/tmp/morf-side-panels-baseline-films/`.
- Bar: `bar_model.lua` owns cached window/status/clock readings and validated
  actions; `bar.lua` owns preference policy and desktop reservations using the
  selected view's dimensions. Material and Tsugumori have independent builders.
  All four Material edge captures match their pre-extraction PNGs byte-for-byte.
  Tsugumori uses framed controls, bounded scrollable window lanes, automatic
  reveal of the focused window and compact clock/status layouts. Its shared
  entry motion cancels on hiding or changing orientation. Hidden bars stop
  deriving service data; the clock uses the shared minute signal instead of an
  always-running one-second timer. Focus actions revalidate window identity,
  workspace and visibility, and respect preview mode. The status button selects
  Settings explicitly before opening the sidebar.
  Eight focused cases cover both themes, four edges, action routing, hidden
  readings, stale/removed windows, preview commands, auto mode, palette parity,
  interrupted entry and long compact window lists. All pass, including GPU
  runs (the corrected entry geometry assertion ran separately). Nine Power/Bar
  settings cases and five frame/rail cases also pass: 22 unique cases overall.
  Both themes' full-shell checks at 500×720, with the bar visible and its
  Settings page open, report zero errors and warnings. Private Cage verifies
  all four edges, horizontal/vertical scrolling and Settings handoff at that
  size; captures are in `/tmp/morf-bar-cage/out/morf-bar/`. Window data is fake,
  commands are disabled and preferences use scratch files. GPU parity and
  palette captures are in `/tmp/morf-bar-films/` and `/tmp/morf-bar-baseline-films/`.
  A second live Cage run verifies interrupted entry, complete hiding and
  vertical reopening in `/tmp/morf-bar-motion-cage/out/morf-motion/`. The visible
  preview is refreshed with the bar enabled in its scratch settings only.
- Lule now uses `lule_model` and theme-specific page builders. Scanning,
  preview generation and apply commands remain in `lule_studio`; the preview
  guard prevents wallpaper hooks during dry-run inspection. Tsugumori uses a
  responsive wallpaper/palette workspace and viewport-aware section titles.
- On-screen keyboard: `keyboard.lua` owns delivery, input-method subscription,
  hardware/auto-show policy and manual ownership. Both themes have independent
  `views.keyboard` builders. Material preserves its geometry; Tsugumori supplies
  responsive layout choices, framed key faces, shared title/hover motion and
  a Hide action. Narrow outputs wrap layout choices into two rows.
  `lib.osk` retains the layouts, modifiers, repeats, long presses and patterns;
  themes can supply key faces and dimensions without implementing key delivery.
  Its active scope cancels held keys on hiding, disabling or changing layouts.
  Lock/greeter keyboards in both themes supply sheet/method/busy/output scopes.
  Hardware-keyboard detection is injected by the authentication controllers;
  the authentication views no longer query that service directly.
  Seven keyboard cases, four auth-keyboard cases, five appearance cases and the
  original lock case pass (17 unique cases); all seven keyboard cases also pass
  with GPU rendering. The seven Material layout captures are byte-identical to
  the saved source in `/tmp/morf-keyboard-material-baseline/`. Captures are in
  `/tmp/morf-keyboard-{baseline,theme}-films/`. A 500x720 private Cage run verifies
  layout changes, interrupted reopening, Hide and retained `keyboard_focus=none`
  in `/tmp/morf-keyboard-cage/out/morf-keyboard/`. These are preview checks, not
  a claim of complete authentication or multi-output coverage.
  The full-shell Material 1920x1080 and Tsugumori 500x720 keyboard checks both
  report zero errors and warnings. The visible isolated preview opens on the
  Tsugumori keyboard; the installed process and personal settings are untouched.
- Installation/bundle resolution and complete multi-output visual parity
  remain unverified. Do not install based on the partial checks above.
- Lock desktop data: `models/lock_desktop.lua` now owns the optional weather/MPRIS
  readers and artwork resolution, shared across output views. Reads pause when
  the lock is inactive; playback validates the action, current primary output
  and dry-run state. Authentication views no longer import these readers or
  read the hostname. The greeter controller injects host identity into both
  themes. Tsugumori now has a resting-screen weather/playback register, with
  shared title decode and finite entry motion, that fits portrait outputs and
  hides while the authentication sheet is open.
  Seven desktop-data cases, four auth-keyboard cases, five appearance cases
  and the original lock case pass (17 unique cases). Six desktop-data cases
  also pass with GPU snapshots; all four Material lock/greeter rest/sheet
  images are byte-identical to `/tmp/morf-auth-desktop-baseline/`. Captures are
  in `/tmp/morf-auth-desktop-films/` and `/tmp/morf-auth-desktop-baseline-films/`.
  Private 500x720 Cage runs verify lock playback/data updates and sheet return
  in `/tmp/morf-lock-desktop-cage/out/morf-lock/`, and greeter host identity,
  sheet transitions and F2 session selection in
  `/tmp/morf-greeter-desktop-cage/out/morf-greet/`. They use fake desktop readers,
  window/preview lock mode, and no greetd socket. Full authentication,
  account/session and multi-output behavior still need broader coverage.
- The six original Material suite failures were stale assertions, independently
  reproduced against the pre-split launcher and session source: launcher top
  placement, calculator answer ID, fallback results for plain queries, and
  read-only GPU probes during session/utility checks. Expectations now match
  that baseline; dry-run checks allow only the exact read-only probe argv.
  A corrected combined run stalled after case ten, although its calculator
  case passes separately. All 31 cases now pass when each runs in a fresh
  process (`--filter`); the log is `/tmp/morf-material-per-case.log`. This
  establishes case coverage, not a fix for the combined-run stall.
- The latest full-shell Tsugumori check with its dashboard open reports zero
  errors and warnings. Material's open overview reports one empty-layout lint
  warning; the exact pre-extraction dashboard reproduces it. Before/after GPU
  captures preserve Material's geometry. Installation remains untested.

### Dashboard builder

`views.dashboard.build(model)` returns `content`, `width`, `height`, `size`,
`edge`, optional drawer `props` and optional Material `bud`. The controller
owns dismissal, edge activation and background history. The model supplies
readings, calendar navigation, playback callbacks and cached page builders.
`tab` is requested navigation; the view calls `present(index)` when its cover
allows the displayed page to change. Heading and page activity follow that
displayed index. Overview readouts retain their last values during departure
without keeping hidden service bindings active. Theme-specific layout and
animation handles remain in the builder.

`views.dashboard_weather.build(model)`, `views.dashboard_battery.build(model)`
and `views.dashboard_performance.build(model)`
return `page` and optional dynamic
`width`, `height` and `resize(available_width, available_height)`. The builder's
`WIDTH` and `HEIGHT` are its preferred dimensions. Dashboard calls `resize`
for responsive pages and sizes their viewport content from the returned
dimensions; fixed pages retain native sizes. `shell/dashboard_weather.lua`
constructs its shared model and selected view; the Battery controller follows
the same boundary, as does Performance. A page can expose a reactive `navigation`
signal to reset its viewport when its own displayed detail changes.
Shared samplers own collection, and the models gate UI reads
by visibility. Views own reflow and finite entry animations. The Material
builders have no service imports or startup side effects.

`views.graphs.new(samples)` builds graph components from an injected history
capacity. Material preserves its existing plot geometry. Tsugumori supplies
responsive paths, grid and caption components, with newest samples at the right
edge and every point clamped within its scale. Material Performance retains
its original inline graph rendering, now supplied by the shared model.

### Bottom workspace builders

`views.bottom.build(model)` returns `content`, `width`, `height`, optional
`edge` and drawer `props`. Width and height may be bindings. The controller
owns its drawer; global IPC, hover and outside-click handling stay in the shell.
The view receives tab metadata, `tab`, `displayed`, `opened`, `select`,
`present(index)`, screen/desk sizes and `page(key, width, height)`.
`present` rejects stale selections. Each page receives its own active predicate,
title and connection status through `views.assistant.build(context, w, h)` or
`views.drop.build(context, w, h)`. Neither page imports a controller or service.

Tsugumori calls `present` beneath the page cover and resets the incoming scroll
position. Titles and entry animation follow the displayed page. Its width
follows the available desk, including changes to bar placement; Material keeps
its previous dimensions. The hover trigger reads the selected view's actual
width. Further tabs are declared in the shared model with corresponding page
builders rather than added to either visual container.

### Settings builders

`views.utilities.build(model, width, height)` returns `node` and `height()`
for the overview content. The controller supplies `TOGGLES` (readers and action
callbacks), level readers/setters, `DETAILS`, `page_content(key, w, h)`, capture
handoff and shared `detail`/`displayed`/`opened` signals. `request(key)` validates
navigation; `present(key)` accepts only the current request. The model caches
overview readings while hidden and owns idle inhibition independently of panel
visibility. Themes contain no service imports, process commands or configuration
writes. They own composition, scrolling, control feedback and transition timing.

The sidebar's displayed Settings scope drives the controller lifecycle. Shared
Material tabs now publish the same visibility scopes as Tsugumori's covered tabs.
Each Settings detail scope follows the displayed detail, including VPN watch
lifetime and battery reads. Material presents immediately; Tsugumori presents
beneath its cover and cancels stale transitions on closing or rapid navigation.

### Wired, VPN and Tor builders

`views.net_pages.build(model, width, height)` receives `kind`, `active`, keyed
`network`, `apps` and `tor` row lists, service availability, scan completion,
request status and `toggle(target, optional_on)`. Each row has a stable key,
presentation metadata and a capability flag. `row(target)` reads the cached
snapshot; `toggle` resolves the identity again against the current source and
rejects missing, unavailable, read-only or hidden targets. Callback generations
prevent earlier requests or hidden completions from replacing current feedback.
NetworkManager-managed mesh links remain excluded from the Tunnel register.

The shared model holds `lib.vpns.watch` only while its actual page is presented
and releases it on hiding. Theme builders own typography, layout, scrolling
and animation; they import no service and start no polling timer. Internal
request feedback handles both synchronous NetworkManager errors and VPN commands
whose result arrives later without an immediate return value.

### Power and Bar settings builders

`views.power_page.build(model, width, height)` receives the shared battery model
plus profile readers, `PROFILES`, formatted `FACTS`, `summary`, `message`,
`select(id)` and `open_battery()`. Profile capability follows the profile service,
independently of UPower battery availability. Selection validates the current
offered modes and stops when hidden or in dry-run mode. The view never invokes
UPower or imports a dashboard controller. Cached readings stay stable while
the page leaves without subscribing to hidden source updates.

`views.bar_page.build(model, width, height)` receives mode/edge/title metadata,
their readers and validated setters. These are internal preference changes,
so sandbox previews can exercise them against scratch configuration even in
dry-run mode. Views contain no configuration writes or bar-controller imports.
Both builders own their geometry, scrolling, titles and finite entry motion.

### Radio page builder

`views.connectivity.build(model, width, height)` builds Wi-Fi or Bluetooth from
the shared controller's model. `wireless` selects the radio kind; `rows` is the
sorted keyed list and `row(target)` resolves its current cached data. `available`,
`enabled`, `discovering`, `message` and `failed` expose presentation state.
`set_enabled`, `scan`, `choose` and `open_settings` remain in the model, including
service callbacks and dry-run handling. Visual packages own list geometry,
scrolling and row feedback and import no network or Bluetooth service.

### Audio page builder

`views.sound_page.build(model, width, height)` returns the page node. The shared
controller supplies `kind` (output/input), `active`, reactive `devices`, `streams`
and `channels` lists, plus cached `available`, `default`, `device`, `stream` and
`channel` readers. Actions accept the row being operated on so the model can
check its identity against the current server before issuing a command. Channel
updates preserve the other channels from the live device. Themes call those
actions; they do not import `morf.audio` or start their own samplers.

### Session builder contract

`views.session.build(state)` returns `content`, `width`, `height`, `edge`,
optional drawer `props`, a `dim()` builder and a `shown(open)` callback.
The controller supplies action metadata, account name/picture, `focus` and
`opened` signals, and `run`, `accept`, `close` and `key` callbacks. Views own
their geometry, keyboard input node and animation handles; only the controller
reads configured commands or invokes them. `session_theme_spec.lua` exercises
both themes against this boundary, including actual dispatch into command
stubs. No test runs real session or power commands.

### Authentication builder contracts

Lock views receive `ctx.desktop.for_output(output_name)`, which supplies
`weather_available()`, `weather()`, `weather_symbol()`, `media_available()`,
`player()`, `artwork()` and `control(action)`. Creating another output wrapper
does not create another reader. A view decides where to display these readings;
it does not connect to MPRIS, create weather polling or resolve remote files.
The controller retains the primary-output and active-stage policies. Greeter
views receive `ctx.hostname`, read once by their controller.

`views.polkit.build(state)` receives `request`, `phase`, `info`, `typed` (a
capped count), `opened`, and submit/cancel/focus callbacks. It returns a drawer
description (`content`, `width`, `height`, `edge`, optional `props`) and `shake`.
The controller mounts its private input after the view is built, prevents text
entry while checking, and clears input on submit, completion or drawer close.

`views.authsteps.build(model)` receives the shared marker state plus `opened`
and returns a drawer description. It handles no authentication or input.
`auth_dialog_theme_spec.lua` covers both themes' simulated retry/success/cancel,
late completion timers, output focus, stage changes, hidden animation settling,
compact field bounds and suppression of the real agent in dry-run mode.

### Bar builder

`views.bar` exports numeric `horizontal` and `vertical` reserved thicknesses
and `build(model)`. The controller uses these dimensions for `insets()` and
`desk()` before widgets are built. The model supplies visibility/orientation,
screen and inset queries, title preference, a keyed window list, status and
clock signals, and focus/launcher/dashboard/Settings callbacks. Icon lookup is
shared; views receive icon names/paths and own their rendering. Views own
window overflow, geometry, hover and entry motion. They never read compositor
or device services, issue commands, or start polling timers.

### Side-panel builder

`views.side_panel.build(model)` returns `width`, a `height` binding, `edge`,
`content` and drawer `props`. Its model provides tab metadata, `tab`,
`displayed`, `opened`, `select`, `showing`, `present(index)`, `desk_size()` and
`page(key, width, height)`. The controller chooses and builds the page; the
theme decides its dimensions and when to call `present` during a transition.
The model ignores stale presentation callbacks. Actual displayed-page scopes
drive page headings and service-derived rows. Dismissal, Taskwarrior polling
and notification coverage remain controller responsibilities.

### Planner builders

`views.tasks_page.build(model, width, height)` receives task rows, filter/query
signals, a draft, field metadata and edit/save/cancel/action callbacks. The
height is a binding. `editor_revision` changes once per new editing session;
views copy draft values into their inputs on that revision and keep focus
handling separate from draft synchronization. Views own scroll position and
input focus; the model owns the original values used to send changed fields.

`views.calendar_page.build(model, width, height)` receives month/day/agenda data,
the shared selected-day signal and navigation/plan/edit callbacks. The shared
controller switches to Tasks for editing. `views.planner_widgets` supplies each
theme's form controls; fields accept an injected Escape callback and do not
import the left drawer or Taskwarrior. Neither page builder issues commands or
starts polling. The shared planner retains a single Taskwarrior client.

### Keyboard builder

`views.keyboard.build(model)` receives `active()`, `send(event)`, `close()` and
`desk_size()`. It returns `width`, `height`, `content`, the shared OSK `keys`
interface and optional `shown(on)` motion. It does not subscribe to input
methods, create a virtual-keyboard sender or change surface focus. The
controller validates manual modes, distinguishes manual and automatic opening,
and suppresses delivery while hidden or in dry-run mode. Authentication views
send only through their injected controller callbacks and scope their OSKs to
the visible, enabled entry method.

### Popup and OSD builders

`views.notifications.build(state)` returns drawer properties plus `dismiss(i)`.
The latter starts visual departure and returns its duration; the controller
captures the notification ID before the animation and dismisses that ID after
the returned delay. A theme never owns notification expiry or server calls.
`notification_theme_spec.lua` verifies both themes' history, expiry, DND,
focused-output routing, server dismissal and overlapping arrival/dismissal;
the compact Tsugumori test scrolls to the final displayed card.

`views.levels.build(model)` receives reading/icon callbacks, `shown`/`active`
signals, desktop/rail geometry and the sidebar drawer to follow. It returns
`node`, its frame-field `shape`, `show(previous_kind)`, `hide(done)`, `geometry`
and optional `hold` duration. The controller owns repeated-update timeout
renewal; each theme owns interrupted animation handling. `levels_theme_spec.lua`
checks both layouts' auto-show suppression, switching readings, timeout renewal,
interrupted dismissal and the compact Tsugumori mute/brightness presentation.
The existing `osd` IPC defaults to volume and now also accepts `brightness` or
`volume` explicitly, allowing either view to be inspected without setting a
device level.

### History builder

`views.notification_history.build(state, width, height)` returns `node`,
`animate_clear()` (departure duration) and `reset_clear()`. The controller
supplies grouped data, expansion queries, an active scope, count and
copy/forget/clear/toggle callbacks. It retains captured IDs and clear timers;
views own all geometry and animation handles. Both layouts retain the existing
limit of eight recent application groups and four entries per group;
Tsugumori's viewport scrolls these groups rather than clipping the bottom ones.
The shared heading's own visibility also follows expanded/collapsed content.

### Frame and rail builders

`views.frame.build(model)` receives desktop geometry, the bar, drawers,
rail/level nodes and shapes, controller-built input planes and edge triggers.
`themes/frame_host.lua` composes them in the same stacking order. Each theme
owns frame geometry, paint and optional decorations; `insets(bar)` supplies the
surface reservation. No frame builder adds dismissal or hover policies.

`views.rail.build(model)` receives workspace/occupancy callbacks, enabled and
hold preferences, group size/base and the left drawer to follow. It returns a
node and frame shape. `geometry(model)` remains callable before construction,
so the shared OSD can align itself without instantiating the rail. Both views
cancel transient motion when disabled, including the separately painted frame
shape. The short-lived Tsugumori indicator uses the shared heading with shorter
decode lead/stagger; ordinary headings retain their original timing.

## Reference

[Tsugumori](https://github.com/Aleph1-9012/Tsugumori) is the design reference.
The initial checkout for inspection is `/tmp/morf-tsugumori-reference-20260928`.
Its fixed red palette is deliberately not adopted. Keep attribution for any
assets or code actually reused; independently authored components use morf's
own scene/SDF primitives.

### Lock and greeter skin refinement (2026-09-29)

The shared authentication layouts now expose appearance hooks for avatars,
backgrounds, clocks, keyboard colours and sheet motion. Tsugumori uses bevelled
account portraits, quiet accent corner marks, Japanese SHUGO/AKATSUKI reveal
covers, a finite per-digit clock roll and two bounded phase-art strips. Material
retains its existing surfaces and timing. Account, input, method and session
controls keep their shared order and geometry. The password field now preserves
its semantic error border instead of replacing it with a decorative accent.

New animations run on appearance or a changed clock digit, then settle. Rapid
sheet reopening cancels the previous reveal before starting another. On-screen
keyboard styling uses the library's existing look/action hooks; key delivery,
repeat cancellation, passwords, PAM, greetd and retry policy are unchanged.

Validation: 20 cases passed across appearance, auth_keyboard, lock, auth_skin
and shared_layout specs; the separate auth regression script passed startup,
empty/duplicate submit, explicit retry, stale callbacks and preview isolation.
The new skin fixtures render rest, intermediate reveal, password, error, pattern
and 500x720 keyboard views in `/tmp/morf-auth-refined`. Native Cage rendered both
roles in isolated preview modes without Lua runtime errors.

Installed and applied on 2026-09-29 at the user's request. Apply now snapshots
selected theme/font defaults beside each configuration, so the greeter account
inherits Tsugumori without reading the user's home. Personal settings and
explicit preview overrides retain precedence. Four defaults-precedence cases
and seven installer cases pass. All 570 staged runtime/configuration files
matched their installed checksums; installed lock and greeter checks each
reported zero errors and warnings. Only the fixed `previous` backup remains.
