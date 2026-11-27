# Changelog

All notable changes to **Liebert Dynamic Island** are documented in this file.
Versioning follows `X.Y.Z` with automatic carry-over at `10` (`Z >= 10` → `Y += 1, Z = 0`; `Y >= 10` → `X += 1, Y = 0`).

---

## [0.2.0] - 2026-11-27

### Added
- **Music-Playing Mascot Animation (Windows & Linux)** (`src/ui/icons.rs`, `src/ui/mod.rs`): When media playback is active, the idle mascot transforms its mouth into **3 animated vertical equalizer bars** (+ right accent dot), sways/nods gently left and right, and performs periodic **"yepii"** upward bounce pulses (`1.0x → 1.16x`) while keeping cursor eye-tracking, blinking, and glow active across the music pill, Mascot/Clock idle modes, and Home Clock tile.
- **Notification Countdown Bar Shrinking Toward Center (Windows & Linux)** (`src/state.rs`, `src/ui/mod.rs`): Active notification popups (`Compact`) display a colored timer bar at the bottom edge whose width shrinks symmetrically toward the horizontal center as auto-dismiss approaches (and pauses at full width when hovered/expanded).
- **Swipe-to-Dismiss Notifications (Windows & Linux)** (`src/ui/mod.rs`): Dragging/sliding any notification card horizontally inside the `Notifications` tab reveals a red delete indicator and dismisses/removes the item cleanly.
- **`Glassmorphism` Material & Improved Glass Legibility (Windows & Linux)** (`src/config.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`): Added a 5th background material option (`Glassmorphism`) featuring ambient dual-orb bloom, dark contrast scrims, top specular sheen, and luminous rim borders. Raised glass opacity floors (`178–224`) and boosted `text_color32()` / `muted_color32()` brightness across `Acrylic`, `Frosted Glass`, `Liquid Glass`, and `Glassmorphism` so text remains sharp on light wallpapers.
- **GitHub Contributors in Settings → About** (`src/ui/settings.rs`): Listed repository contributors (`Zahwan316` · 34 commits, `Gjenius20` / `Zjen` · 8 commits).
- **Release Automation & Versioning Rules** (`AGENTS.md`, `CHANGELOG.md`): Added semver carry-over rule (`0.2.0`) and `dev-main` → `main` changelog + GitHub Releases workflow rule.

### Changed
- Bumped release metadata and User-Agent strings to **`0.2.0`** across `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/{PKGBUILD,.SRCINFO}`, `src/platform/linux.rs`, `src/lyrics.rs`, and `README.md`.
- Rebuilt and verified release artifacts (`Liebert_Dynamic_Island-0.2.0-x86_64.AppImage`, `liebert-dynamic-island-linux-x86_64.tar.gz`, `liebert-dynamic-island-windows-x86_64.zip`, `liebert-dynamic-island-windows-x86_64-setup.exe`, `liebert-dynamic-island-aur-0.2.0.tar.gz`) with `cargo test --offline` **80 passed** and clean Linux + Windows-target `clippy`.

### Fixed
- **Wrong Lyrics Displayed on Track Playback / Tab Open (Windows & Linux)** (`src/lyrics.rs`, `src/ui/mod.rs`): Included 5-second duration buckets in `RawCacheKey` and `EnrichedCacheKey` so initial `duration = 0` lookups do not poison exact-duration matches; enforced strict title/artist/duration (`±25 s`) verification via `lrc_candidate_matches()` and `call_lrclib_get()`; immediately reset lyrics to `Loading` on track changes and validated case-insensitive `(artist, title)` match on `LyricsLoaded` and `apply_snapshot`.
- **Hide Website Names in Brave Notifications (Windows & Linux)** (`src/state.rs`, `src/platform/windows.rs`): Added `is_brave_app()`, `is_web_origin_line()`, and `strip_brave_origin_from_text()` to remove origin domains (such as `web.whatsapp.com`, `youtube.com`, `via chat.openai.com`) emitted by Brave on Linux D-Bus and Windows WinRT.

---

## [0.1.13] - 2026-11-27

### Added
- **Mascot Blink & Element Glow (Windows & Linux):** Added natural ~170 ms eye blink scheduling and dual-pass white glow halos (`paint_mascot_fx`).
- **App Icons & Independent Left-Aligned `Notifications` Tab:** Extracted 28×28 RGBA app icons on Linux D-Bus/desktop/hicolor themes and Windows WinRT `AppDisplayInfo::GetLogo`; added `NOTIFICATIONS` history tab (max 30 items) with inline `Popups`, `DND`, `Test`, `Clear`, and `Settings` actions, strictly left-aligned independent of `text_align`.
- **Interactive Month Calendar Component:** Split `CALENDAR` tab into a 7-column clickable month grid (today ring, selected day pill, event dots) and day/upcoming event list.
- **System Monitor Vector Icons & Settings `About` Tab:** Added `Cpu`, `Memory`, `Disk`, `ArrowDown`, and `ArrowUp` vector icons; added `SettingsTab::About`.
- **Glass Surface Materials (`Solid`, `Acrylic`, `Frosted Glass`, `Liquid Glass`):** Styled pill/expanded/notch surfaces and Settings backdrop.

### Changed
- Rewrote Home `Overview` tab using deterministic explicit-`Rect` geometry (`4×2` Gallery Grid) eliminating nested layout overlap.
- Removed the border line between expanded header and content, and switched expanded header labels and clocks to lightweight Latin Bold fonts (`mmap`).

---

## [0.1.12] - 2026-11-27

### Fixed & Changed
- Sanitized HTML/XML tags (`<a href=...>`, `<br/>`, escaped entities) from WhatsApp Web and browser notifications across Windows WinRT and Linux D-Bus.
- Redesigned Home tab into symmetric `Gallery Grid` tiles and enabled `lto = "fat"` + `opt-level = "z"` + 1× SHM Settings pool allocation (Linux Wayland RAM down to `16.94 MB RSS` Settings / `18.85 MB RSS` Demo).

---

## [0.1.11] - 2026-11-27

### Added & Fixed
- Added cursor-tracking idle Mascot (`IdleDisplayMode::Mascot` + `show_idle_mascot`), Bento Grid Home & Settings UI, pure-CPU `SoftCanvas` Settings windows (Wayland `wl_shm` + Windows `BitBlt`), `memmap2` font sharing, and fixed karaoke double-blend white flicker (`Paint` clip separation + MPRIS2 jitter suppression).

---

## [0.1.5] – [0.1.10] - 2026-11-27

### Highlights
- **v0.1.10:** Uniform expanded width, removed top gap, 180 ms tab slide/fade animations, softer gradient washes, font `Arc` sharing.
- **v0.1.9:** Added `IdleDisplayMode::Battery` and side-arrow content gutters.
- **v0.1.8:** Added per-user OS autostart (HKCU Run / XDG `.desktop`), animated `Battery` tab, donut CPU/RAM/Disk charts, cross-mount disk aggregation, and Linux D-Bus passive `BecomeMonitor` fallback.
- **v0.1.7:** Minimal expanded header, side chevron controls, explicit desktop battery status, and Linux Docker NSIS installer packaging.
- **v0.1.5 – v0.1.6:** Native `/proc` & sysfs System/Battery tabs, keyboard Left/Right navigation, restored 2-column Music card with hash-keyed 2048 px album art decoding.

---

## [0.1.1] – [0.1.4] - 2026-11-26

### Highlights
- **v0.1.4:** Fixed Windows NSIS staging paths and Windows `clippy` cleanliness in CI.
- **v0.1.3:** Auto-sizing centered idle clock, exact `TextAlign` alignment across pill/expanded layouts, `LyricsBold` font family, and blurred full-lyric context scroll.
- **v0.1.1 – v0.1.2:** Multi-player MPRIS2 tracking + `SetPosition` seek, LRCLIB duration calibration, offline Japanese/CJK/Arabic romanization (`kakasi` + `any_ascii`), and Spotify/data-URI/local cover fetching.

---

## [0.1.0] - 2026-11-25

### Initial Release Highlights
- Cross-platform Dynamic Island overlay for **Windows 10/11** and **Linux (Wayland `wlr-layer-shell` + X11 fallback)** built with Rust 2024 and CPU `SoftCanvas`.
- Real-time system notifications (WinRT `UserNotificationListener` & Linux D-Bus), `Idle ↔ Compact ↔ Expanded` spring animations, SMTC/MPRIS2 synced karaoke lyrics, Todo checklist, Pomodoro timer, Google Calendar `.ics`, IANA timezone clock, fullscreen auto-hide, and system tray controls.
