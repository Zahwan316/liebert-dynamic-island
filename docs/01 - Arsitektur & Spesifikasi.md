---
type: spec
status: active
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - architecture
---

# 01 - Arsitektur & Spesifikasi

Referensi spesifikasi produk: [[PRD-LiebertDynamicIsland-MVP]].

## Tech Stack (Aktual)
- **Bahasa:** Rust (`edition = "2024"`, `rustc 1.98.1` Linux / `rustc 1.99.1` Windows)
- **UI / Renderer:** `egui` CPU rasterizer pure-Rust (`src/ui/soft_render.rs` `SoftCanvas` ke buffer premultiplied BGRA) untuk overlay utama Wayland (`wl_shm`) & Windows (`UpdateLayeredWindow`), ditambah `eframe 0.37` (`Renderer::Glow`, fitur `x11` & `wayland`) khusus untuk jendela `--settings` on-demand dan fallback X11 — lihat [[03 - Catatan Rapat & Keputusan]] (ADR-018)
- **Windows Bridge:** `windows 0.62` — overlay utama `src/platform/windows_overlay.rs` (`UpdateLayeredWindow` + DIB section 32-bit BGRA, `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`, event-driven `GetMessageW` + `WM_APP` wake + `SetTimer`, tanpa GPU ICD); fallback `eframe` di `src/platform/windows.rs` (`apply_no_activate_hook`, `WM_NCCALCSIZE`, DWM attributes, `set_overlay_click_through`), `Win32_UI_Shell` `SHQueryUserNotificationState` + `spawn_fullscreen_watcher` `250 ms` dengan `classify_fullscreen_rects`, WinRT `UserNotificationListener` (+ polling `500 ms`), `Media_Control` (SMTC), & `Storage_Streams` (Album Cover)
- **Linux Bridge:** `zbus 6.5` (D-Bus `org.freedesktop.Notifications` & MPRIS2 `org.mpris.MediaPlayer2.*` dengan seleksi pemutar aktif, `Properties.Get("Position")` + `SetPosition` + tracking drift `evaluate_mpris_sync`, dan proxy reuse `CacheProperties::No`), Hyprland UNIX IPC socket (`.socket.sock` untuk `hl.layer_rule`/`eval`/fullscreen/cursor tracking), serta native Wayland `smithay-client-toolkit 0.19` (fitur `calloop` + `xkbcommon` untuk `KeyboardHandler` Left/Right) + `calloop 0.13` + `wl_shm` (`SlotPool`) di `src/platform/wayland_layer.rs` (dengan pemulihan otomatis `LayerSurface` saat compositor menutup surface); fallback `daemon.rs` + `eframe` untuk X11/non-layer-shell. Metrik System berasal dari `/proc` dan sysfs via `src/sysmon.rs` + `src/battery.rs` (tanpa crate baru).
- **System Tray, Config & Integrations:** `tray-icon 0.27` (fitur `ksni` murni D-Bus StatusNotifierItem tanpa dependensi GTK di Linux), `serde` + `serde_json` + `toml 1.2`, `directories 6.0`, `ureq 3.5` (LRCLIB, Google Translate `dt=t`, Google Calendar `.ics`, & fetch HTTP `mpris:artUrl`), `kakasi 0.1` + `any_ascii 0.3` (romanisasi offline multi-aksara), `image 0.25` (`png`, `jpeg`, `webp`, `bmp`, `gif` untuk cover `file://`/`data:image/...;base64`/`http(s)` & ekstraksi warna dominan), `chrono 0.4` + `chrono-tz 0.10`

## Struktur Folder Kode
```text
liebert-dynamic-island/
├── Cargo.toml
├── LICENSE.md                   # Lisensi GPL-3.0-or-later
├── AGENTS.md                    # Panduan kolaborasi & aturan repo
├── .github/workflows/
│   └── release.yml              # GitHub Actions: AppImage + tar.gz, zip + NSIS installer, bundel AUR (NSIS memakai STAGING_DIR/OUT_FILE absolut)
├── assets/                      # .desktop & ikon 256x256 bersama (AppImage, tar.gz, AUR)
├── docs/                        # Dokumentasi Obsidian & PRD
├── installer/windows/
│   └── installer.nsi            # Installer NSIS Windows (per-user, Start Menu & uninstaller)
├── packaging/aur/
│   ├── PKGBUILD                 # Paket AUR: build dari tag v0.1.5 via cargo --locked
│   └── .SRCINFO                 # Metadata AUR yang dihasilkan makepkg --printsrcinfo
├── scripts/
│   ├── build-appimage.sh        # Packaging AppImage Linux (x86_64)
│   └── build-linux-release.sh   # Build binary + AppImage + tar.gz Linux
└── src/
    ├── main.rs                  # Entry point, CLI flags (--ui, --settings, --ipc-addr/--ipc-sock, --spike-notifications)
    ├── daemon.rs                # Headless background daemon & on-demand UI spawner (fallback X11 / --ui)
    ├── ipc.rs                   # Protokol IPC JSON-lines via TCP loopback (DaemonToUi / UiToDaemon)
    ├── config.rs                # Load/save config.toml (size, colors, presets, clock, idle mode, .ics URL)
    ├── state.rs                 # Island state machine (Idle, Compact, Expanded), queue, & auto-hide delay
    ├── clock.rs                 # Jam digital & formatter zona waktu IANA (chrono-tz)
    ├── lyrics.rs                # LRC / Enhanced LRC parser, karaoke progress, romaji & translate worker
    ├── todo.rs                  # TodoStore & persistensi todos.toml
    ├── pomodoro.rs              # PomodoroTimer (Focus/Short/Long Break & siklus 4 sesi)
    ├── calendar.rs              # RFC 5545 .ics parser & Google Calendar sync worker
    ├── sysmon.rs                # Sampler CPU/RAM/disk/net (Win32 + /proc) & worker System tab
    ├── battery.rs               # Baterai Win32 GetSystemPowerStatus / sysfs power_supply
    ├── tray.rs                  # System tray menu & lifecycle control
    ├── ui/
    │   ├── mod.rs               # Overlay pill (Idle/Compact/Expanded tabs), spring physics, unified AA notch mesh, & UI loop
    │   ├── soft_render.rs       # Pure-Rust CPU egui ClippedPrimitive rasterizer (premultiplied BGRA/RGBA)
    │   ├── icons.rs             # Crisp Painter-drawn vector icons & responsive icon buttons
    │   └── settings.rs          # Settings tabs + standalone --settings runner (Unix socket / TCP)
    └── platform/
        ├── mod.rs               # Facade lintas OS (overlay, topmost, hover, font, MediaTrack, PlatformEvent)
        ├── windows.rs           # Win32 hover/no-activate/DWM, fullscreen watcher, WinRT Notifications & SMTC
        ├── windows_overlay.rs   # Native Win32 UpdateLayeredWindow + DIB SoftCanvas overlay runner
        ├── linux.rs             # D-Bus Notifications, MPRIS2 cached proxy, Hyprland IPC, & fontconfig lookup
        └── wayland_layer.rs     # Native Wayland wlr-layer-shell wl_shm + SoftCanvas overlay & recovery loop
```

### Distribusi & Rilis
- **`scripts/build-linux-release.sh`:** `cargo build --release --locked` → `build-appimage.sh` → staging `liebert-dynamic-island-linux-x86_64/` (binary, `LICENSE.md`, `README.md`, `.desktop`, ikon) → `dist/*.tar.gz` dan `dist/*.AppImage`.
- **`installer/windows/installer.nsi`:** Instalasi NSIS per-user di `%LOCALAPPDATA%\Programs\Liebert Dynamic Island`, shortcut Start Menu, entri Uninstall di registry HKCU, dan uninstaller. Dibangun di job Windows GitHub Actions dengan `makensis` dari direktori staging.
- **`packaging/aur/`:** `PKGBUILD` meng-clone tag `v$pkgver`, membangun dengan `cargo build --release --locked`, lalu memasang binary ke `/usr/bin`, lisensi, `.desktop`, dan ikon. `.SRCINFO` digenerate via `makepkg --printsrcinfo`. Publikasi ke AUR masih manual (butuh akun maintainer); bundel keduanya ikut sebagai aset release.
- **`.github/workflows/release.yml`:** Tag `v*` memicu build Linux + Windows, lalu `softprops/action-gh-release` mengunggah AppImage, tar.gz, zip, installer `.exe`, dan bundel AUR.

## Skema Data & State Machine
### Notification Payload
```rust
pub struct NotificationItem {
    pub id: String,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub icon_rgba: Option<Vec<u8>>,
    pub timestamp: u64,
}
```

### State Machine Overlay
- **`Idle`:** Pill compact di top-center atau menyatu seamless dengan tepi atas monitor (`y = 0`) lewat transisi `notch_morph` (`Top Notch` ↔ `Floating Island`), menampilkan mini cover + lirik karaoke **Bold** (`Original` atau `Romaji`/`Translate` sesuai `karaoke_lyric_mode`, disinkronkan via `duration` LRCLIB, `end_ms` jeda instrumental, & `lyric_offset_ms`) + 3-bar equalizer saat musik diputar (dengan opsi gradasi `#000` ke warna dominan album di paling bawah via `compact_album_gradient`), jam zona waktu dengan lebar pill adaptif otomatis yang selalu rata tengah simetris, atau jadwal terdekat; otomatis tersembunyi saat `auto_hide` atau `hide_in_fullscreen` aktif.
- **`Compact`:** Pill melebar menampilkan avatar berwarna brand aplikasi (`WhatsApp`, `Discord`, `Gmail`, browser, dll.), badge nama aplikasi + judul yang sejajar sesuai `TextAlign`, preview singkat, serta indikator jumlah antrean (turut mendukung gradasi `#000` → warna album bila musik aktif & `compact_album_gradient` menyala); timer auto-collapse default 5 detik.
- **`Expanded`:** Aktif saat hover pada `Compact` (menjeda timer). Strip tab atas sudah dihapus; header hanya memuat label tab aktif, indikator `◀ ▶`, tombol cepat ganti mode `Island`/`Notch`, dan tombol `⚙` yang membawa jendela Settings ke depan layar via `bring_window_to_front`. Navigasi Left/Right berjalan di `eframe`, hotkey Win32 `WM_HOTKEY`, dan `KeyboardHandler` Wayland, berpindah melalui modul yang aktif (`Home`, `Music`, `Todo`, `Calendar`, `Pomodoro`, `System`) secara wrap-around. Tab `Music` memakai layout lyrics-first: strip now-playing + playbar + transport di atas, lalu lirik full-width (`ScrollArea` + auto-scroll ke tengah pada baris aktif ber-font `LyricsBold`, dengan blur baris sebelum/sesudah). Settings terpisah memakai grup kiri (`Display`, `Lyrics`, `Features`, `System`, `Tasks`, `Calendar`) dan konten kanan scrollable. Juga menghormati opsi `show_notifications`.

### Preferensi Teks & Font Bold Lirik
- **`TextAlign` (`src/config.rs`) & `LyricsBold` (`src/ui/mod.rs`, `src/platform/`):** enum `Left` (default) / `Center` / `Right`, disimpan sebagai `text_align` di `config.toml` dan dipilih lewat tombol di tab Appearance Settings. `LayoutJob::halign` + `galley_draw_pos` + `aligned_galley_x` memastikan teks satu baris maupun multi-baris ter-wrap rata presisi di pill maupun expanded (notifikasi, Home, Todo, Calendar, Music). Keluarga font `LyricsBold` memuat font Bold sistem Linux/Windows (+ sibling bold dari font pilihan & fallback CJK `All`) agar lirik utama selalu tebal tanpa mengandalkan `RichText::strong()`.
- **Bounded caches:** `IslandState.queue` maksimal `MAX_QUEUED_NOTIFICATIONS = 32`, worker lirik memakai `BoundedCache` (raw 32, enriched 64) tanpa `duration_secs` pada kunci, dan `alerted_events` dipangkas ke `ALERT_RETENTION_SECS = 86_400`. Gradient texture cache dikuantisasi per 8 kanal warna agar animasi spring tidak meng-upload ulang tekstur tiap frame. `SoftCanvas` memindahkan piksel `TexturesDelta` (`Arc::get_mut`/`mem::take`) alih-alih meng-clone, dan decode album art dibatasi `1024 px` / `8 MiB` lalu di-resize ke `128×128`.
- **Daur hidup overlay:** `IslandApp::ui` membatalkan `close_requested()` root viewport; hanya `TrayAction::Exit` yang menutup aplikasi. Di Linux, auto-hide mengecilkan viewport ke `1×1` (bukan `Visible(false)`), dan layer-shell yang ditutup compositor mengembalikan `Err` agar `main.rs` menjalankan fallback eframe.

## Alur Komponen Utama
1. **Startup:** `config.rs` membaca `config.toml` → `tray.rs` membuat ikon tray → `platform` memunculkan listener notifikasi di background thread/async task → `ui` membuka overlay window dalam state `Idle`.
2. **Notifikasi Masuk:** OS memicu event (`UserNotificationListener` di Windows / D-Bus di Linux) → `platform` memetakan payload ke `NotificationItem` → kirim via channel ke `state.rs` (antrean sederhana) → `ui` beranimasi ke `Compact`.
3. **Interaksi & Dismiss:** Hover menjeda timer dan masuk ke `Expanded`; klik `Dismiss` (atau timer 5 detik habis) menutup item aktif dan menampilkan item berikutnya di queue atau kembali ke `Idle`.

## Target Non-Fungsional
- **Startup Time:** `< 500 ms` hingga pill tampil.
- **Notification Latency:** `< 150 ms` dari event OS hingga frame animasi masuk.
- **Resource Footprint:** `< 25 MB` RAM idle (`< 50 MB` saat expanded), `< 0.5%` CPU idle (event-driven, tanpa busy-polling).
- **Rendering & DPI:** `>= 60 FPS` saat animasi transisi; Per-Monitor DPI Aware v2 (Windows) & fractional scaling (Linux).
- **Privasi & Jaringan:** Data notifikasi OS diproses 100% lokal tanpa telemetri maupun log persisten. Permintaan jaringan keluar bersifat opsional dan terbatas pada metadata lagu (`artist`/`title`) ke `lrclib.net` (saat `show_lyrics` aktif) serta fetch URL `.ics` Google Calendar yang dikonfigurasi pengguna.
