# FlyoutLite

**A lightweight Windows 11 media flyout.** Hit play/pause/next from anywhere — without freezing your game.

[🇹🇷 Türkçe README](README.tr.md)

---

## What it is

FlyoutLite is a tiny native Windows 11 utility that pops up a Fluent-styled media card when you press the media keys (Play/Pause, Next, Previous). It shows the current track's title, artist, and album art, with transport buttons and a click-to-scrub seek bar. It runs from the system tray and stays out of your way.

It is built in **Rust** with **Direct2D + DirectComposition** directly on Win32 — no .NET runtime, no XAML, no Electron, no background services. The release binary is roughly **250 KB**.

## Why another one?

Two excellent projects already do something similar:

- [**ModernFlyouts**](https://github.com/ModernFlyouts-Community/ModernFlyouts) (C#/WPF) — a polished, feature-rich replacement for the legacy Windows flyouts.
- [**FluentFlyout**](https://github.com/unchihugo/FluentFlyout) (C#/WinUI) — a clean modern reimagining in the Fluent style.

Both inspired this project, and both are recommended if they work for your setup.

FlyoutLite was started because, on my own machine, the existing options introduced **multi-second input latency in fullscreen games** (specifically Rocket League) every time a media key was pressed — long enough to cost a goal. The XAML/.NET stack involves cold-path UI work on every keypress, and that interacts badly with exclusive-fullscreen presentation.

So this is a focused rewrite of the same idea with a different goal: **a near-zero-overhead flyout that never makes the foreground app stutter.** The flyout window is pre-created at startup, the render path uses a persistent Direct2D device with a composition swapchain, and the media-key handling is a `WH_KEYBOARD_LL` hook that never consumes events. Nothing is allocated on the hot path.

If you don't have the latency problem, **ModernFlyouts and FluentFlyout are great** and likely a better fit — they have far more features. FlyoutLite is intentionally minimal.

### No shared code

FlyoutLite **does not contain any code from FluentFlyout or ModernFlyouts.** Different language, different UI stack, different architecture — only the high-level idea is shared. Both of those projects are credited above purely because they showed what a good modern flyout looks like and are part of why this category of utility exists at all.

## Features

- Album art, track title, artist
- Shuffle / Previous / Play-Pause / Next / Repeat buttons (shuffle and repeat grey out when the active player doesn't expose them through SMTC)
- Seek bar with click-to-scrub
- Mica backdrop, rounded corners, accent color, follows Windows light/dark theme
- Pops up on media-key press **and** when an app changes track or toggles play/pause from inside its own window (toggle in Settings)
- Tray icon with right-click menu (Settings, Run at startup, Quit)
- Custom-painted Settings window with 9 anchor positions, custom X/Y, margin tuning, visible-duration control, compact mode, "show on track change" toggle, and run-at-startup toggle
- **Compact mode** — a 280×64 mini card with just art + title + artist, no controls
- Single-instance — re-launching the exe doesn't spawn a duplicate
- Does not interfere with media keys (always calls `CallNextHookEx`)
- Hides automatically in exclusive-fullscreen apps
- Run at startup (HKCU `Run` key, no service, no scheduled task)

## Install

Download `flyout-lite.exe` from the [Releases](https://github.com/AhmedCemil/flyout-lite/releases) page and run it. There's no installer — the binary is self-contained.

To launch automatically with Windows, right-click the tray icon → **Run at startup**.

## Build from source

Requires:
- Rust (stable, MSVC toolchain) — install via [rustup](https://rustup.rs)
- Windows 11 SDK + MSVC build tools (Visual Studio Build Tools 2022 or 2026, "Desktop development with C++" workload)

Then:

```powershell
cargo build --release
```

Output: `target/release/flyout-lite.exe`

## Tested on

- **Windows 11 25H2** (the author's machine) — fully working.

That's it. FlyoutLite has not been tested on older Windows 11 builds, Windows 10, ARM64, or in multi-monitor setups beyond the primary display. **If you run it on a different setup and something works (or doesn't), please open an issue** — reports and PRs are very welcome, especially for builds other than 25H2.

## Known limitations

- Primary monitor only (multi-monitor positioning is not yet implemented).
- Settings window text fields don't show a blinking caret yet — focus highlight only. Typing digits and backspace work.
- Some players don't report timeline data (no position/duration); the seek bar shows `-:--` in that case but a click-to-scrub attempt is still made.

## License

[MIT](LICENSE) © 2026 Ahmed Cemil Bilgin
