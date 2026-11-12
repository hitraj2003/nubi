# Current HTML → Slint parity

Status: **partial; not accepted as visual or interaction parity**. Differences are
larger than font antialiasing. This supersedes the old toolkit gate as the visual
acceptance record; the old gate remains available for its input experiments.

## Evidence

- `reference/`: 84 PNGs from the unmodified production HTML in headless Chrome,
  with both themes and device scale factors 1 and 1.5. This covers the closed
  notch, open Sessions and Usage tabs, Inbox, Sessions, Usage, Alerts, all eight
  settings panes including DeskTime/About, and six overlay states: approval,
  detailed approval, expanded parameters, single choice, selected multi-select,
  and a custom answer.
- `reference/manifest.json`: actual logical sizes, DPR, content text, and measured
  element bounds. The panel viewport is 1100×760; notch/overlay viewport is
  720×700. At 150%, PNG dimensions are 1650×1140 and 1080×1050 respectively.
- `reference/provenance.json`: Chrome version and source/fixture SHA-256 hashes.
  The capture harness rejects logged JavaScript failures.
- `fixture.json`: the shared deterministic time, sessions, usage, settings, and
  alerts. `prepare-parity.py` derives Slint models from it. Chrome evaluates the
  actual `usageColor()` function into the prepared palette; colours are not
  approximated with provider colours.
- `compare/`: 60 sets with reference on the left and compiled Slint on the right,
  plus separate Slint PNGs, difference images, and `metrics.json`. The new port
  renders three notch states, six panel/settings pages, and six overlay states
  at both scales/themes.
  Transparent Slint pixels are composited over the same Chrome canvas backdrop
  (RGB 18 in dark mode, white in light mode for notch/panel, and RGB 96 for
  transparent light overlay captures).

The comparisons are real software renders of the compiled components. They do
not use screenshots as Slint page backgrounds. Cropped content metrics are
included because whole-window percentages understate differences in small
notches. The >12-channel-error threshold is a diagnostic, not a pass criterion.

## Implemented scope

- Closed notch: source-measured 145×44 content area, concave ears, 16px lower
  corners, provider SVGs, 30px rings, thick session arcs at radius 11/stroke 2.6,
  thin weekly arcs at radius 14.4/stroke 1.6, percentage text, track opacity, and
  source OKLCH colours. The previous spike incorrectly used solid circles and a
  180px pill.
- Open notch: the 620px shell, Sessions rows, header tabs/count pill, bundled
  provider/control SVGs, row hover, and Usage cards with the source's filled pie
  sectors rather than circular borders. The source 40/140ms hover dwell and
  numerical spring engine remain in use; switching Usage resizes depth to 180px.
  The window button opens the new panel on Inbox.
- Panel: the 64px rail, warm themes, source logo/icons, Usage cards and threshold
  bars, Sessions rows and provider filter, empty Inbox, fixture Alerts, General
  settings cards, and the About layout with the real AboutSlint widget.
- General: pointer/keyboard switches, provider selection, shortcut capture,
  draft state, Save/Discard, and guarded navigation away from unsaved settings.
  Session filtering uses a filtered model so removed rows do not reserve space.
- Fonts: bundled Geist and JetBrains Mono; prepared static Geist 400/500/600
  faces preserve the source outlines. The source UI font remains the installed
  Segoe UI Variable family; see the weight limitation below.
- Software shadows: source-derived, reusable procedural shadow images, sliced
  around variable card sizes. Slint 1.18.1's software `draw_box_shadow` is a TODO;
  setting native shadow properties alone produces no shadow. The generation
  script uses the source CSS offsets, spread, opacity, and blur radius.
- Full request overlay: source 340px card, 24px radius, 16px padding, request
  header/icons, command summary, All parameters disclosure with scrollable JSON,
  Allow/Deny and terminal actions, provider time bar, and source countdown text.
  The source formats whole minutes as m:ss and values below a minute as seconds.
  Hover over the card, including action buttons, freezes the local countdown;
  external held state is also supported. Expiration resolves the simulated
  request as a terminal takeover. Overlay shadows use their own source CSS
  layers rather than the panel shadow. The bar has explicit corner geometry
  because software rounded clipping does not preserve that edge.
- Question overlay: single choice advances immediately; later steps focus the
  first option. Multi-select toggles without advancing and submits with Done.
  Custom text uses Slint TextInput, trims before submission, preserves Unicode,
  and supports Enter. Completed answers retain question order in an in-memory
  model. Answer in terminal and Escape hand the simulated question back.
- Overlay input: initial Deny/first-option focus, trapped forward/reverse Tab,
  Enter/Space buttons, Escape, focus outlines, and empty-answer guards. Colour
  transitions respect reduced motion; overlay entrance/exit springs are not
  implemented. The fixture adds existing production UI text as sample questions;
  no production request copy or design tokens were changed.

## Remaining differences and missing behavior

### Visual differences on the implemented fixture screens

1. Segoe UI Variable's named Text Semibold face is not selected like Chrome by
   the current Slint font database. Chrome's font probe reports
   `Segoe-UI-Variable-Text-Semibold` for 500-weight labels and
   `Segoe-UI-Variable-Text` for descriptions. Slint labels are visibly lighter;
   their widths and baselines also differ. Geist and small-number rasterization
   still differ. These are not all antialiasing differences.
2. Exact text positioning, header/tab widths, badges, segmented-control widths,
   shortcut-cap spacing, state-pill sizing, and per-row status alignment need
   further adjustment. Some values are reconstructed for the supplied fixture
   rather than measured dynamically for arbitrary content.
3. The procedural shadow approximation does not exactly match Chrome's blur,
   inset highlight, and composition. Sessions and Alerts still lack the complete
   source shadow treatment. Selected sidebar/segmented highlights and switch
   thumb shadows also differ.
4. Source rounded clipping, SVG stroke rasterization/caps, lower-corner edges,
   and fractional positioning differ at 100% and 150%. Ring colours use integer
   percent palette samples; intermediate animated colours are quantized.
5. AboutSlint is an intentional addition authorized by the owner. It has no
   counterpart in today's HTML reference. About spacing, version-history card
   sizing, and text do not yet match exactly around the addition.
6. The layout is verified only at the captured logical sizes. The new panel has
   a 900px minimum width and does not reproduce every source resize/scroll case.

### Unported states and surfaces

- Appearance, Display, Questions & approvals, Notifications, DeskTime, and
  Connections settings content. Their navigation labels exist; their content is
  not implemented. Their reference PNGs are available, but no Slint comparisons
  are claimed for them.
- Overlay slim approvals, request decks/list arrangements, concurrent alert
  islands, popup folding, missed requests, and backend snapshot reconciliation.
  The new full overlay is separate from the older `toolkit-gate` approval demo.
- Inbox approval/question cards and request arrangements. Only the empty Inbox
  fixture is ported; the new question/disclosure/countdown behavior belongs to
  the separate full overlay.
- Notch Waiting/request cards, alert view, DeskTime content, folded thin-tab
  mode, side/bottom edges, dragging, missed requests, working/attention activity
  rings, equalizer animation, stale/auth/loading/error states, exhausted glyph
  dimming, and dynamic usage values. The implemented fixture uses idle/done
  sessions and fresh usage.
- Session context menus and their actions, subagents, attention/compact-queued
  copy, long-name overflow variants, and full scrolling behavior.
- Alert grouping, queued markers, test-alert feedback, live ages/countdowns,
  refresh spinner/failure behavior, full theme preview, report/release actions,
  and their fake-backend state transitions. Callback declarations alone do not
  establish parity for these actions.

### Motion and input gaps

- Numerical notch spring integration matches `ui/motion.js`, but source content
  crossfades, blur, scale, stagger, tab springs, panel rail/side/segmented springs,
  press transforms, enter/leave transitions, and retargeting are not all ported.
  The current rail uses a timed easing; it is not the source's spring.
- Tooltips exist on rail buttons; exact placement/fades and all source tooltips
  are incomplete. Button/row hover states exist, but every hover/press variant
  has not been captured or compared.
- General keyboard/pointer state changes and shortcut capture have automated
  coverage. Complete Tab order, focus outlines, arrow navigation, initial focus,
  modal focus trapping/restoration, Escape/backdrop handling, DND keyboard input,
  and reduced-motion behavior remain incomplete or unverified.
  The separate full overlay now has automated forward/reverse traversal, initial
  focus, Enter/Escape, real option clicks, custom Unicode input, multi-select,
  empty Done, duplicate-resolution protection, and local/external hold coverage.
  This does not establish focus restoration or OS foreground activation.
- Native foreground focus, physical keyboard/mouse, Narrator, IME, high contrast,
  monitor transitions, desktop transparency/composition, and displayed-frame
  smoothness require a real desktop review. Offscreen tests do not prove them.

## Validation and resources

Final command results, executable sizes, native private bytes, and CPU samples
are recorded below after the final measurement run. Resource measurements are
idle probes, not animation or real-session workload profiles. No dependency was
downloaded. Production `ui/` and `src-tauri/` remain unchanged; no commit was made.

## Full overlay acceptance limits (2026-11-12)

The overlay is **partial**, not accepted as exact visual or interaction parity.
The 24 compiled overlay renders compare real components against unmodified HTML
at 100% and 150%, in both themes. Capture checks report no production JavaScript
errors. `compare-parity.py` now fails when any required overlay pair is missing.

Remaining overlay gaps:

- Font weights, glyph widths/baselines, question line spacing, exact link/button
  widths and vertical alignment, disclosure marker rasterization, and shadows
  still differ. The question's inline muted prefix uses StyledText; its line
  spacing is approximated for these fixture screens. Description-bearing options
  and arbitrary long questions need additional reference coverage.
- Entrance/exit blur, opacity/scale/translation springs and their retargeting are
  absent. Settled cards are rendered immediately, including under reduced motion.
- Countdown and resolution are local simulation. Terminal actions expose an
  in-memory signal; no terminal opens, backend hold messages are not sent, and
  backend acknowledgements/retries or synchronization with Inbox are not ported.
  Countdown hold transitions are sampled at 100ms; actual expiry timing needs
  desktop/backend validation. Resolution hides the simulated window immediately.
- Keyboard traversal/initial focus are explicit additions because the production
  overlay does not currently define that full keyboard contract. Native focus,
  focus restoration, Narrator, IME composition, physical input, OS reduced-motion
  detection, and multi-monitor/high-contrast behavior remain unverified. The
  standalone overlay accepts `--reduce-motion`; this is not automatic OS detection.
- Slim cards, deck/list layouts, multiple requests, alert coexistence and automatic
  folding remain outside the implemented full-card fixture.

Validation on the final overlay sources:

- `cargo build --release --offline`: pass.
- `cargo clippy --offline -- -D warnings`: pass.
- `cargo test --offline`: pass, 11 tests across the three binaries.
- `node capture-reference.cjs --overlays-only`: pass, 24 overlay references,
  no captured JavaScript errors.
- `target/release/parity.exe --render`: pass, 60 software-rendered bitmaps.
- `python compare-parity.py`: pass, 60 reference-left comparisons, including
  all 24 required overlay pairs, separate Slint PNGs, differences and metrics.
- JavaScript syntax, Rust formatting for modified Rust files, source hash
  verification, and `git diff --check`: pass.

Overlay content-region mean absolute RGB error ranges from 2.08 to 6.26 on the
0–255 channel scale. The fraction of content pixels exceeding a 12-channel error
ranges from 2.35% to 9.38%. These are diagnostics, not visual acceptance. The
content crop is logical x=185..535, y=8..520 at each scale and includes shadows;
it does not normalize to the exact card area. Reviewed reference-left approval,
expanded parameters, question and selected multi-select images at both scales
still show the font/spacing/shadow differences listed above. All software renders
use compiled UI elements and procedural assets, never reference image backgrounds.
