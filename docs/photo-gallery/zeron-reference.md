# Zeron reference — design and performance

Zeron is a native GPUI desktop app. We use it as the reference for the photo
prototype's look and for its performance discipline.

## Source

- Repository: <https://github.com/zeronsh/zeron>
- Revision read for this document: [`7a472fce41b6a834da87b460e189adb05eacf3d4`](https://github.com/zeronsh/zeron/tree/7a472fce41b6a834da87b460e189adb05eacf3d4) (2026-10-01, v0.2.101)
- License: MIT, Copyright (c) 2026 Wing. See [`LICENSE`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/LICENSE).

Every file path below is relative to that revision. Read a file at the pinned
revision by prefixing
`https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/`.

To get a local copy:

```sh
git clone --depth 1 https://github.com/zeronsh/zeron /tmp/zeron
```

Size for scale: about 380,000 lines of Rust in 18 crates. The GPUI app alone
(`crates/ui`) is about 103,000 lines with roughly 1,400 test functions.

## Why this is our reference

- Same stack lineage. Zeron uses `gpui` plus a forked `gpui-component` published
  as `gpui-base` (Apache-2.0). Our `gpui-kit` comes from the same upstream.
- The visual language matches what we want: dark, low-contrast chrome, one
  accent, frosted glass, small radii, hairline borders.
- It is MIT, so we may take code with attribution.
- It publishes measured performance work instead of opinions.

## Part 1 — Design

### 1.1 Semantic roles, not source formats

[`crates/theme/src/lib.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/theme/src/lib.rs)
holds a source-neutral model: `ThemeFamily` groups variants, `ThemeVariant` is
one fully resolved palette, `ThemeSelection` stores one variant id per
appearance. Import formats stop at a compiler
([`crates/theme/src/vscode.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/theme/src/vscode.rs)).
No widget ever sees a VS Code workbench id.

[`crates/ui/src/theme.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/theme.rs)
turns a variant into the struct the app paints from. The roles we should copy:

| Group | Roles |
| --- | --- |
| Neutral surfaces | `bg`, `surface`, `surface_raised` |
| Elevation ladder | `surface_card`, `surface_dialog`, `surface_overlay` |
| Interaction | `element_hover`, `element_active`, `border`, `border_strong` |
| Text | `text`, `text_muted`, `text_faint`, `text_dim` |
| Solid | `solid`, `on_solid` |
| Accent | `accent`, `accent_strong`, `accent_wash`, `on_accent` |
| Status | `danger`, `danger_muted`, `warning`, `warning_muted`, `success`, `success_muted`, `busy` |
| Components | `surface_raised_hover`, `band`, `input_bg`, `selection`, `cursor`, `caret`, `danger_strong` |
| Content | `code_text`, `code_wash`, `diff_add`, `diff_del`, `diff_hunk_bg` |

The dark values are sampled, not derived by arithmetic:

```
bg                grey(6)          # main panel, sampled #060606
surface           grey(13)         # shell and sidebar, sampled #0d0d0d
surface_card      grey(0x0e)
surface_dialog    grey(0x10)
surface_overlay   grey(0x16)       # popover, menu, command palette
element_hover     hsla(0, 0, 0.92, 0.11)
element_active    hsla(0, 0, 0.92, 0.16)
border            white @ 0.08
border_strong     white @ 0.14
input_bg          white @ 0.03
```

The four elevation steps are small and deliberate. The module warns that
collapsing them onto one token visibly lifts popovers off their intended plane.

### 1.2 Light is designed, not inverted

The module documentation names three failure modes and fixes each one. This is
the single most useful idea in the file.

1. **Surface order flips.** On dark, the content panel is the darkest plane and
   the chrome sits above it. On light, the content panel is white and the chrome
   recedes by going *grey*. Popovers stay white and earn separation from a
   border and a shadow instead of from lightness.
2. **Elevation reverses.** A faint white wash reads as raised on near-black. Its
   literal translation, a faint black wash on white, reads as *recessed*. The
   composer looked like a dent instead of a plate. Light lifts with pure white
   plus a border and a shadow.
3. **Accents must move down the scale.** The dark palette's 400-level accents
   fall to 2–4:1 on white and fail WCAG AA. Light mode uses the 600-level
   siblings at the same hue, which restores the contrast ratio.

Two constants carry the numbers across appearances:

- `INK_FILL_SCALE = 1.0` — alphas are quoted in dark-mode terms at every call
  site and the light value is derived. A blanket light multiplier of 0.5 was
  tried and removed: it turned `ink(0.03)` into 1.5% black on white, so the
  composer lost its background.
- `INK_HAIRLINE_SCALE = 1.35` — edges scale the opposite way to fills. A 1px
  line needs *more* ink on a bright field, not less. Capped at 0.5.

The result is asserted, not eyeballed:
`tests::text_contrast_is_paired_across_appearances` checks that each light text
token lands within about 0.5 of its dark counterpart's contrast ratio.

### 1.3 Numbers drive layout, colours are paint

Layout constants are plain `f32` on `Theme` and never depend on the palette.
Copy these directly:

```
TITLEBAR_HEIGHT        38    TITLEBAR_TOP_PAD     4
STATUS_STRIP_HEIGHT    24    TRANSCRIPT_FADE_BAND 24
HEADER_HEIGHT          44
BUBBLE_RADIUS          16    PANEL_RADIUS         10    CONTROL_RADIUS 6
SPACE_XS/SM/MD/LG    4 / 8 / 12 / 16
TEXT_STACK_GAP          1
```

`TITLEBAR_TOP_PAD` has a comment explaining the arithmetic: top-only padding
moves a flex-centred row by half its value, so `38 / 2 + 4 / 2 = 21` matches the
centre of the native macOS traffic lights.

[`crates/ui/src/surface_chrome.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/surface_chrome.rs)
shows how to keep a toolbar consistent: one `toolbar(theme)` builder and one
`input()` builder, both built from named constants (`CONTROL_SIZE = 24`,
`ICON_SIZE = 14`, `CONTROL_GAP = 4`, `EDGE_INSET = 8`).

### 1.4 Contrast hardening runs at build time and at runtime

Three levels, all in `theme.rs`:

1. `Color::ensure_contrast(background, minimum)` walks a colour toward black or
   white in 20 steps until the ratio is met.
2. `harden_model_foreground(...)` runs at import time over every solid surface a
   shared role is painted on. It first looks for a stronger *related* role in the
   source palette, then adjusts the colour toward a contrast-safe anchor.
3. `contrast_checked_tint_alpha(...)` runs at paint time. It raises a translucent
   tint's coverage in 20 steps until primary text clears 4.5:1 and muted text
   clears 3:1 against the *adverse* backdrop for the appearance (white in dark
   mode, black in light mode).

Lesson for us: an imported palette should get a denser material, never broken
text. Do not clamp the user's choice away; strengthen the surface behind it.

## Part 2 — Glass

This is the part we most want. Zeron has three distinct materials. Keep them
separate in our code.

| Material | What it blurs | Where it runs | Entry point |
| --- | --- | --- | --- |
| Window glass | The desktop behind the window | Native compositor | `WindowBackgroundAppearance::Blurred` |
| Frost | In-app content behind a floating card | gpui renderer (Metal, wgpu, D3D) | `frost::frosted(...)` |
| Plate | Nothing; it is a lit surface | CPU paint | `glass::light_plate` / `accent_plate` |

### 2.1 Window glass

Set in `WindowOptions`, from `Theme::window_background_appearance()`:

- Linux: `Transparent`, always. The shell draws its own decorations and rounded
  window corners need real transparency. The frost itself is opaque there.
- macOS and Windows, when the resolved surface treatment is frosted: `Blurred`.
- Otherwise: `Opaque`.

**Glass only shows where a translucent surface sits over the window's backdrop.**
This is the trap that cost a pass: a window can ask for the blur, resolve a
translucent tint, and still show nothing, because an opaque plane covers the
backdrop and every library component paints its own opaque token over the rest.
The application must leave the root unpainted, let one region own the opaque
plane, and override the component tokens with the chrome tint. See Part 10,
fourth pass.

Two traps, both documented in the code:

- **The value must be re-applied after every theme swap.** gpui's macOS backend
  tears the `NSVisualEffectView` out of the hierarchy whenever the value is
  anything but `Blurred`. `crates/ui/src/lib.rs` and `appearance::apply` are kept
  as one source of truth. If they disagree, vibrancy dies on the first theme
  change and never returns.
- `Theme::is_glass()` must gate on the *resolved alpha* (`glass().a < 1.0`), not
  on the platform constant. The constant is platform-wide; the frost alpha is
  per-appearance.

Colour changes are applied with `App::refresh_windows`, not `notify()`. Colours
are read imperatively at paint time, so no view knows its colours went stale.
`refresh_windows` marks every window dirty and disables the per-view prepaint
cache for the frame.

### 2.2 The window tint: `Theme::glass()`

```rust
GLASS_ALPHA       = 0.80   // macOS, Windows
GLASS_ALPHA_LIGHT = 0.80
// Linux: 1.0, so the window stays opaque
```

The base tint is `surface` at that alpha, then raised by
`contrast_checked_glass_alpha` until the shell's text roles stay legible. The
comment records why 0.80 and not the reference app's 0.76: the reference used
Electron's `under-window` vibrancy material, which pre-darkens the blur. A bare
backdrop blur has no material layer, so the scrim runs heavier to land on the
same perceived tone.

Light frost deliberately runs at the same alpha as dark. A light tint controls a
blur less than a dark one, so equal-looking light frost lets the desktop colour
bleed through.

### 2.3 In-app frost

[`crates/ui/src/frost.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/frost.rs)
is 60 lines of real code and it is the highest-leverage file in the repository.

```rust
pub const MENU_BLUR: f32 = 16.0;

pub fn frosted(corner_radius: f32, blur_radius: f32, child: impl IntoElement) -> Frosted
```

`Frosted` is a custom `Element` that forwards layout and prepaint, then in
`paint`:

```rust
window.paint_layer(bounds, |window| {
    window.paint_backdrop_blur(bounds, Corners::all(px(radius)), px(blur));
    self.child.paint(window, cx);
});
```

Two rules make it work:

- **The whole card paints inside one `paint_layer`.** Draw order matters. With
  per-primitive bounds-tree ordering, an unrelated hover repaint could reassign
  the card's quads *below* the blur, so washes, dividers and borders were
  intermittently snapshotted and blurred away. Inside one layer the relationship
  is structural: blur first, then shadow, tint, border, rows, text.
- **A nested overlay inside a frosted card needs its own layer.**
  `frost::layered(child)` exists for this. Inside one layer, primitives of equal
  order render grouped by kind (quads, then icons, then images), so a close
  button's circle painted "after" a thumbnail still appears *under* the image.

Blur radius: 16 everywhere, so menus, the command palette and the composer share
one surface. `MENU_BLUR` is a named constant for exactly that reason.

### 2.4 Tint recipes for floating surfaces

All in `theme.rs`. Read them together; they are one system.

```rust
// The card's own fill over its blur.
glass_overlay():
  light -> surface_overlay @ 0.45          // fixed: keep the scene visible
  dark  -> surface_overlay @ contrast_checked_tint_alpha(base = 0.50)

// The composer pill and dark floating surfaces.
composer_surface_bg():
  frost + dark -> composer_sidebar_tint()  // solves for the alpha that lands on
                                           // the target tone; returns alpha 0.15
  else         -> input_glass_bg()

// Inputs, queue tray, panels.
input_glass_bg():
  no frost -> flatten(input_bg, bg)
  light    -> that tint @ 0.35
  dark     -> input_bg @ contrast_checked_tint_alpha(base = input_bg.a)

// Settings cards and in-panel cards.
card_glass_bg():
  no frost -> surface
  else     -> surface @ contrast_checked_tint_alpha(base = 0.40)
```

Two judgement calls recorded in the comments and worth repeating:

- Light glass keeps the blurred scene visible. An earlier contrast guard against
  solid black raised the coverage to 85–100%, which selected opaque material even
  when the user asked for frost. Shortening the guard is better than lying about
  the setting.
- For a light appearance, strengthen the *foreground* instead of increasing the
  fill. `for_popup()` mixes muted text toward the primary text until it clears
  4.5:1 over the composited background. Increasing coverage is the dark-mode
  answer; on a bright field it kills the material.

The dark composer tint is the only real solve in the file. `composer_sidebar_tint`
computes the minimum alpha that moves the composited canvas onto the target tone,
then recovers the source RGB by un-compositing, and returns it at a fixed 0.15
coverage. If we want the composer to match a panel behind it, that is the shape
of the calculation.

### 2.5 Plates: lit surfaces for controls

[`crates/ui/src/glass.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/glass.rs).
Each plate is a vertical gradient, a hairline rim, an inner top highlight and a
soft drop. `Plate` has both an `apply` for styled elements and a `paint` for
canvases, so the same recipe works in either place.

Neutral plate, dark:

```
background  vertical(white 0.08 * t  ->  white 0.05 * t)
rim         white 0.09 * t
inset       white 0.07, y = 1, blur = 0
drop        black 0.16, y = 1, blur = 2
```

Neutral plate, light — note that the shadows are **inset only**. GPUI paints drop
shadows under the whole box, so an outer lip would show through the translucent
fill and whiten it.

```
background  vertical(black 0.075 * t ->  black 0.04 * t)
rim         black 0.08 * t
inset       black 0.08, y = 1, blur = 2
inset       white 0.55, y = -1     // lit inside the bottom edge
```

Accent plate (Stop button, enabled switch, slider fill):

```
top         lift(accent_strong, 0.06 dark / 0.22 light)
background  vertical(top @ t -> accent_strong @ t)
rim         dark  lift(accent_strong, 0.35) @ 0.35
            light mix(accent_strong, black, 0.14)
inset       white 0.12 dark / 0.32 light
inset ring  white 0.00 dark / 0.22 light, blur 1
halo        accent @ (0.0 dark / 0.14) + 0.30 * glow, spread grows with glow
```

Dark plates sit flat at rest. Light plates keep a faint coloured glow. That is
the same "light is designed, not inverted" rule applied at the control level.

Thumb plate: the light-appearance neutral plate in *every* appearance, so it
stays solid over translucent dark glass. Rim `black 0.12` dark, `0.11` light.

The `t` parameter fades the entire treatment in. That is how a hover or an
activation blends a control from flat to lit without a second recipe.

### 2.6 Edge fades

[`crates/ui/src/edge_fade.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/edge_fade.rs),
used 31 times in the app.

`edge_faded(band, top, bottom, child)` wraps a subtree in a gpui `EdgeFade`
scope. Primitives inside fade by their distance to the wrapper's own edges, with
per-pixel text opacity. That produces a true static gradient instead of a
whole-row opacity.

This is required, not decorative. Over a see-through blurred backdrop, no painted
overlay can fade content out, because "what is behind the window" is not a
paintable colour. The sidebar's `SIDEBAR_GLASS_FADE_BAND` is 24, matching
`TRANSCRIPT_FADE_BAND`.

One implementation note: gpui *replaces* nested `EdgeFade` scopes rather than
composing them. The module keeps the active wrapper scopes in a global
`PaintFades` keyed by `WindowId` and re-inherits the parent's vertical band onto
a nested label, so a text label keeps its scroll container's fade.

### 2.7 Platform matrix

| | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Window frost | Native vibrancy | Acrylic | Opaque. Compositor blur is not guaranteed and a transparent window would expose the raw desktop. |
| In-app frost | Metal | Bounded Direct3D `BackdropBlur` | wgpu raster path |
| Window background | `Blurred` | `Blurred` | `Transparent`, for CSD corners only |

The surface preference (`Theme default` / `Frosted` / `Opaque`) stays portable
even where a given surface cannot honour blur. Frost is derived from the mapped
theme roles, not from fixed greys, so forcing frost onto an imported palette does
not erase that palette's identity.

### 2.8 Traps to carry over

1. Re-apply the window background appearance after every theme change.
2. One `paint_layer` per frosted card. Never let the blur and its content share a
   layer with the rest of the view.
3. Nested overlays inside a card need `layered()`.
4. Gate window glass on the resolved alpha, not on the platform constant.
5. Fills rest on `ink(0.0)`, never on transparent black. Opaque washes killed the
   glass and flashed dark mid-fade.
6. Light mode strengthens foregrounds; dark mode strengthens fills.
7. Contrast-correct the composited result, not the source colour. The backdrop
   under a translucent tint is what the text actually sits on.

## Part 3 — Motion

[`crates/ui/src/motion.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/motion.rs)
(1,460 lines) is a self-contained kit over gpui `Animation`.

### 3.1 Curves and catalog

`CubicBezier` solves `x(t) = input` by Newton iteration with a bisection
fallback, and exposes itself as a gpui easing closure. `eval` clamps its output
hard to `[0,1]`: f32 rounding pushed `sample_y` to 1.000000119, and gpui's
animation element asserts the delta range and aborts.

| Name | Value | Use |
| --- | --- | --- |
| `EASE_OUT_EXPO` | (0.16, 1, 0.3, 1) | Signature entrance |
| `EASE_OUT` | (0, 0, 0.58, 1) | Width and height transitions |
| `EASE` | (0.25, 0.1, 0.25, 1) | Fades, menu and dialog pops |
| `EASE_OUT_QUINT` | (0.22, 1, 0.36, 1) | List resort glide |
| `EASE_IN_OUT` | (0.42, 0, 0.58, 1) | Scroll glide |
| `EASE_TAILWIND` | (0.4, 0, 0.2, 1) | Hover colour blend |

| Spec | Duration | Notes |
| --- | --- | --- |
| `FADE_IN` | 500 ms, expo-out | Entrance, 4px rise |
| `FADE_QUICK` | 150 ms | Opacity only |
| `MENU_IN` | 140 ms | Popover |
| `MENU_OUT` | 100 ms | Exits are shorter than entrances |
| `DIALOG_IN` | 180 ms | |
| `SPLASH_OUT` | 500 ms after a 150 ms hold | |
| `RESIZE` | 200 ms ease-out | Pane width and height |
| `TAB_SLIDE` | 150 ms | Tab drag-reorder |
| `COLLAPSE` | 180 ms | Per-file collapse |
| `RESORT` | 260 ms quint | Sidebar row positions |
| `HOVER_FADE` | 150 ms Tailwind | Every interactive wash |
| `STICK` spring | see 4.3 | Transcript follow-tail |

`translateY` is implemented as a relative-position `top` inset. Taffy applies
relative insets after layout, so siblings never move, the same way a CSS
transform behaves.

gpui has no scale transform for `div`s at this revision, so the scale component
of `MENU_IN` and `DIALOG_IN` is approximated with fade plus translate. Yaw the
same check when we adopt `gpui-kit` 0.6.1.

### 3.2 A shared pulse clock

Running the loaders as `gpui with_animation(..repeat())` was a real bug: one
Working row pinned the whole window at 120Hz, measured at 36% CPU on an M-series
laptop, with the always-hot Metal pipeline holding hundreds of megabytes of
graphics buffers.

The fix is one global clock:

```rust
const PULSE_TICK:  Duration = Duration::from_millis(33);   // ~30fps
const PULSE_LEASE: Duration = Duration::from_millis(300);
```

Each view that paints a spinner registers a lease. A background task ticks every
33 ms, renews leases, and calls `cx.notify(view)` for every lease that is due.
When no lease renews, the clock parks itself and schedules nothing. A view drops
off automatically when it stops painting, so an unmounted spinner costs nothing.

Two refinements worth copying:

- Leases carry a **stride**, so a coarse 15Hz cell loader and a 30Hz text dissolve
  can share one clock without the fast one dragging the slow one up.
- All cells across all views derive phase from one shared epoch
  (`(epoch.elapsed() / period).fract()`), so multiple loaders stay phase-locked
  instead of beating against each other.

### 3.3 Hover fades off the render pass

gpui `.hover()` styles snap: the style applies the frame the pointer enters.
Zeron wants Tailwind's 150 ms `transition-colors` everywhere, so it keeps a
`thread_local` store of per-key hover progress:

- `set_hover(key, hovered, reduced)` records the direction change, re-anchoring
  at the current value so a mid-flight reversal stays continuous.
- `hover_t(key)` returns the eased progress for this frame.
- A once-per-frame `tick_at` advances a frame counter, prunes entries that
  settled at rest, and **prunes any entry that went a full frame unread**. An
  element that unmounts mid-hover never sends a leave event; without the liveness
  stamp a reopened menu inherits a dead entry's wash.

It is a thread-local rather than a gpui global so that free-function element
builders can read it without threading `cx` through every signature.

### 3.4 FLIP resort

`shell.rs::resort_offsets(old, new, gap)` takes the previous keyed order and the
new one and returns, for each surviving key, the paint-only start offset
`old_y - new_y`. Only keys that moved more than 0.5px are included. The list then
glides each row to zero over `RESORT`.

The guard `sidebar_key_order_changed` prevents height changes from triggering it:
sidebar disclosures animate their own height and must not also FLIP every
following section.

### 3.5 Reduced motion

gpui's `App::reduce_motion` flag is honoured automatically by every
`with_animation`: one-shot animations snap to their end state, repeating ones to
their start state, and no frames are scheduled. Zeron wraps it as one global
switch with an optional "pause animations while the window is unfocused", and
pure helpers take the flag explicitly where they run outside an element.

## Part 4 — Performance

The repository ships a `docs/performance*` family with raw JSON and CSV beside
the prose. Copy the habit, not only the findings.

### 4.1 Measure first, then gate

`docs/memory-plan.md` states the acceptance targets before listing the changes:
viewer laptop under 250MB after browsing 20 chats, RSS flat within ±10% over
eight hours, engine idle under 40MB, reopen p95 under 100ms. Each phase lands
with before/after numbers in the PR.

Two measured facts decided the whole plan:

1. Cold-open from a local snapshot was within about 11ms of a warm in-memory
   document, even for a 1.6MB chat, in a debug build. Eviction is therefore not a
   feel trade.
2. Nothing was ever released. RSS was monotonic in chats-ever-touched and
   images-ever-viewed, plus allocator watermark.

### 4.2 Allocator

This is a trap worth knowing. Zeron adopted `mimalloc` on macOS to fix a
libmalloc high-water-mark ratchet, then the crate's default moved to mimalloc
v3.3.2, which retains streaming churn as permanent RSS itself. Measured on Linux:
the daemon held 908MB for 7.3MB of documents with no idle recovery, while glibc
stayed flat on the same workload.

Final setup:

- macOS only, pinned to the crate's `v2` feature.
- Linux runs the system allocator, plus a thread that calls `malloc_trim(0)` once
  a minute. That measured about 265MB and 18 CPU-seconds against glibc's default
  of about 390MB. The automatic top-of-heap trim does not reach pages freed
  inside a thread arena.

### 4.3 Transcript: virtualization and follow-tail

`crates/ui/src/transcript.rs` (14,198 lines) is the model for any long
scrollback.

- Row model is **block-granular**: one row per markdown top-level block, not one
  row per message. Ids are stable (`{msgId}#{partId}.{blockIx}`), and live
  streaming rows split exactly like completed ones, so completion never
  flickers.
- Row caches key on a content fingerprint, so a streamed token rebuilds one row.
- Row-set changes diff by `(id, version)` into one minimal `ListState::splice`.
- `OVERDRAW_PX = 320`, `STICK_THRESHOLD_PX = 70`.

Follow-tail is a velocity spring with a feed-forward term, not a scroll-to-end
call:

```rust
SPRING_DAMPING       0.7      SPRING_STIFFNESS    0.05
SPRING_MASS          1.25     SPRING_FRAME_MS     1000/60
SPRING_MAX_CATCHUP   8 frames SPRING_GROWTH_EMA   0.12
SPRING_CHASE_MAX_LEAD 32 px
```

Each tick computes a smoothed estimate of how fast the target is growing and adds
it to the position, so commits read as one continuous glide instead of a snap per
commit. The chase target leads the true end by at most 32px. The spring never
overshoots, is monotone while approaching, and snaps exactly once within 0.5px.
The pin breaks only on user input and re-engages inside the 70px band.

### 4.4 Streaming text

`crates/ui/src/markdown/veil.rs` dissolves newly arrived characters:

- Duration adapts to the stream cadence: `clamp(ema_of_gaps * 3, 120ms, 400ms)`.
- Curve: `veil = (1 - p)^1.6`, text alpha `1 - veil`.
- A chunk fades exactly once. A settled element returns no spans at all.
- **Zero translate.** Opacity only, per the "no positional offset on streamed
  content" rule.

It is paint-only by construction. The veil multiplies alpha into `TextRun`
colours, and cosmic-text ignores colour when deciding shaping compatibility, so
kerning, ligatures and wrapping are byte-identical to an unsplit render.

### 4.5 Images

Directly relevant to a photo app.

[`crates/ui/src/image_media.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/image_media.rs):

```
PREVIEW_PIXELS   1 MiB target      MAX_RASTER_SIDE  4096
GPUI_SVG_SCALE   2.0               // gpui rasterizes SVG at twice its declared size
```

Decoding is bounded and SVG is resolved in memory, never on the UI host.

The important finding: **`gpui::ImageSource::evict` is required.** Calling
`remove_asset` alone leaked sprite-atlas tiles. Zeron releases batches through
`cx.defer`:

```rust
cx.defer(move |cx| {
    for image in images {
        gpui::ImageSource::Image(image).evict(None, cx);
    }
});
```

The memory plan still lists a follow-up here: atlas tiles for raw-bytes images
free only on window close, which needs another gpui-fork patch exposing a drop
path for `ImageSource::Image`. For a grid of photos this is the highest-risk
area, and the first thing to measure.

Also worth taking:
[`crates/ui/src/new_thread_background_mask.rs`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/crates/ui/src/new_thread_background_mask.rs)
does paint-time source-alpha feathering with a rounded cutout. Resizing changes
only GPU parameters, never the image identity, the pixels, the atlas entry or an
asynchronous raster job. That is the right shape for a large preview with rounded
corners.

### 4.6 Idle cost

`docs/performance-idle-presence.md` is the best single example of the process.
The complaint was a footprint jump. The investigation traced about 210MiB to
*owned, unmapped graphics memory*, then isolated it with a standalone probe
(`scripts/macos-metal-memory-probe.m`) that submits one GPU command every 15
seconds with no window, no history and no network. Result: the transient charge
is driver work, not application textures. A blit-only submission settles at
6.1MiB and peaks at 94.4MiB; a blit plus render peaks at 243.2MiB; the reported
Metal allocation size stays at 20.53MiB throughout.

Conclusion: reduce the number of submissions, and accept that a flat footprint
while rendering is not promised.

The two fixes are general:

- An unchanged visible presentation must not invalidate the app. Heartbeats still
  update their timestamps; only a real presentation change redraws.
- The renderer must actually stop its display link when frame requests park, and
  restart it safely on the next request.

Measured over two idle minutes on the same conversation and window:

| | Before | After |
| --- | ---: | ---: |
| Average CPU, % of one core | 2.114 | 0.749 |
| Peak 500 ms CPU | 69.99 | 12.53 |
| Package idle wakeups/second | 8.305 | 0.356 |
| Mean physical footprint, MiB | 202.70 | 171.19 |

### 4.7 Protocol and data changes that paid

From the memory plan's status section:

- Delta document protocol instead of full snapshots. Measured on a 1.6MB
  streamed reply: 257MB of watch frames before, 2.3MB after, a 110× reduction.
  Median frame 2.7KB.
- Bounded RPC stream queues (256) with backpressure.
- An LRU on warm documents (12 warm, 80MB estimate) with pins for watched,
  live-writer and host-pending-commands states.
- Attachment images under a 64MB encoded LRU, with gpui asset release on
  eviction.

### 4.8 Fork patches worth porting

The pinned gpui fork (`https://github.com/zeronsh/zui`, rev
`667d0aaf9531d2d1b2d0674a5e55f977df1b09f6`) carries these, each with a reason:

- Bounded renderer GPU memory: region-sized backdrop-blur scratch, idle release
  of blur and path intermediates, instance pool shrink. Diagnostics behind
  `ZERON_GPU_STATS=1`.
- `ImageSource::evict`, to free sprite-atlas tiles.
- Destination alpha on transparent windows uses Porter-Duff OVER, not additive.
- Horizontal `EdgeFade` for quads and images, so content fades into glass.
- `object-fit: cover` cropping via atlas-tile UVs, so image corner radii hold.
- The macOS blurred view switched to `UnderWindowBackground`, because macOS 26
  stopped vending `CABackdropLayer` for `Selection` and window blur went dead.
- `BackdropBlur` rasterized in the wgpu renderer, so frost works on Linux.

Check which of these already exist in the gpui pre-release that `gpui-kit` 0.6.1
pins before planning any port.

## Part 5 — Process worth copying

- `docs/research/` holds technique and library reports with pinned revisions and
  license tables. `docs/adr/`, `docs/plans/` (dated), `docs/performance/` (raw
  data), `docs/regressions/`.
- Module docs explain *why*, name the rejected alternative, and cite the user
  report that forced the change. Many are longer than the code beneath them.
- `docs/theme-system.md` ends with a visual QA matrix: ten fixture scenes
  (sidebar, transcript markdown, transcript code, composer, picker, appearance
  settings, diff, terminal, empty state, dialog) crossed with ten states
  (normal, hover, active, focused, selected, disabled, working, warning, error,
  success). Automated contrast checks are gates, not substitutes for that pass.
- `crates/ui/examples/` holds named scene fixtures (`sidebar-fixture`,
  `command-palette-fixture`, `new-project-fixture`, `appshots-fixture`) behind
  cargo features. That is how a visual change gets reviewed without a full app
  run.
- `docs/theme-system.md` also states the transport rule for continuing to look
  right: any change to appearance or surface must be checked over every scene, in
  both appearances, at both frost settings.

## Part 6 — What to take for the photo prototype

Ordered by value per unit of effort.

1. **`frost::frosted` plus the tint recipes.** Highest visual return. Needs
   `Window::paint_backdrop_blur` in our gpui build — verify first.
2. **The role table and the light/dark rules.** Replace any hand-picked colour in
   the app with a role, and run the contrast-pairing test.
3. **The plate recipes.** Rounded rectangles with a rim and an inset highlight
   are most of the "expensive" look of buttons and chips.
4. **The pulse clock.** Any grid shimmer or import spinner routes through it, not
   through per-tile `with_animation`.
5. **`ImageSource::evict` on every preview that leaves the viewport.** The single
   biggest correctness and memory risk in a gallery.
6. **FLIP resort offsets** for filter changes and re-sorts.
7. **Hover fades via a thread-local progress store**, since `gpui-kit` hover
   styles snap.
8. **Edge-faded scroll regions** over the sidebar and the grid.
9. **The measurement habit**: a `docs/photo-gallery/performance/` folder with raw
   runs beside the prose, and thresholds that fail closed.

### Open questions to answer before adopting

- Does the gpui that `gpui-kit` 0.6.1 pins expose `paint_backdrop_blur`,
  `paint_layer`, `EdgeFade`, `ImageAlphaMask` and `ImageSource::evict`?
- Does it have a scale transform for `div`s? If not, `MENU_IN` and `DIALOG_IN`
  become fade plus translate, as they did for Zeron.
- Does `gpui-kit`'s component set already own the popover, dialog and command
  palette surfaces? If so, wrap its card in `frosted` rather than rebuilding it.
- Which component-layer scrollbar sync, if any, replaces Zeron's
  `sync_gpui_base_scrollbar`?

## Part 7 — Caveats

- Zeron builds on a private fork of gpui with patches we do not have. Anything
  touching `BackdropBlur`, `EdgeFade`, `ImageAlphaMask` or `ImageSource::evict`
  needs a port decision.
- Most of the look is hand-rolled. Zeron uses `gpui-base` only for a few widgets
  and for scrollbar syncing. Do not read this repository as a component-library
  usage example.
- `zeron-proto` contains pure phase and view helpers shared by both frontends.
  The split is good practice, but it is a layering decision with real cost; take
  it only if we grow a second frontend.
- The MIT grant covers Zeron's own source. `gpui-base` is Apache-2.0, the bundled
  Geist fonts are under SIL OFL 1.1, the bundled Solar Icons Linear set is CC BY
  4.0 (480 Design), and the gpui fork inherits Apache-2.0 from Zed. Carry the
  notices if we copy assets. See
  [`THIRD_PARTY_NOTICES.md`](https://github.com/zeronsh/zeron/blob/7a472fce41b6a834da87b460e189adb05eacf3d4/THIRD_PARTY_NOTICES.md)
  and `crates/ui/assets/fonts/licenses/`.
- Bundled UI fonts are 2.2MB of Geist; file icons are another 2.3MB. Budget for
  binary size if we follow the asset approach.

## Part 8 — File index

Paths are relative to the pinned revision.

| Area | File | Lines |
| --- | --- | ---: |
| Theme roles and recipes | `crates/ui/src/theme.rs` | 3,028 |
| Theme model and importer | `crates/theme/src/lib.rs`, `crates/theme/src/vscode.rs` | 836, 1,699 |
| Built-in variants | `crates/theme/src/builtins.rs` | 1,338 |
| In-app frost | `crates/ui/src/frost.rs` | 177 |
| Glass plates | `crates/ui/src/glass.rs` | 161 |
| Edge fades | `crates/ui/src/edge_fade.rs` | 451 |
| Motion kit | `crates/ui/src/motion.rs` | 1,460 |
| Loaders | `crates/ui/src/loaders.rs` | 406 |
| Typography | `crates/ui/src/typography.rs` | 886 |
| Icons and assets | `crates/ui/src/icons.rs` | 288 |
| App shell and layout | `crates/ui/src/shell.rs` | 16,652 |
| Transcript | `crates/ui/src/transcript.rs` | 14,198 |
| Composer | `crates/ui/src/composer.rs` | 14,113 |
| Popover and dialogs | `crates/ui/src/popover.rs` | 2,546 |
| Image handling | `crates/ui/src/image_media.rs`, `image_viewer.rs`, `new_thread_background_mask.rs` | 426, 572, 597 |
| App bootstrap and window | `crates/ui/src/lib.rs` | 598 |
| Appearance switching | `crates/ui/src/appearance.rs` | 411 |
| Toolbar metrics | `crates/ui/src/surface_chrome.rs` | 46 |
| Theme system doc | `docs/theme-system.md` | 174 |
| GPUI build guide | `docs/research/gpui.md` | 188 |
| Memory plan and results | `docs/memory-plan.md`, `docs/performance-idle-presence.md` | 172, 181 |
| Scene fixtures | `crates/ui/examples/*-fixture.rs` | — |

## Part 10 — Second pass: what the first capture got wrong

The first capture measured the *planes* and called the work verified. Judging
the same capture as a design found four things, three of them real defects.
Recorded here so the numbers are auditable.

### Structural changes

- **One chrome band, not two.** A native titlebar sat above the toolbar and both
  painted the chrome plane, so the top of the window read as one flat slab. The
  strip is now the library's own `TitleBar` component holding the window's
  object, its import actions and the count, with the system titlebar hidden and
  the traffic lights placed by the library.
- **A photo is selected on open.** The app opened with nothing selected, so the
  inspector was an empty slab and the next action was not offered. `upsert`
  repairs it when a library opens while its first import is still running.
- **The status line stopped repeating the title strip's count** and now carries
  the one thing the grid cannot show: the weight of the originals.
- **The grid has a gutter.** The tiles were flush to the window edge.

### Defect one: every unselected tile carried a visible rim

`selection_ring`'s unselected case returned the palette's `border`, so every tile
in the grid carried a 2px visible rim where the original had a transparent one.
That is what made the grid read as a field of outlined boxes rather than
photographs on a plane. Unselected is now transparent, and a test pins it.

### Defect two: the accent role was wrong, and no repair could save it

The ring colour came from `theme.selection`, which is a **30%-alpha wash**, not
an opaque colour. Used as a ring it composited into the plane behind it. Measured
from the first capture: the ring painted `#386e64` at **hue 169°** — the accent is
a blue at 224° — and no contrast repair could reach 3:1, because at 30% coverage
no lightness can. Both the teal and the unreachable budget came from the same
mistake.

The ring now uses `theme.list_active_border`, the library's own opaque
selected-border colour, and the wash uses `theme.selection`. Both are authored as
a pair, so the derivation I had invented is gone. A test pins that the ring is
opaque and the wash is not.

### Defect three: contrast repair destroyed the hue

`ensure_contrast` mixes toward black or white, and `mix` lerps the hue and the
saturation along with the lightness. Lifting a dark blue toward white on the
near-black content plane walked the hue across the wheel and drained the chroma.
This is the exact failure Zeron documented when they stopped repairing at
runtime and authored their accent pairs instead.

`lighten_to_contrast` raises lightness alone and keeps the hue and saturation.
Measured on the final capture: the ring is `#1e51e1` at **hue 224°**, saturation
0.87 — the colour the palette chose, with its budget repaired. A test pins that
neither the hue nor the saturation moves.

### Two test-fixture bugs, found the same way

1. `Theme::from(&ThemeColor)` leaves `mode` at the library's own default, which
   is **Light**. A dark palette handed to `Design::new` through that fixture was
   therefore resolved by the light code path, and every dark assertion passed by
   coincidence. The fixtures now name the mode.
2. The synthetic-photo generator wrote a PNG filter byte inside the per-*pixel*
   comprehension instead of once per row. Every scanline was malformed, vips
   misread the images, and two captures showed photos averaging 26/255 when the
   sources average 133. Two rounds of judging the design on broken fixtures came
   from one line.

### Final measurements, from the second-pass capture

| Region | Value |
| --- | --- |
| Title strip | `#141414` |
| Content plane | `#0a0a0a` |
| Inspector | `#141414` |
| Status bar | `#171717` (the library's own opaque treatment) |
| Selection ring | `#1e51e1`, hue 224°, saturation 0.87, 2px, with a 1px inner highlight |
| Tests | 66 unit, 15 integration |

### Still unverified

- **The light appearance has never been captured.** The harness follows the OS
  appearance and nothing in the scripts can pin it, so the light planes are
  covered only by tests.
- **The hover plate and the viewer's glass bar are not in any capture.** Both
  need a pointer or a double-click that the smoke run does not perform.
- Only the dark appearance has been measured pixel by pixel.

Start with `frost.rs`, `glass.rs`, `motion.rs` and `theme.rs`. Those four files
carry most of the look, and they are the four smallest things that explain it.

## Part 9 — Implementation status

Implemented in `apps/desktop/src/ui/design/` on 2026-10-01, against the same
revision as the rest of this document.

### Answers to the Part 6 open questions

| Question | Answer |
| --- | --- |
| Does `gpui-pre 0.3.4` expose `paint_backdrop_blur`, `paint_layer`, `EdgeFade`, `ImageAlphaMask`, `ImageSource::evict`? | `paint_layer` — yes. The other four — **no**. All four are Zeron fork patches. |
| Does it have a scale transform for `div`s? | No. Menus and dialogs are fade plus a relative `top` inset, exactly as in Zeron. |
| Does `gpui-kit` own the popover, dialog and palette surfaces? | Yes. `gpui-component` ships `Sheet`, `Dialog`, menus and popover styling, so we wrap its surfaces rather than rebuild them. |
| What replaces `sync_gpui_base_scrollbar`? | Nothing needed. The library's own scrollbar already tracks the scroll handle. |

### What landed

| Module | Contents |
| --- | --- |
| `design/color.rs` | `flatten`, `mix`, WCAG luminance and contrast, `ensure_contrast`, `contrast_checked_alpha`, and the ink/hairline/wash helpers with their two scales. |
| `design/mod.rs` | [`Design`] — the planes and the elevation ladder, derived from the library's own palette rather than a second palette. Geometry constants. Surface preference. |
| `design/glass.rs` | Window glass and its re-apply rule, the chrome tint, card/overlay/field tints, `overlay_text`, `light_plate`, `selection_ring`, `frosted`, `edge_scrim`. |
| `design/motion.rs` | The bezier solver, the `MotionSpec` catalog, one-shot element helpers, and the shared 33ms `FrameClock` with leases and self-parking. |
| `design/hover.rs` | Thread-local hover fades riding that clock. |

Wired into the app:

- **Window** — `WindowBackgroundAppearance::Blurred`, applied at creation and
  re-applied on every appearance change. `Root`'s own background is transparent,
  because it would otherwise paint the blurred desktop away.
- **Chrome** — toolbar, inspector and titlebar painted at the resolved glass
  alpha. Measured from a Metal scene capture: `#141414` over a `#0a0a0a` content
  plane, so the tint resolved to about 0.77 coverage, under its 0.80 cap, and the
  contrast check did not have to densify it.
- **Grid** — opaque content plane, with edge scrims top and bottom.
- **Tiles** — the card plane at `PANEL_RADIUS = 10`, a selection ring with an
  accent halo, and a hover plate that fades in over the frame. The plate child is
  only added while a fade is in flight, so a tile at rest costs no children.
- **Viewer** — a scrim over the grid, a floating glass bar, and a dialog-in
  entrance.
- **Hover** — enter and leave recorded per tile; only a real change asks for a
  redraw, so a pointer sweep costs two paints per tile rather than one per move
  event.

### What did not land, and why

| Item | Reason |
| --- | --- |
| In-app backdrop blur | No primitive in `gpui-pre 0.3.4`. [`frosted`] paints the tint, border and shadow; the blur call is documented at the site. |
| Per-pixel edge fade | No `EdgeFade`. `edge_scrim` gradients into an opaque plane, which is exact on the content plane and approximate over glass. |
| `accent_plate`, `thumb_plate` | This app's filled controls are library buttons, so an accent plate has no call site. `selection_ring` is the accent equivalent it does need. |
| Image eviction | Zeron's `ImageSource::evict` fixes a leak in a different image API. This app already evicts through a bounded LRU over the library's `ImageCache::remove`. |
| Surface-preference settings UI | The resolver and its tests exist; the app has no settings screen yet and resolves to the theme default, which is frost. |

### How it was verified

- 63 unit tests — colour math, glass alphas and contrast in both appearances,
  plate recipes, the catalog, clock lease mechanics, hover fades — plus the 15
  existing library integration tests.
- `cargo fmt`, `cargo clippy --all-targets` and a release build clean. The one
  remaining clippy warning is pre-existing in `library.rs`, which this work did
  not touch.
- Smoke run against a fresh library: three synthetic 1600×1200 JPEGs imported,
  derivatives generated, two Metal scene captures rendered, no errors on exit.
- Measured from that capture, per plane: content `#0a0a0a`; toolbar, inspector
  and titlebar `#141414`; status bar `#171717` (the library's own opaque
  background, untouched by design); a photo tile painting `#c73c3b` from a
  `#c83c3c` source, so photographs render true and are not tinted by their frame.

### Third pass: sidebar and title-strip spacing

- **The sidebar is the library's own `Sidebar` component** — width, the 200ms
  collapse transition, the `sidebar` tokens and the footer geometry all come from
  the component family rather than from hand-rolled divs. Its width is set
  explicitly from `design::SIDEBAR_WIDTH` so the grid can subtract a real number
  instead of guessing the component's private default.
- **Spacing.** The strip sets a leading pad for the traffic lights and no
  trailing pad, so the trailing control was glued to the window edge; `pr` fixes
  it. The title takes a trailing margin of its own, because the component gives
  its children no gap and six children pushed the title against the Import
  button.
- **A bug this introduced and its fix.** Adding the sidebar without subtracting
  its width left the column count one too high, so the rightmost column was
  clipped under the inspector. `layout()` now subtracts both panels.

### Fourth pass: the glass was never visible

**Window glass was configured but structurally invisible.** The window asked the
compositor to blur the desktop and the tint resolved to a translucent 0.80, so the
code looked right and every earlier note called it working. It could not show, for
two reasons:

1. The gallery root painted an **opaque plane across the entire window**, so the
   translucent chrome composited over the app's own plane instead of over the
   blurred desktop. Arithmetic on the capture proves it: inspector `#141414` is
   exactly `#171717` at 0.80 over the content plane's `#0a0a0a`.
2. The sidebar, title strip and status bar are library components that paint their
   own **opaque** tokens (`sidebar`, the title-bar gradient,
   `status_bar.background`), so the only translucent surface was the inspector,
   and it sat over an opaque plane.

Measured before the fix: every region of the capture had **alpha 255**.

After the fix the root paints nothing, so the window's blurred backdrop is behind
the chrome; the grid region owns the one opaque plane, because the gaps between
tiles must not show the desktop; and the sidebar, title strip and status bar
override their opaque tokens with `design.chrome()`. Measured: chrome **alpha
204** (0.80 × 255) across the title strip, sidebar, inspector and status bar, and
255 for the content plane.

The material is now asserted at runtime instead of assumed.
`PHOTO_GLASS_DEBUG=1` prints `glass: material=Blurred tint_alpha=0.800` — the
check that should have existed the first time.

Legibility over an unknown desktop is already guarded: `contrast_checked_alpha`
raises the tint until the chrome's own text clears 4.5:1 against the adverse
backdrop. At 0.80 the chrome keeps primary text near 9:1 even against a white
desktop, so the guard does not densify it and the glass stays glass.

**What a scene capture still cannot show.** `Window::render_to_image` captures the
app's own scene, so the desktop behind the window is not in the image. The alpha
channel is the proof the chrome is translucent; the blurred wallpaper itself has
to be seen in a real window.

### Third-party attribution

The colour math, glass recipes, plate recipes, motion catalog, frame clock and
hover fades are ported from Zeron's `crates/ui`, MIT licensed, Copyright (c)
2026 Wing. The MIT licence requires that its copyright notice accompany the
software, so the notice is carried in `apps/desktop/THIRD_PARTY_NOTICES.md`. Each
module names its source file in its own module documentation.
