---
type: decision-log
status: active
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - decisions
---

# 03 - Catatan Rapat & Keputusan

Log *Architecture Decision Records (ADR)* untuk proyek [[00 - Dashboard Proyek]].

---

## ADR-001: Target Stack Native Rust GUI (Tanpa Electron/WebView)
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Aplikasi utility overlay selalu berjalan di background. Electron/WebView memakan 150–400 MB RAM dan lambat saat startup.
- **Keputusan:** Menggunakan Rust dengan native/lightweight GUI (`egui`/`slint`/`iced` atau native hooks) tanpa WebView.
- **Konsekuensi:** Footprint RAM < 25 MB dan startup instan, namun styling animasi harus ditulis di Rust dan perlu penanganan khusus per-OS untuk windowing overlay.

## ADR-002: Scope MVP Fokus pada System Notifications (Windows & Linux)
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Timeline MVP adalah 1–2 minggu oleh 1 developer. Memasukkan Media Control (SMTC/MPRIS2) dan System HUD sekaligus akan memperbesar risiko keterlambatan.
- **Keputusan:** P0 dibatasi pada Floating Pill Overlay, System Notification Listener (Windows WinRT + Linux D-Bus), interaksi dasar (hover/expand/dismiss), dan System Tray + Config minimal. Media Control & System HUD dipindah ke P1.
- **Konsekuensi:** Arsitektur Platform Bridge harus modular agar fitur P1 bisa ditambahkan tanpa refactor besar.

## ADR-003: Hasil Spike Fase 1 — Renderer `eframe 0.37` (Glow) & WinRT Unpackaged
- **Tanggal:** 2026-11-25
- **Status:** Partially Accepted *(Renderer, Windows, dan build/spike D-Bus Linux diterima; verifikasi overlay Wayland/X11 masih pending)*
- **Konteks:** [[PRD-LiebertDynamicIsland-MVP]] dan [[01 - Arsitektur & Spesifikasi]] memerlukan keputusan pada 3 spike Fase 1 sebelum mengunci implementasi penuh:
  1. **Renderer Rust:** `eframe 0.37` dengan `Renderer::Glow` dipilih untuk MVP karena mendukung transparent borderless viewport, `WindowLevel::AlwaysOnTop`, event-driven `request_repaint_after`, serta waktu build dan kompleksitas minimal dalam target 1–2 minggu.
  2. **Windows WinRT `UserNotificationListener`:** Hasil eksekusi `cargo run -- --spike-notifications` di Windows host menunjukkan `SPIKE OK: Access: Allowed | Active toasts: 21` langsung dari `.exe` unpackaged (tanpa Sparse Package/MSIX).
  3. **Linux D-Bus & Wayland Positioning:** `src/platform/linux.rs` kini lolos build release dengan `zbus 6.5` (API builder `session()/name()/serve_at()` mengembalikan `Self`, error baru muncul di `build().await`; `OwnedVariant` menggantikan `OwnedValue`). `cargo run --release -- --spike-notifications` melaporkan sesi D-Bus OK, namun `org.freedesktop.Notifications` saat ini dipegang daemon lain, sehingga klaim nama/listener runtime belum terverifikasi. Positioning `wlr-layer-shell` pada Wayland belum didukung oleh `eframe`/`winit` dan dicatat sebagai limitasi sementara.
- **Keputusan:** Mengunci `eframe 0.37` (`Glow`) dan jalur WinRT unpackaged untuk Windows; build & spike D-Bus Linux divalidasi di host Linux, uji runtime/overlay menyusul.
- **Konsekuensi:** MVP Windows dapat langsung diuji harian dan binary Linux (`target/release/liebert-dynamic-island`) dapat dibangun. Untuk uji notifikasi Linux, daemon incumbent (mis. dunst/mako) harus dihentikan agar aplikasi dapat mengklaim `org.freedesktop.Notifications`. Untuk target Wayland native positioning di masa depan, layer windowing Linux perlu ditingkatkan ke `smithay-client-toolkit` atau `iced_layershell` jika posisi top-center `winit` tidak memadai pada compositor tertentu.

## ADR-004: Arsitektur Lyrics, Settings Viewport, Todo, Google Calendar `.ics`, & Perbaikan Hover
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna meminta tambahan 4 fitur (lirik musik seperti Spotify, menu settings ukuran/warna/tampilan, todo list, dan Google Calendar) serta perbaikan bug saat hover Dynamic Island tidak membesar.
- **Keputusan:**
  1. **Hover Fix:** `IslandState::set_hovered` diubah agar `Idle` selalu bertransisi ke `Expanded` saat hover (sebelumnya hanya bekerja bila `active.is_some()`), dan di Windows posisi kursor dicek langsung via Win32 `GetCursorPos` + `GetWindowRect` (dengan hysteresis padding) karena window `WS_EX_NOACTIVATE` sering tidak menerima event hover pointer standar.
  2. **Settings Window Terpisah (`show_viewport_immediate`):** Karena pill utama memakai `WS_EX_NOACTIVATE` (tidak bisa menerima focus keyboard), input teks untuk Todo & Google Calendar serta slider/color picker ditempatkan di window sekunder normal `eframe` yang bisa difokuskan.
  3. **Music Lyrics (SMTC/MPRIS2 + LRCLIB):** Metadata & timeline dibaca dari OS (`Media_Control` SMTC di Windows, MPRIS2 D-Bus di Linux) dan lirik LRC diambil dari API publik `lrclib.net` dengan cache di memori — tanpa login Spotify OAuth.
  4. **Google Calendar via Secret `.ics` URL:** Menggunakan feed iCal `.ics` resmi Google Calendar + parser RFC 5545 lokal (`src/calendar.rs`) agar tidak memerlukan OAuth client secret maupun local redirect server.
- **Konsekuensi:** Menambah dependensi ringan `ureq 3.5`, `serde_json`, `chrono`, dan fitur `Media_Control`, namun seluruh interaksi overlay tetap non-focus-stealing dan bebas login OAuth eksternal.

## ADR-005: Jam IANA (`chrono-tz`), Idle Display Mode, & Top-Edge Hover Reveal saat Auto-Hide
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna membutuhkan jam dengan pilihan zona waktu, opsi memilih konten apa yang tampil saat idle, serta auto-hide setelah jeda *N* detik yang tetap bisa dimunculkan lagi dengan mengarahkan kursor ke tepi atas layar.
- **Keputusan:**
  1. **Zona Waktu (`chrono-tz`):** Menggunakan `chrono-tz 0.10` di `src/clock.rs` untuk mendukung preset umum sekaligus input IANA timezone bebas (dengan fallback aman ke `Local`).
  2. **Idle Display Mode:** Menambahkan enum `IdleDisplayMode` (`Auto`, `Clock`, `Lyrics`, `NextEvent`, `Minimal`) pada `Config`.
  3. **Delayed Auto-Hide & Top-Edge Reveal:** `IslandState` melacak `idle_since` dan `auto_hide_delay` untuk menunda penyembunyian selama *N* detik. Saat window disembunyikan (`ViewportCommand::Visible(false)`), `eframe::App::logic` tetap dibangunkan tiap `80 ms` dan memanggil Win32 `GetCursorPos` (`is_cursor_in_top_trigger`) untuk mendeteksi kursor di zona tepi atas tengah layar dan memunculkan notch kembali.
- **Konsekuensi:** Fitur berjalan mulus di Windows dengan CPU tetap rendah, namun top-edge reveal saat window tersembunyi belum memiliki implementasi global pointer hook di Linux (`is_cursor_in_top_trigger` mengembalikan `None`).

## ADR-006: Karaoke Clip-Rect Painter, Translate+Romaji (`dt=t&dt=rm`), & Lazy CJK Font Loading
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna menambahkan kebutuhan karaoke lyric, translate lyric, romaji untuk aksara non-abjad (Jepang, Cina, Korea, Arab, dll.), dan playbar penuh (seek, shuffle, prev, play, next, repeat) tanpa mengorbankan target RAM idle `< 25 MB`.
- **Keputusan:**
  1. **Karaoke Highlight:** `src/lyrics.rs` mendukung Enhanced LRC (`<mm:ss.xx>`) maupun interpolasi linear per baris, lalu `src/ui/mod.rs` merender dua galley dengan `Painter::with_clip_rect` sehingga sapuan warna berjalan mulus di `~30 FPS` (`33 ms`) saat playback aktif.
  2. **Translate + Romaji Satu Request:** Worker `src/lyrics.rs` menggabungkan 20 baris per batch ke `translate.googleapis.com/translate_a/single` (`dt=t&dt=rm`) dengan cache dua tingkat (`raw_cache` & `enriched_cache`) serta fallback `kana_to_romaji` offline.
  3. **Lazy Font Fallback:** Font ringan (`segoeui.ttf`, `seguisym.ttf`) dimuat saat startup, sedangkan font CJK/Hangul (`msgothic.ttc`, `malgun.ttf`) baru dimuat dari sistem ketika teks CJK/Hangul terdeteksi.
- **Konsekuensi:** RAM saat idle tanpa lagu CJK tetap rendah, namun fitur Shuffle/Repeat/Seek bergantung pada dukungan SMTC/MPRIS2 masing-masing media player.

## ADR-007: Redesign Music Card 2-Kolom (Referensi Visual), Album Art Gradient Wash, & `LayoutJob` Karaoke
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna mengunggah desain referensi (dark charcoal card dengan bottom wash coral/merah dari cover album, kolom kiri cover+kontrol, kolom kanan multi-line karaoke original+romaji+translate + upcoming redup, dan 2 tombol pill di bawah).
- **Keputusan:**
  1. **Album Art & Dominant Accent:** WinRT `Storage_Streams::DataReader` (Windows) dan `mpris:artUrl` (Linux) membaca cover album sekali per perubahan lagu, lalu `image 0.25` mengecilkan ke maks `160×160` dan `extract_dominant_accent` menghitung warna aksen dominan dinamis.
  2. **Gradient Wash:** `ensure_gradient_texture` menghasilkan tekstur `1×64` yang di-cache dan dirender lewat `RectShape::filled(...).with_texture(...)` dengan corner radius card.
  3. **Wrapped Multi-Line Karaoke:** `render_wrapped_karaoke` menggunakan `egui::text::LayoutJob` dengan segment warna lit/dim per karakter agar karaoke tetap rapi meski baris panjang mengalami word-wrap.
- **Konsekuensi:** Ukuran expanded card saat di tab Musik melebar ke `436×308` (dikalikan `scale`), dan pengguna tetap bisa menonaktifkan `dynamic_album_color` atau memilih preset `"Crimson"` di Settings.

## ADR-008: Notch Dock Mode (`egui::Mesh` Concave Ears) & Romanisasi Offline (`kakasi` + `any_ascii`)
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna menginginkan bentuk notch yang menyatu dengan bezel atas layar (seperti MacBook Notch, tersembunyi saat fullscreen), memindahkan seluruh toggle Karaoke/Romaji/Translate ke popup Settings saja, serta memperbaiki Romaji yang gagal/tidak akurat saat bergantung pada API Google `dt=rm` (sering terkena rate-limit/HTML block atau gagal di Kanji).
- **Keputusan:**
  1. **Notch Dock Mode:** Menambahkan `notch_dock_mode` (`y = 0`, sudut atas rata, telinga cekung kiri/kanan via `egui::Mesh`, mini album cover + lirik + 3-bar equalizer saat idle) sambil tetap menghormati `hide_in_fullscreen`.
  2. **Romanisasi Offline:** Mengganti `dt=rm` + tabel manual dengan `kakasi 0.1` (mendukung Kanji+Kana Jepang, diaktifkan per-lagu bila lagu mengandung Kana) dan `any_ascii 0.3` (untuk Cina, Korea, Arab, Cyrillic, dll.) sehingga Romaji 100% offline, deterministik, dan selaras baris demi baris.
  3. **Settings-Only Lyric Toggles:** Menghapus tombol inline `KR`, `Translate`, dan `Romaji` dari pulau utama agar layout Music Card lebih bersih dan fokus.
- **Konsekuensi:** Fitur Romaji tidak lagi bergantung pada jaringan/Google `dt=rm`; perlu diperhatikan bahwa crate `kakasi 0.1` berlisensi **GPL-3.0**.

## ADR-009: Seamless Windows 11 DWM Notch, Polling Fallback WinRT 500ms, Romaji Karaoke, & Morph `Top Notch` ↔ `Floating Island`
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pada Windows 11, DWM tetap memberi frame border tipis dan corner rounding pada window transparan `winit` sehingga muncul kotak abu-abu dan celah `1–2 px` di tepi atas monitor. Selain itu, callback `UserNotificationListener::NotificationChanged` tidak selalu terpanggil pada `.exe` unpackaged, pengguna juga meminta Romaji bisa dipilih sebagai teks karaoke utama, dan ingin transisi halus saat berpindah antara mode `Notch` dan `Floating Island`.
- **Keputusan:**
  1. **Seamless Windows 11 Top Edge:** Menambahkan fitur `Win32_Graphics_Dwm` dan menyetel `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND`, `DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE`, serta `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED` di `apply_no_activate_hook`, membersihkan `WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME` dari `GWL_STYLE`, serta memanggil `pin_window_top_y(WINDOW_TITLE, 0)` ketika mode Notch aktif penuh.
  2. **Reliable Toast Popups (`500 ms` Fallback Polling):** `spawn_listener` di `src/platform/windows.rs` menggunakan `sig_rx.recv_timeout(Duration::from_millis(500))` dan fallback `visual.Bindings()` (dan hanya memanggil `on_wake()` saat ada perubahan) agar toast WhatsApp, Discord, browser, Gmail, dll. tetap muncul meski `NotificationChanged` gagal memicu callback.
  3. **Romaji Karaoke & Morphing Transition:** Menambahkan `KaraokeLyricMode` (`Original` / `Romaji`) pada `Config` serta state `notch_morph: f32` di `IslandApp` untuk menginterpolasi `y`, lebar telinga cekung, corner radius atas/bawah, dan border opacity secara kontinu saat berganti antara `Top Notch` dan `Floating Island`.
- **Konsekuensi:** Overlay menyatu rata dengan tepi atas Windows 11 tanpa outline DWM, notifikasi desktop masuk dalam `<= 500 ms` saat Idle CPU tetap rendah, dan perpindahan mode tampilan berlangsung mulus baik dari header island, Settings, maupun System Tray.

## ADR-010: Perbaikan DWM Glitch saat Hover, SMTC `LastUpdatedTime` Sync, Dual-Galley Karaoke, & Foreground Settings
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pasca rilis fitur morphing notch & Romaji karaoke, ditemukan 4 regresi/bug: (1) tombol chrome klasik Win32 (minimize/maximize/close) berkedip saat hover/resize, (2) beberapa elemen UI saling menumpuk pada mode compact/expanded/music, (3) lirik & karaoke Romaji tidak sinkron atau melompat saat word-wrap, dan (4) tombol Settings sulit diklik / jendela Settings gagal naik ke foreground karena root window memakai `WS_EX_NOACTIVATE`.
- **Keputusan:**
  1. **Hover Chrome Fix:** Menghapus `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED` (yang merusak komposisi transparan DWM dan memunculkan non-client buttons Win32 saat resize), menggantinya dengan pembersihan `WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX` + `.with_*_button(false)` sambil tetap mempertahankan `DWMWCP_DONOTROUND` & `DWMWA_COLOR_NONE`.
  2. **Akurasi Lyric & Romaji Sync:** Mengekstrapolasi `Position` SMTC Windows menggunakan `LastUpdatedTime` (`UniversalTime`) agar posisi awal tidak usang, mencocokkan `duration` di LRCLIB, mempertahankan baris kosong LRC sebagai batas `end_ms` jeda instrumental, menyediakan slider `lyric_offset_ms` (`-5000..=5000`), serta mengganti `render_wrapped_karaoke` ke dual-galley per-baris agar highlight Romaji halus tanpa membelah kata.
  3. **Layout Anti-Overlap & Akses Settings:** Membagi header `render_expanded` menjadi slot kiri/kanan terpisah ber-clip, menjaga panel expanded hanya dirender saat `height >= 95 * scale`, membungkus kolom lirik kanan dengan `ScrollArea::vertical()`, dan memanggil `bring_window_to_front` (`HWND_TOPMOST` -> `HWND_NOTOPMOST` + `SetForegroundWindow`) sambil menjaga root viewport tetap visible selama Settings terbuka.
- **Konsekuensi:** Tidak ada lagi glitch tombol caption Win32 atau tabrakan widget UI, lirik Romaji tersinkron halus dengan dukungan kalibrasi offset pengguna, dan jendela Settings selalu langsung muncul ke depan ketika tombol `⚙` diklik.

## ADR-011: Fullscreen Notification Pass-Through, Pergantian Mode Khusus Settings, & Presisi Alignment UI
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pengguna meminta agar saat aplikasi fullscreen aktif Dynamic Island tetap sembunyi namun tetap muncul jika ada notifikasi masuk, pergantian mode `Notch` ↔ `Floating Island` dibatasi hanya melalui Settings (bukan di island/tray), serta seluruh ikon dan elemen UI dirapikan perataannya.
- **Keputusan:**
  1. **Fullscreen Notification Pass-Through:** `IslandState::should_be_visible` hanya menyembunyikan overlay saat `hide_in_fullscreen && fullscreen_active && active.is_none()`, dan `logic()` memanggil `raise_overlay_topmost` (`HWND_TOPMOST`) tiap kali overlay menjadi visible sehingga popup notifikasi tetap muncul di atas aplikasi borderless fullscreen (dengan toleransi `2 px` di `is_foreground_fullscreen`), lalu kembali tersembunyi setelah notifikasi selesai/di-dismiss.
  2. **Settings-Only Display Mode Switch:** Menghapus `TrayAction::ToggleDockMode` dan tombol `Island`/`Notch` dari header expanded island; pengguna mengubah mode secara eksklusif dari Settings (`Display Mode`) dengan animasi `notch_morph`.
  3. **Alignment Konsisten:** Menambahkan `render_icon_button` (`Align2::CENTER_CENTER`), memusatkan idle Clock & Minimal label memakai `LayoutJob`, meratakan vertical galley pada `render_karaoke_line`, menyelaraskan slot ikon overview (`render_overview_summary_row`), playbar (`render_playbar`), kontrol transport, serta baris Todo dan Calendar.
- **Konsekuensi:** Overlay tidak mengganggu saat menonton video/game fullscreen kecuali saat notifikasi penting masuk, antarmuka header expanded lebih bersih, dan seluruh elemen/ikon rata secara konsisten lintas state.

## ADR-012: Subclass `WM_NCCALCSIZE`, Envelope-Canvas Smooth Animation, & Fullscreen Watcher Thread
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Masih terlihat garis hitam tipis `1 px` di atas Dynamic Island, opsi ganti mode di Settings kurang terlihat, animasi hover/perpindahan terasa tersendat karena `InnerSize` + `OuterPosition` dipanggil setiap frame animasi, dan deteksi fullscreen gagal membangunkan window saat overlay sedang tersembunyi (`Visible(false)`).
- **Keputusan:**
  1. **Garis Tipis `1 px` (`winit` Hack):** `winit` menambah `params.rgrc[0].top += 1` pada `WM_NCCALCSIZE` untuk `MARKER_UNDECORATED_SHADOW`. `apply_no_activate_hook` kini memasang Win32 subclass `overlay_wndproc` yang mengembalikan `LRESULT(0)` untuk `WM_NCCALCSIZE` (`wparam != 0`), menghilangkan garis tersebut sepenuhnya.
  2. **Envelope Canvas Smoothness:** Selama `animating || morphing`, ukuran window OS dikunci ke envelope pembungkus (`env_w`, `env_h`, `window_top_y = 0`) tanpa memanggil `InnerSize`/`OuterPosition` tiap frame; pill & isi UI beranimasi penuh `>= 60 FPS` di dalam canvas transparan menggunakan `is_cursor_in_screen_rect` dan `multiply_opacity`, lalu mengecil kembali ke ukuran pas setelah diam.
  3. **Display Mode & Reliable Fullscreen Watcher:** Menempatkan tombol `Top Notch (Seamless Top)` vs `Floating Dynamic Island` paling atas di Settings Appearance; memindahkan deteksi fullscreen ke thread background `spawn_fullscreen_watcher` (`SHQueryUserNotificationState` + `GetAncestor(GA_ROOT)` + check taskbar/caption) dengan `on_wake()` agar `Visible(false <-> true)` selalu terpanggil meski window sedang tersembunyi.
- **Konsekuensi:** Garis tipis di atas island hilang total, transisi hover & pindah mode berlangsung mulus tanpa jitter resize Win32, opsi mode langsung terlihat di Settings, dan auto-hide/restore saat fullscreen selalu sinkron.

## ADR-013: Kanvas Permanen + Click-Through Win32 & Direct `ShowWindow` Anti-Style-Wipe
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Meskipun memakai envelope canvas selama animasi, memanggil `ViewportCommand::InnerSize`/`OuterPosition` pada frame awal dan akhir buka-tutup tetap menimbulkan lompatan posisi `1–2` frame karena `winit` menerapkan perintah viewport secara asinkron setelah `ui()` selesai. Selain itu, `ViewportCommand::Visible(bool)` di `winit 0.30` memanggil `SetWindowLongW` dari state internalnya (menghapus `WS_EX_NOACTIVATE` & `WS_POPUP`) serta `ShowWindow(SW_SHOW)` yang mencuri fokus foreground dari aplikasi fullscreen sehingga `is_foreground_fullscreen()` gagal. Jendela maximized dengan auto-hide taskbar Windows 11 (`work.bottom` selisih `2 px`) juga sempat salah terdeteksi sebagai fullscreen.
- **Keputusan:**
  1. **Permanent Fixed Canvas + Dynamic Click-Through (Windows):** Ukuran window root dikunci ke `permanent_canvas_size(config)` pada `y = 0.0` sepanjang waktu tanpa memanggil `InnerSize`/`OuterPosition` saat buka-tutup atau morph; area di luar pill dibuat click-through via `set_overlay_click_through` (`WS_EX_LAYERED | WS_EX_TRANSPARENT`).
  2. **Direct Win32 Visibility:** Mengganti `ViewportCommand::Visible` di Windows dengan `set_overlay_visible` yang memanggil `ShowWindow(hwnd, SW_HIDE / SW_SHOWNOACTIVATE)` langsung tanpa menyentuh `winit` `apply_diff`.
  3. **Accurate Fullscreen Bounds:** Menggunakan `DWMWA_EXTENDED_FRAME_BOUNDS` dan ambang batas taskbar `>= 20 px` (`is_rect_covering_monitor_as_fullscreen`) agar jendela maximized biasa tidak memicu auto-hide.
- **Konsekuensi:** Flicker buka-tutup/morph hilang total di Windows, style `WS_EX_NOACTIVATE` dan fokus aplikasi fullscreen tetap terjaga, dan seluruh area di luar pill tetap bisa diklik menembus overlay.

## ADR-013b: Native Wayland `wlr-layer-shell`, Pill Translation, & Spring Physics
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pada Linux Wayland, window reguler `eframe`/`winit` terikat workspace sehingga tidak dapat melayang global di atas seluruh workspace dan layer seperti Quickshell. Selain itu, pengguna menginginkan baris terjemahan ringkas di idle/compact pill dan animasi pegas harmonik ala Dynamic Island macOS.
- **Keputusan:**
  1. **Native Wayland `wlr-layer-shell` (`src/platform/wayland_layer.rs`):** Di Linux Wayland, overlay utama dijalankan langsung di atas `zwlr_layer_surface_v1` (`Layer::Overlay`, `Anchor::TOP`, `exclusive_zone = -1`, `KeyboardInteractivity::None`) menggunakan `smithay-client-toolkit 0.19` + `glutin 0.32` EGL + `egui_glow 0.37` + `calloop 0.13`, ditambah `hl.layer_rule` Hyprland (`order = -1`, `above_lock = false`, `no_anim = true`, `blur = false`). Dengan ini overlay terpisah penuh dari seluruh workspace, tetap tepat di atas Quickshell namun di bawah lockscreen dan layer overlay ber-order lebih tinggi, serta memakai region input kosong (`set_input_region`) saat tersembunyi. Mode `--settings --ipc-sock` menangani jendela Settings interaktif secara terpisah lewat `eframe` dengan sinkronisasi live via UNIX socket. Jalur `eframe` tetap dipertahankan sebagai fallback otomatis untuk X11/compositor non-`wlr-layer-shell` dan Windows.
  2. **Pill Lyric Translation (`show_pill_translation`):** Menambahkan `Config::show_pill_translation` (default `true`) beserta checkbox di Settings (`Music & Synced Lyrics`), serta memperluas `IdleDisplayMode::Lyrics` agar merender baris terjemahan di bawah lirik utama (`44 px * scale` saat terjemahan tersedia).
  3. **Animasi Spring Bouncy ala macOS (`step_spring`):** Mengganti animasi eksponensial di `src/ui/mod.rs` dengan fisika pegas harmonik teredam 2nd-order ber-substep (`omega_x`/`omega_y` & `zeta` berbeda untuk expand vs collapse serta pop-in saat `became_visible`), dilengkapi animasi spring `notch_morph` dan pacing `6 ms` pada runner Wayland.
- **Konsekuensi:** Di Wayland (`Hyprland`/`Sway`/`River`/`Niri`), Dynamic Island tidak pernah lagi masuk atau berpindah ke workspace aplikasi biasa dan posisinya stabil di top-center di atas Quickshell.

## ADR-015: Lisensi GPL-3.0 (`LICENSE.md`), Transisi Lirik Fade-Up/Down, & Release Workflow
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Proyek memerlukan berkas `LICENSE.md` yang kompatibel dengan dependensi `kakasi 0.1` (`GPL-3.0`), animasi pergantian baris lirik yang lebih halus di pill mode (`IdleDisplayMode::Lyrics`), serta otomatisasi rilis lintas OS Windows & Linux.
- **Keputusan:** Menetapkan `GPL-3.0-or-later` pada `LICENSE.md` & `Cargo.toml`, menambahkan `PillLyricTransition` (`240 ms` fade-up/down terklip berdasarkan `ActiveKaraoke::line_index`) di `src/ui/mod.rs`, dan menambahkan `.github/workflows/release.yml` untuk membangun rilis Windows (`.zip`) dan Linux (`AppImage` + `.tar.gz`).
- **Konsekuensi:** Seluruh distribusi biner memiliki lisensi yang sah, transisi lirik di pill terasa mulus di `>= 60 FPS`, dan rilis Windows + Linux otomatis tersedia setiap tag `v*` didorong ke GitHub.

## ADR-014: Pomodoro Timer, Font Perangkat, Karaoke Translate, & Windowed-Fullscreen
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pengguna membutuhkan dukungan sembunyi otomatis untuk aplikasi **windowed-fullscreen** (yang sering memiliki `WS_CAPTION` meski menutupi layar penuh), kepatuhan terhadap `auto_hide_delay_secs` saat keluar fullscreen, opsi **Translate** sebagai teks karaoke utama, pemilih font dari font terinstal di perangkat, dan fitur **Pomodoro**.
- **Keputusan:**
  1. **Windowed-Fullscreen & Delay:** `is_rect_covering_monitor_as_fullscreen` mendeteksi window ber-`WS_CAPTION` yang menutupi penuh tepi bawah monitor (`win_rect.bottom >= mon.bottom`) saat auto-hide taskbar aktif; `set_fullscreen_active` selalu me-reset `idle_since` saat status fullscreen berubah.
  2. **Device Font Picker:** `collect_fonts_from_dirs` memindai direktori font OS (`.ttf`/`.otf`/`.ttc`) tanpa dependensi tambahan dan `apply_configured_fonts` menyisipkan font pilihan pada prioritas tertinggi `FontFamily::Proportional`.
  3. **Karaoke Translate & Pomodoro:** Menambah `KaraokeLyricMode::Translate` dan modul OS-agnostic `src/pomodoro.rs` (`Focus`/`ShortBreak`/`LongBreak`, notifikasi selesai fase, & countdown idle).
- **Konsekuensi:** Seluruh preferensi tersimpan lokal di `config.toml`, tetap kompatibel dengan `#[serde(default)]`.

## ADR-016: Text Alignment, Perlindungan Close Linux, & Batas Memori
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pengguna meminta pengaturan perataan teks (kiri/kanan/tengah) lewat tombol di Settings, melaporkan aplikasi Linux seolah-olah selalu di-force-close, serta RAM yang terus naik seiring waktu.
- **Keputusan:**
  1. **Text Alignment:** `TextAlign` (`Left`/`Center`/`Right`, default `Left`) ditambahkan ke `Config` dengan tombol segmented di Settings Appearance. Renderer memakai `egui_align` untuk layout dan `aligned_galley_x` untuk galley/`Painter` manual (karaoke idle & kartu musik, judul/artis, dot clock) agar perilaku konsisten lintas backend.
  2. **Daur hidup Linux:** `IslandApp::ui` mengirim `ViewportCommand::CancelClose` saat WM meminta close; `TrayAction::Exit` adalah satu-satunya jalur keluar (Close di eframe, `want_exit` di layer-shell). Auto-hide/fullscreen pada eframe Linux memakai viewport `1×1`, bukan `Visible(false)`. Compositor yang mematikan layer surface mengubah `try_run_layer_shell` menjadi `Err` sehingga fallback eframe di `main.rs` benar-benar berjalan.
  3. **Stabilitas memori:** `textures_delta` selalu di-drain walau tersembunyi (mencegah kebocoran tekstur GL), loop Wayland menjumpai frame idle tanpa tessellation/GL, antrean notifikasi dibatasi 32, cache lirik memakai FIFO bounded, alert kalender dipangkas 1 hari, dan font hanya dipindai sekali.
- **Konsekuensi:** Preferensi alignment persist di `config.toml` tanpa merusak konfigurasi lama (`#[serde(default)]`); aplikasi tidak lagi berhenti karena close request WM/auto-hide; penggunaan RAM tidak bertumbuh tak terbatas pada sesi panjang. Terverifikasi dengan `cargo test` (41 tests) dan `cargo clippy --all-targets -- -D warnings`. Lihat [[04 - Changelog & Dev Log]].

## ADR-017: Kemasan Rilis v0.1.0 (AppImage, tar.gz, AUR, NSIS)
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pengguna meminta AppImage, rilis Linux `.tar.gz`, paket AUR, dan installer Windows, lalu mendorong semuanya ke `main` dan mempublikasikan rilis `v0.1.0`.
- **Keputusan:**
  1. **Aset bersama:** `.desktop` dan ikon PNG 256×256 disimpan di `assets/` dan dipakai ulang oleh AppImage, tar.gz, dan AUR; generator ikon inline dihapus dari `scripts/build-appimage.sh`.
  2. **Linux:** `scripts/build-linux-release.sh` menghasilkan AppImage + arsip tar.gz berisi binary, lisensi, README, `.desktop`, dan ikon.
  3. **AUR:** `packaging/aur/PKGBUILD` membangun dari tag git `v0.1.0` dengan `cargo --locked`; `sha256sums=('SKIP')` dipakai secara sah untuk sumber VCS dan `.SRCINFO` digenerate dengan `makepkg --printsrcinfo`.
  4. **Windows:** `installer/windows/installer.nsi` menghasilkan `setup.exe` NSIS per-user (shortcut Start Menu + uninstaller), dibangun di runner Windows GitHub Actions karena `makensis` tidak tersedia di mesin pengembangan Linux. Arsip `.zip` portable tetap dipertahankan.
  5. **Publikasi:** Workflow release membangun keempat aset plus bundel AUR pada tag `v*`; tag `v0.1.0` didorong setelah `main` agar GitHub Release terbit otomatis.
- **Konsekuensi:** Pengguna Windows mendapat wizard instalasi, pengguna Linux memilih AppImage/tar.gz/AUR, dan semua aset memiliki lisensi yang sama. Submit ke AUR tetap manual karena memerlukan akun maintainer; CI tidak dapat melakukannya. Lihat [[04 - Changelog & Dev Log]].

## ADR-018: Pure-Rust CPU Rasterizer (`SoftCanvas`), Wayland `wl_shm` Recovery, & Win32 `UpdateLayeredWindow`
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pasca merge PR #15 (`0c9daa4`), proses overlay `eframe`/`glutin`/`egui_glow` tetap menarik pustaka OpenGL/EGL/WGL (40–120 MB RSS dari driver Mesa/NVIDIA/WGL ICD), serta memicu hilangnya Dynamic Island di Hyprland sambil ikon tray tetap berjalan (akibat commit `set_buffer_scale` sebelum buffer serasi dan kegagalan `surface_closed` yang memicu fallback `xdg_toplevel` non-layer-shell).
- **Keputusan:**
  1. **Pure-Rust CPU Rasterizer (`src/ui/soft_render.rs`):** Merasterisasi `egui::ClippedPrimitive` langsung ke buffer premultiplied BGRA dengan top-left edge fill rule (mencegah double-blend diagonal) serta pelepasan delta tekstur secara langsung tanpa konteks OpenGL/EGL.
  2. **Wayland `wl_shm` + Auto-Recovery (`src/platform/wayland_layer.rs`):** Mengganti `glutin`/`glow`/`egui_glow` dengan `smithay-client-toolkit` `wl_shm::SlotPool` (`512 KB`), mewajibkan `set_buffer_scale` dan attach buffer dalam satu commit atomik, menambahkan `spawn_fullscreen_watcher` & `apply_configured_fonts`, dan merekonstruksi `LayerSurface` otomatis saat `ZwlrLayerSurfaceV1::Closed` / `new_output` terjadi setelah DPMS atau reload Hyprland.
  3. **Win32 `UpdateLayeredWindow` (`src/platform/windows_overlay.rs`) + `--settings` Terpisah (`src/ui/settings.rs`):** Overlay utama Windows dirender via DIB 32-bit BGRA + `UpdateLayeredWindow` (zero WGL/OpenGL driver DLL, native click-through alpha = 0); `eframe` hanya dimuat di proses child `--settings --ipc-addr` saat jendela Settings dibuka lalu dibebaskan saat ditutup.
  4. **Optimasi Font & MPRIS D-Bus (`src/platform/{linux,windows,mod}.rs`, `src/ui/mod.rs`):** Memangkas font eager Linux/Windows, memuat tepat satu index face TTC per `CjkScript`, melepaskan tekstur album/gradient saat media kosong & daftar font saat Settings tutup, serta memakai ulang proxy `zbus` `CacheProperties::No`.
- **Konsekuensi:** Target RAM `< 20 MB` (`Pss 8.5 MB` / `VmRSS 11.2 MB` terukur pada daemon headless Linux) tercapai tanpa dependensi langsung `libGL`/`libEGL`; hilangnya overlay di Hyprland saat sleep/reload teratasi tanpa jatuh ke window reguler. Jalur fallback `eframe` tetap dipertahankan untuk X11 dan jendela Settings. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[04 - Changelog & Dev Log]].

## ADR-019: Sinkronisasi MPRIS2 Presisi & Multi-Candidate LRCLIB Fetch (v0.1.1)
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pada Linux, pemutar musik seperti Chromium/Firefox/Spotify/MPV sering mengombinasikan beberapa bus MPRIS sekaligus, mengirim `mpris:length` bertipe `u64`, menunda pembaruan `Position` pada `GetAll`, menempelkan suffix seperti `(Official Video)` / `- Topic`, dan mengalami selisih timestamp saat `load_art_from_url` melakukan download HTTP cover album. Selain itu, LRCLIB gagal ditemukan saat metadata `artist` kosong atau berisi daftar multi-artis.
- **Keputusan:**
  1. **Seleksi & Sampling MPRIS2 (`src/platform/linux.rs`):** Menilai seluruh bus `org.mpris.MediaPlayer2.*` (`Playing` prioritas utama), mendukung decoding angka fleksibel (`i64`/`u64`/`i32`/`u32`/`f64`) dan `xesam:artist` tunggal maupun array, mengambil `Position` eksplisit lewat `Properties.Get` tepat saat mencatat `Instant`, meng-ambil ulang `Position` pasca download cover art lambat, memakai `SetPosition` pada seek, dan memakai `evaluate_mpris_sync` berbasis drift + konfirmasi lag 2-poll.
  2. **Multi-Candidate LRCLIB (`src/lyrics.rs`, `src/ui/mod.rs`, `src/daemon.rs`):** Membersihkan tag kebisingan dan suffix `- Topic`, memisahkan `Artist - Title` saat `artist` kosong, memecah artis utama, memprioritaskan hasil `synced_lyrics` walau tanpa durasi, mengulang fetch otomatis ketika `duration_ms` beralih `0 -> >0`, serta menambahkan tombol **Retry Fetch**.
- **Konsekuensi:** Lirik tersinkron halus dengan playback/seek Linux tanpa lompatan balik dan tingkat keberhasilan pencarian LRCLIB meningkat untuk pemutar desktop maupun browser; versi dinaikkan ke `v0.1.1`. Lihat [[04 - Changelog & Dev Log]].

## ADR-020: Pemulihan Fetch Album Cover MPRIS2 Linux (`data:`, Delayed URL, & `bmp`/`gif`) (v0.1.2)
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pada Linux, album cover sering gagal muncul karena: (1) player seperti Spotify/Chromium mengisi `mpris:artUrl` sesudah metadata judul/artis sehingga `cached_art` dan `sync_album_art_texture` terlanjur mengunci status `None`, (2) browser/Electron mengirim thumbnail inline sebagai `data:image/...;base64,...`, (3) beberapa player mengirim `file://localhost/...` atau path lokal polos, dan (4) `open.spotify.com/image/*` atau kegagalan HTTP sementara gagal di-decode.
- **Keputusan:** Menggunakan `ArtCacheState` yang melacak `(key, art_url)` + retry backoff 3 kali dalam thread MPRIS dengan satu `ureq::Agent` persisten, mendukung `data:image/...;base64,...`, `file://localhost/...`, fallback `open.spotify.com/image` → `i.scdn.co/image`, mengaktifkan fitur `bmp`/`gif` di crate `image`, memvalidasi perubahan pointer `Arc<AlbumArt>` pada `sync_album_art_texture`, serta mendokumentasikan alur rilis standar pengguna di `AGENTS.md`.
- **Konsekuensi:** Album cover dari desktop player maupun browser ter-fetch dan ter-render konsisten meski `artUrl` datang terlambat; versi naik ke `v0.1.2`. Lihat [[04 - Changelog & Dev Log]].

## ADR-021: Auto-Sized Centered Clock, LayoutJob Alignment, LyricsBold Family, & Blurred Full-Lyrics (v0.1.3)
- **Tanggal:** 2026-11-27
- **Status:** Accepted
- **Konteks:** Pada versi `v0.1.2`, ditemukan 4 isu UI/UX lintas Linux & Windows: (1) pill Idle Clock tidak otomatis mengubah lebar saat format/zona waktu berubah dan isinya condong ke kiri karena `ui.centered_and_justified` + `ui.horizontal` tetap left-aligned, (2) pengaturan `TextAlign::Center` tidak memusatkan teks single/multi-baris di pill mode maupun expanded mode (`ScrollArea` mereset layout ke `Align::Min` dan `Label::truncate`/`layout` memakai `Align::LEFT`), (3) `RichText::strong()` di `egui` tidak memakai huruf Bold untuk lirik utama, dan (4) mode expanded Music hanya merender baris aktif + 1 baris berikutnya alih-alih seluruh lirik dengan konteks sebelum/sesudah yang diblur.
- **Keputusan:**
  1. **Idle Clock Adaptif & Simetris (`src/ui/mod.rs`):** Lebar pill clock diukur pada frame aktif via `ui.painter().layout_job` (`clock_auto_width`, memakai `stabilized_clock_measure_text` agar detik tidak bergetar `±1 px`) + fallback `estimate_clock_pill_width`, sedangkan konten `[dot + waktu + zona]` digambar rata tengah melalui `render_dot_label(..., Align::Center)` dan pemosisian `eframe` membaca `viewport().monitor_size`.
  2. **Perataan Teks Lintas Mode:** Mengganti pemakaian `Label::wrap`/`truncate` pada area ber-`TextAlign` dengan `LayoutJob::halign` + `galley_draw_pos` (`render_aligned_wrapped_text`, `render_wrapped_karaoke`, `render_aligned_truncated_text`, `render_overview_row`, `render_compact`, `NextEvent`, `Minimal`).
  3. **Font `LyricsBold` (`src/platform/{mod,linux,windows}.rs`, `src/ui/mod.rs`):** Mendaftarkan keluarga font khusus `LyricsBold` dari font Bold OS (`NotoSans-Bold` / `DejaVuSans-Bold` / `fc-match sans-serif:bold` di Linux; `segoeuib.ttf` / `arialbd.ttf` / `malgunbd.ttf` di Windows) + sibling Bold custom font dan meluaskan fallback CJK menjadi `FontFamilyTarget::All`.
  4. **Semua Lirik & Blur Lintas Platform (`render_music_tab`, `render_blurred_aligned_text`):** Menampilkan seluruh baris `LyricsData::Synced` dan `LyricsData::Plain` di panel kanan Music Card, memicu fetch bila kunci lirik belum ada saat masuk Music Card, otomatis menggulirkan baris aktif ke tengah (`scroll_to_me`), dan menerapkan blur isotropik 12-pass tanpa inti tajam + fade berbasis jarak (`dist == 1 / 2 / >=3`, dengan skip paint saat di luar `clip_rect`) yang bekerja seragam pada `SoftCanvas` (Windows/Wayland) dan OpenGL.
- **Konsekuensi:** Pill clock menyesuaikan lebar dan selalu berada di tengah, `TextAlign::Center` konsisten di pill maupun expanded, lirik utama tampil Bold pada aksara Latin maupun CJK, dan lirik penuh di mode expanded menampilkan fokus karaoke tajam di antara lirik sebelum/sesudah yang diblur; versi rilis dinaikkan ke `v0.1.3`. Lihat [[04 - Changelog & Dev Log]].

## ADR-022: Perbaikan Pipeline Rilis Windows & Bump v0.1.4
- **Tanggal:** 2026-11-27
- **Status:** Accepted
- **Konteks:** Dua kali workflow `Release` gagal di job Windows. `cargo test` dan `clippy` sudah lolos setelah perbaikan lint sebelumnya, tetapi `makensis` gagal dengan `File: "dist\staging\liebert-dynamic-island.exe" -> no files found` karena NSIS me-resolve path relatif terhadap direktori skrip (`installer/windows/`), bukan working directory repository. Selain itu, refactor overlay berlapis memperkenalkan error `clippy` yang hanya muncul pada target Windows (helper eframe lama dianggap `dead_code`, `field_reassign_with_default` pada `BITMAPINFOHEADER`, `manual_map` pada parser IPC, dan `needless_return` di cabang layered client).
- **Keputusan:**
  1. **Path NSIS eksplisit (`.github/workflows/release.yml`, `installer/windows/installer.nsi`):** CI mengirim `/DSTAGING_DIR=<workspace>\dist\staging` dan `/DOUT_FILE=<workspace>\dist\liebert-dynamic-island-windows-x86_64-setup.exe`; default skrip juga diubah ke `..\..\dist\staging` & `..\..\dist\...-setup.exe` agar tetap benar bila dijalankan manual dari repository root.
  2. **Clippy Windows-target bersih:** Helper kompatibilitas eframe diberi atribut `dead_code` yang sesuai, `BITMAPINFOHEADER` dibuat sebagai struct literal di dalam `BITMAPINFO`, parser argumen memakai `Option::or`, dan cabang Windows memakai ekspresi `match` sebagai nilai fungsi.
  3. **Versi rilis `v0.1.4`:** Seluruh metadata versi (Cargo, AppImage, NSIS, AUR, string internal, README) diselaraskan; AppImage/tar.gz dibangun ulang lokal, sedangkan `.exe` portabel dan `*-setup.exe` tetap diproduksi oleh job Windows CI. Tidak ada fitur baru dalam rilis ini.
- **Konsekuensi:** Job Windows dapat menyelesaikan tahap NSIS dan publish-release mendapatkan keempat aset plus bundel AUR. Host Linux tidak memiliki toolchain Windows/NSIS permanen, tetapi sejak v0.1.7 installer final **juga** dapat dibangun lokal di container Docker (`rust:slim` + `gcc-mingw-w64-x86-64` + `nsis`) dengan cross-compile `x86_64-pc-windows-gnu`; di Linux `makensis` memakai opsi `-D` alih-alih `/D`. Lihat [[04 - Changelog & Dev Log]].

## ADR-023: System Tab Linux, Navigasi Arrow, Layout Rework & Batas RAM (v0.1.5)
- **Tanggal:** 2026-11-27
- **Status:** Accepted
- **Konteks:** Tab System previously kosong di Linux karena sampler/worker hanya ada di Windows; strip tab atas memakan ruang expanded; tidak ada navigasi keyboard; Settings memakai strip horizontal yang sulit dipindai; dan ukuran decode album art serta kloning delta tekstur menambah puncak RSS. Pengguna meminta perbaikan untuk Linux **dan** Windows dengan target RAM idle `< 10 MB` dan maksimal `30 MB`.
- **Keputusan:**
  1. **Sampler Linux tanpa dependensi baru (`src/sysmon.rs`, `src/battery.rs`):** `/proc/stat`, `/proc/meminfo`, `/proc/diskstats`, `/proc/net/dev`, `df -P -k /`, dan `/sys/class/power_supply` dibaca di worker yang di-spawn pada Wayland layer-shell, klien `--ui`, dan overlay Windows. Total CPU jiffies Linux dimasukkan kembali ke nilai ala `GetSystemTimes` agar `compute_cpu_delta` tidak mengurangi idle dua kali; satuan sysfs baterai dikonversi ke detik.
  2. **Navigasi & Header (`src/ui/mod.rs`, `src/platform/windows_overlay.rs`, `src/platform/wayland_layer.rs`):** Left/Right mengganti strip tab, dengan daftar modul mengikuti config dan wrap-around. Windows mendaftarkan `RegisterHotKey(VK_LEFT/VK_RIGHT)` hanya saat overlay terlihat dan melepasnya saat hide/exit. Wayland memakai `KeyboardInteractivity::OnDemand` + `KeyboardHandler` dengan komentar wajib `// shortcut: <batas>, <kapan upgrade>`; keyboard grab global compositor-agnostik belum tersedia lewat winit/eframe.
  3. **Layout (`src/ui/mod.rs`, `src/ui/settings.rs`):** Home lebih rapat, Music expanded menjadi lyrics-first full-width, Settings menjadi dua panel (grup kiri + konten kanan). Ini mengurangi widget per frame tanpa menambah abstraksi atau dependensi baru.
  4. **RAM (`src/ui/soft_render.rs`, `src/platform/mod.rs`, `Cargo.toml`):** Piksel `TexturesDelta` di-pindahkan (`Arc::get_mut`/`mem::take`) ke `SoftTexture`, decode gambar dibatasi `1024 px` / `8 MiB` dan di-resize `128×128`, serta `smithay-client-toolkit` mengaktifkan `xkbcommon`.
- **Konsekuensi:** `cargo test` 65 passed, clippy Linux + Windows-target bersih, dan paket v0.1.5 dibangun. RSS overlay Wayland terukur `VmRSS 23,2 MB` / `Pss 19,8 MB` (di bawah batas `30 MB`, di atas target idle `10 MB`). Fallback X11/eframe masih `VmRSS 15,6 MB` daemon + `83,8 MB` child UI, sehingga target `< 30 MB` untuk jalur X11 serta target `< 10 MB` untuk Wayland **belum tercapai** dan dicatat sebagai utang teknis; RSS Windows tidak dapat diukur dari host Linux. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].

## ADR-024: Arrow Vektor, Kembali ke Layout Music, Fetch Cover & RAM Settings (v0.1.6)
- **Tanggal:** 2026-11-27
- **Status:** Accepted
- **Konteks:** Tiga regresi v0.1.5 dilaporkan: (1) indikator `◀ ▶` berupa glyph Unicode tidak tampil pada beberapa instalasi Linux dan tidak bisa diklik bila overlay tidak memiliki fokus keyboard, (2) layout Music lyrics-first menghilangkan komposisi cover + lirik yang sebelumnya diharapkan, dan (3) album cover Linux berhenti ter-fetch. Investigasi menemukan batas decode `1024 px` / `8 MiB` menolak cover `1280×720`/`1500×1500` yang umum dari MPRIS, lalu kegagalan decode diunci oleh retry `ArtCacheState`. Pengguna juga meminta RAM jendela Settings `< 10 MB` di Linux dan Windows.
- **Keputusan:**
  1. **Arrow & Layout (`src/ui/icons.rs`, `src/ui/mod.rs`):** Chevron digambar sebagai `Shape` vektor dan kedua tombol memanggil `navigate_tab` secara langsung; layout `render_music_tab` dikembalikan ke implementasi `v0.1.4`.
  2. **Cover Linux (`src/platform/mod.rs`, `src/ui/mod.rs`):** Plafon decode dikembalikan ke `2048 px` / `16 MiB` sementara output tetap `128×128`; ditambah test JPEG `1280×720`. Cache tekstur beralih dari `Arc::as_ptr` ke hash konten agar `Arc` baru dari IPC tidak memicu upload berlebih dan alamat reused tidak menyisakan cover lama.
  3. **RAM Settings (`src/ui/settings.rs`, `Cargo.toml`):** Launcher `--settings` mencoba `egui_glow::HardwareAcceleration::Off` pada child process lebih dulu; jika abort karena tidak ada konfigurasi EGL software, launcher memulai ulang dengan `Preferred`. Ini bertujuan menghindari pemuatan driver GPU/LLVM ratusan MB tanpa mengorbankan fungsi ketika software GL tidak tersedia.
- **Konsekuensi:** `cargo test` 66 passed, clippy Linux + Windows-target bersih, paket v0.1.6 dibangun. Cover `1280×720` kini lolos decode dengan unit test. Target RAM Settings `< 10 MB` **belum tercapai** pada host pengujian Linux: child `VmRSS 102,8 MB` / `Pss 69,0 MB` dengan driver NVIDIA (`RssFile ≈80 MB` dari `libnvidia-gpucomp`, `libnvidia-eglcore`, `libLLVM`); launcher sendiri `7,3 MB`. Semua varian `LIBGL_ALWAYS_SOFTWARE=1` dan `EGL_PLATFORM=x11|wayland|surfaceless` tetap gagal membuat konfigurasi `hardware_accelerated: Some(false)`. RSS Windows tidak terukur dari host Linux. Utang teknis: jalur Settings perlu renderer CPU (`SoftCanvas`) atau window host ringan untuk benar-benar mencapai `< 10 MB`. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].

## ADR-025: Header Expanded Minimalis, Side Arrows & Status Baterai Eksplisit (v0.1.7)
- **Tanggal:** 2026-11-27
- **Status:** Accepted
- **Konteks:** Setelah v0.1.6, pengguna menegaskan kembali susunan mode Expanded: tab atas harus hilang, header hanya berisi tulisan tab aktif dan tombol Settings, sementara panah kiri/kanan berada di tengah kiri/kanan kartu (bukan di header). Settings sudah bergrup kiri/konten kanan dan perlu dipertahankan. System monitor Linux sudah ada, tetapi baterai tidak terlihat pada PC desktop sehingga pengguna mengira fiturnya tidak bekerja.
- **Keputusan:**
  1. **Header & Side Arrows (`src/ui/mod.rs`):** Menghapus chevron dan toggle Notch/Island dari header; toggle dipertahankan di Settings → Display. `draw_side_nav` menggambar dua tombol vektor di tengah tepi kartu memakai `ui.interact` + `paint_icon`, memanggil `navigate_tab` yang sama dengan keyboard/hotkey Win32/`KeyboardHandler` Wayland.
  2. **Settings & System Monitor:** Susunan grup kiri/konten kanan tidak diubah. `render_system_tab` (overlay) dan `render_system_tab` (Settings) kini menampilkan `No battery detected` bila `read_battery()` mengembalikan `None`, sehingga pembacaan sysfs jelas bekerja meski perangkat tidak ada.
  3. **Verifikasi (`src/sysmon.rs`):** Test `linux_sampler_reads_live_proc_metrics` menjalankan sampler `/proc` nyata untuk memastikan CPU/RAM terbaca; `cargo test` 67 passed, clippy Linux + Windows-target bersih.
- **Konsekuensi:** Header Expanded lebih bersih dan navigasi tetap terlihat tanpa mengorbankan tombol Settings. Versi naik ke `v0.1.7`; AppImage/tar.gz dibangun ulang. Utang RAM Settings dari ADR-024 tetap terbuka. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].

## ADR-032: Maskot Musik (EQ 3-Bar/Nod/Yepii), Fix Match Lirik, Swipe & Bar Timer Notif, Filter Web Brave, Glassmorphism & Semver Carry-Over v0.2.0
- **Tanggal:** 2026-11-27
- **Status:** Accepted (dengan utang teknis RAM `< 10 MB` pada total RssFile binary)
- **Konteks:** Pengguna meminta peningkatan lanjutan lintas Windows & Linux: (1) animasi maskot saat memutar lagu (3 bar EQ menggantikan mulut, angguk kiri/kanan, efek pop "yepii", tetap eye-tracking + berkedip), (2) perbaikan bug lirik kadang salah lagu, (3) geser (slide) untuk hapus notifikasi di tab `Notifications`, (4) bar timer berwarna mengecil ke tengah untuk durasi auto-dismiss notifikasi, (5) menyembunyikan nama web pada notifikasi Brave, (6) desain `Glassmorphism` baru + keterbacaan teks lebih jelas pada seluruh mode kaca, (7) pembaruan versi ke `0.2.0` + aturan carry-over versi (`>= 10`), dan (8) penambahan daftar kontributor GitHub pada tab About Settings.
- **Keputusan:**
  1. **Maskot Musik Terpadu (`src/ui/icons.rs`, `src/ui/mod.rs`):** Menambah `paint_mascot_music()` + `MascotMusicMotion` (`bars`, `dot`, `nod_x`, `yepii` scale pop 1,16×) memakai `paint_mascot_inner` bersama `paint_mascot_fx`, aktif di pill musik idle (`IdleDisplayMode::Lyrics`), mode `Mascot`/`Clock`, dan tile Jam Home sambil mempertahankan `is_mascot_visible_in_idle()` agar eye tracking & kedip terus berjalan.
  2. **Akurasi Pencarian & Sinkronisasi Lirik (`src/lyrics.rs`, `src/ui/mod.rs`):** Memasukkan 5-second `duration_secs` bucket ke `RawCacheKey`/`EnrichedCacheKey`, menyaring hasil `/api/search` & `/api/get` melalui `lrc_candidate_matches()` (`titles_match`, artist loose check, selisih durasi `<= 25 s`), serta mereset/memvalidasi `(artist, title)` secara case-insensitive saat track berpindah atau snapshot diterima.
  3. **UX Notifikasi & Filter Brave (`src/state.rs`, `src/platform/windows.rs`, `src/ui/mod.rs`):** `IslandState::collapse_remaining_frac()` menggerakkan bar timer simetris mengecil ke tengah di `render_compact`; tab `Notifications` mendukung geser horizontal `Sense::click_and_drag()` (`notif_swipe`) untuk menghapus kartu; dan `is_brave_app` + `strip_brave_origin_from_text` menghapus attribution domain web dari Brave lintas WinRT dan Linux D-Bus.
  4. **Glassmorphism, Keterbacaan Kaca, About & Alur Rilis (`src/config.rs`, `src/ui/{mod,settings}.rs`, `AGENTS.md`, `CHANGELOG.md`):** Menambah `BackgroundMaterial::Glassmorphism`, menaikkan opacity scrim kaca + kecerahan teks/muted saat mode kaca aktif, memasukkan `Zahwan316` & `Gjenius20` (sesuai GitHub contributors API) di About, memperbarui `AGENTS.md` §6 dengan semver carry-over `>= 10` menuju **`v0.2.0`** serta kewajiban setelah aman di `dev-main`: pindah ke `main`, merge, generate `CHANGELOG.md`, dan unggah seluruh artefak rilis (`AppImage`, `tar.gz`, Windows `.zip`, NSIS `setup.exe`, dan bundel AUR `.tar.gz`) ke GitHub Releases.
- **Konsekuensi:** `cargo fmt --all -- --check`, `cargo test --offline` **80 passed**, `cargo clippy --offline --all-targets -- -D warnings`, dan `cargo clippy --target x86_64-pc-windows-gnu` (Docker) bersih; AppImage/tar.gz Linux, `.zip`/installer NSIS Windows, serta `liebert-dynamic-island-aur-0.2.0.tar.gz` terbangun dan dirilis ke branch `main` + GitHub Releases `v0.2.0`. Pengukuran Wayland `--settings` **`18,21 MB` RSS (`13,47 MB` PSS; `3,33 MB` anon)** dan `--demo` **`20,01 MB` RSS (`16,25 MB` PSS; `5,08 MB` anon)** — target `< 10 MB` total masih tertahan `RssFile` halaman kode binary monolitik. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].

---

## ADR-031: Kedip & Glow Maskot, Tab Notifications + Ikon App, Komponen Kalender, Glass Materials, Tab About & Fix Home Layout v0.1.13
- **Tanggal:** 2026-11-27
- **Status:** Accepted (dengan utang teknis RAM `< 10 MB` pada total RssFile binary)
- **Konteks:** Pengguna meminta 8 penyempurnaan lintas Windows dan Linux: (1) animasi kedip dan halo glow pada maskot, (2) tab `About` di Settings berisi pembuat dan versi, (3) perbaikan layout tab Home yang masih berantakan di Linux, (4) penghapusan garis batas antara header & konten expanded + header dan jam memakai font Bold, (5) perbaikan layout notifikasi beserta ikon aplikasi + tab `Notifications` lengkap dengan quick-settings yang kebal terhadap `text_align`, (6) komponen kalender bulanan di tab Calendar, (7) ikon pada tab System Monitor, dan (8) material latar `Acrylic`, `Frosted Glass`, & `Liquid Glass` beserta togglenya di Settings — sekaligus task RAM `< 10 MB`, sync `/docs`, dan rilis `v0.1.13`.
- **Keputusan:**
  1. **Maskot & Bold Font Ringan (`src/ui/{icons,mod}.rs`):** `paint_mascot_fx()` menerima `eye_open` (`170 ms` kurva kedip smoothstep tiap `~3,6–5,2 s`) dan `glow_color` dua pass; `apply_configured_fonts_light()` memuat hanya 1 face Bold Latin primer (`NotoSans-Bold.ttf`/`segoeuib.ttf`, ~`71 kB` PSS via `mmap`) untuk `ui_bold_font()` (header expanded, jam `build_clock_layout_job`, judul notifikasi, & tile Home) tanpa memicu pembacaan font CJK Bold ~20 MB sebelum lirik aktif.
  2. **Tab Home Explicit-`Rect` (`src/ui/mod.rs`):** Menghapus `scope_builder` + nested layout di dalam tiap kartu Home (`render_overview_tab`), beralih ke `gallery_grid_layout` & lukis `Painter` langsung pada `hdr_rect`/`body_rect`/`bar_rect` hingga grid 4×2 agar tinggi/lebar simetris dan bebas overflow lintas DPI/OS.
  3. **Notifikasi & Ikon Aplikasi (`src/platform/{linux,windows}.rs`, `src/ui/mod.rs`):** Ekstraksi ikon 28×28 RGBA di cache 32 entri + cache tekstur UI 24 entri (`paint_notif_avatar_in_rect` dengan fallback monogram); tab **`NOTIFICATIONS`** menyimpan `notification_history` maks 30 entri plus toggle `Popups`/`DND`/`Test`/`Clear`/`Settings`; seluruh layout notifikasi dipatok ke `egui::Align::Min` mengabaikan `Config::text_align`.
  4. **Kalender, System Icons, About & Glass Materials (`src/config.rs`, `src/ui/{mod,settings,icons}.rs`):** `render_calendar_tab` terbelah menjadi mini month grid 7-kolom interaktif + daftar event; `render_stat_donut` + baris Network memakai `IconKind::{Cpu, Memory, Disk, ArrowDown, ArrowUp}`; `SettingsTab::About` menampilkan info `Zahwan316`, versi, repo, dan profil aktif; serta `BackgroundMaterial::{Solid, Acrylic, FrostedGlass, LiquidGlass}` menerapkan tekstur/kilau kaca translucent di pill/expanded (termasuk `Top Notch`) dan Settings.
- **Konsekuensi:** `cargo fmt --all -- --check`, `cargo test --offline` **77 passed**, `cargo clippy --offline --all-targets -- -D warnings`, dan `cargo clippy --target x86_64-pc-windows-gnu` (Docker `rust:slim` + `gcc-mingw-w64-x86-64` + `nsis`) bersih; AppImage `6,4 MB`, `.tar.gz` `6,2 MB`, NSIS setup `3,4 MB`, dan `.zip` `4,2 MB` terbangun untuk `v0.1.13`. **RAM terukur di Wayland:** `--settings` **`19,58 MB` RSS (`13,83 MB` PSS; `3,31 MB` anon)** dan `--demo` **`18,95 MB` RSS (`15,07 MB` PSS; `5,12 MB` anon)** — target `< 10 MB` total RSS/PSS tetap tertahan oleh `RssFile` binary monolitik (`8,0–9,8 MB` PSS) dan masih dicatat sebagai utang teknis. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].

---

## ADR-030: Sanitasi Notifikasi Web, Gallery Grid Tab Home & Optimasi Size/RAM v0.1.12
- **Tanggal:** 2026-11-27
- **Status:** Accepted (dengan utang teknis RAM `< 10 MB` pada total RssFile binary)
- **Konteks:** Pengguna melaporkan bug minor pada notifikasi WhatsApp Web/browser di Windows dan Linux yang masih menampilkan raw HTML `<a href=...>` atau isi pesan tidak tersanitasi, serta meminta tab Home didesain ulang menyerupai **Gallery Grid** (bukan kartu asimetris). Standing preference: jalankan juga task penurunan RAM `< 10 MB`, rilis `0.1.12`, dan sinkronisasi `/docs`.
- **Keputusan:**
  1. **Sanitasi Terpusat (`src/state.rs`, `src/platform/{linux,windows}.rs`):** Menambahkan `sanitize_notification_text()` dan `sanitized_notification_item()` di `src/state.rs`, diterapkan di titik masuk WinRT (`extract_notification_item`), D-Bus server & monitor (`NotificationServer::notify`, `decode_monitored_message`), serta disaring kembali di `IslandState::push_notification` sebelum pemotongan `MAX_BODY_BYTES`. Pendekatan 2-pass decode+strip menangani markup asli (`<a href="...">`, `<br>`, `<b>`, tag terpotong) maupun HTML yang sudah di-escape (`&lt;a href=...&gt;`) tanpa merusak angka perbandingan, `<3`, maupun `<email>`.
  2. **Tab Home Gallery Grid (`src/ui/mod.rs`):** `render_overview_tab` memakai tile berukuran seragam (`tile_w × tile_h`, `3×2` pada konfigurasi lengkap dengan tinggi expanded `196*s`), header ikon/badge seragam (`render_gallery_tile_header`), preview visual fokus (hero maskot/clock, thumbnail cover musik, metrik sistem pill bar, ringkasan tasks/focus/calendar, serta tile `Battery` untuk mengisi slot kosong bila ada modul dimatikan).
  3. **RAM & Size (`Cargo.toml`, `src/platform/wayland_settings.rs`):** Memindahkan profil rilis ke `lto = "fat"` + `opt-level = "z"` dan memangkas alokasi awal SHM `wayland_settings.rs` ke `1×` buffer (`byte_len`).
- **Konsekuensi:** `cargo fmt --all -- --check`, `cargo test` **76 passed**, `cargo clippy --all-targets -- -D warnings`, dan clippy Windows-target (Docker `rust:slim` + `gcc-mingw-w64-x86-64`) bersih; AppImage `6,4 MB` + tar.gz `6,1 MB` Linux dan installer NSIS `3,29 MB` + zip Windows `v0.1.12` dibangun. **Pengukuran RAM Wayland:** `--settings` turun menjadi **`16,94 MB` RSS (`12,82 MB` PSS; `3,30 MB` anon)** dan `--demo` turun menjadi **`18,85 MB` RSS (`15,08 MB` PSS; `4,98 MB` anon)**. Total RSS masih di atas `10 MB` karena halaman kode binary monolitik (`9–10,6 MB`), sehingga pemisahan binary/penghapusan `eframe` dicatat sebagai utang teknis lanjutan. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].

---

## ADR-029: Maskot Idle Cursor-Tracking, Fix Karaoke Blink, Bento Grid Home/Settings & CPU Settings Renderer v0.1.11
- **Tanggal:** 2026-11-27
- **Status:** Accepted (dengan utang teknis RAM `< 10 MB` pada total RssFile binary)
- **Konteks:** Pengguna meminta 5 peningkatan lintas Windows dan Linux: (1) maskot idle sesuai desain referensi dengan mata mengikuti posisi kursor, (2) penurunan RAM overlay & Settings `< 10 MB`, (3) perbaikan highlight putih lirik karaoke yang kadang berkedip/nge-blink, (4) tab `Home` minimalis memakai pill chart / kotak membulat / Bento Grid, dan (5) tampilan `Settings` berkonsep dashboard Bento Grid.
- **Keputusan:**
  1. **Maskot Idle Pelacak Kursor (`src/config.rs`, `src/platform/{mod,linux,windows}.rs`, `src/ui/{icons,mod,settings}.rs`):** Menambahkan `IdleDisplayMode::Mascot` ("Mascot (Eye Tracking)") serta `Config::show_idle_mascot` (default `true`) agar maskot langsung tampil di mode `Auto`/`Clock`/`Minimal` atau penuh saat `Mascot`. `paint_mascot()` menggambar mata kiri `L` & kanan `I` berkepala bulat serta mulut kapsul outline + kapsul vertikal; arah mata disampling dari `cursor_screen_info` (Hyprland/X11 di Linux, Win32 `GetCursorPos` + DPI di Windows) dengan interpolasi halus.
  2. **Fix Karaoke White Blink (`src/ui/mod.rs`, `src/platform/linux.rs`):** Menghilangkan tumpang-tindih AA antara `dim_galley` dan `lit_galley` lewat dua clip horizontal terpisah (`paint_karaoke_in_rect` dan midpoint row bands `render_wrapped_karaoke`), mengoreksi `scroll_x` memakai lebar penuh galai, menyimpan `prev_progress` pada `PillLyricTransition`, serta memfilter jitter mundur `< 750 ms` dan `periodic_lock` MPRIS2.
  3. **Bento Grid Home & Settings (`src/ui/{mod,settings}.rs`):** `render_overview_tab` ditata memakai grid kartu membulat dengan pill progress chart (`paint_pill_bar`) untuk System & Tasks. `settings.rs` dirombak ke dashboard Bento Grid 2-kolom dengan rail ikon kiri, mini-maskot live, pill chip, dan lazy font discovery.
  4. **CPU Settings Renderer & Optimasi RAM (`src/platform/{wayland_settings,windows_settings}.rs`, `Cargo.toml`, `src/ui/{mod,taskbar}.rs`):** Menghapus `default_fonts` `eframe`, memakai `memmap2::Mmap` untuk font besar, membatasi `max_texture_side` ke `1024` (nilai minimum valid `epaint::TextureAtlas`, karena `512` memicu panic `Tiny texture atlas`), mengganti simbol taskbar menjadi ASCII, menunda font bold lirik sampai musik aktif, dan mengganti `eframe` di `--settings` dengan CPU `SoftCanvas` murni via `wl_shm` (Wayland) dan Win32 DIB `BitBlt` (Windows).
- **Konsekuensi:** `cargo fmt --all -- --check`, `cargo test` **75 passed**, `cargo clippy --all-targets -- -D warnings`, dan clippy Windows-target (Docker `rust:slim` + `gcc-mingw-w64-x86-64`) bersih; AppImage/tar.gz Linux serta installer/zip Windows v0.1.11 dibangun. **Hasil RAM terukur (Hyprland Wayland `--release`):** `--settings` anjlok dari `90,2 MB → 17,63 MB RSS` (`13,05 MB PSS`, **`3,51 MB` anon/private**); `--demo` turun dari `63,1 MB → 24,63 MB RSS` (`19,36 MB PSS`, **`5,75 MB` anon/private**). Penghalang agar total proses `< 10 MB` kini tinggal halaman kode binary monolitik `RssFile` (`9,6–13,7 MB` PSS) akibat bundel `eframe`/`egui_glow`/`winit`/`rustls`/`zbus` dalam satu executable 17 MB. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].

---

## ADR-028: UI Polish (Expanded/Home/Settings/Animasi/Gradient) & Batas RAM v0.1.10
- **Tanggal:** 2026-11-27
- **Status:** Accepted (dengan utang teknis RAM)
- **Konteks:** Pengguna meminta perbaikan ruang kosong di atas expanded, Home lebih minimalis, animasi saat next/prev tab, layout yang selalu center, Settings lebih minimalis, dan warna gradient yang enak dipandang—semuanya di Windows **dan** Linux. Standing preference: setiap request perubahan kode harus menjalankan task penurunan RAM ke `< 10 MB` untuk overlay dan Settings.
- **Keputusan:**
  1. **Expanded & centering (`src/ui/mod.rs`):** Inset atas `9*s` (music `10*s`) diturunkan ke `2*s`, separator dipadatkan, dan ambang `expand_alpha` diturunkan ke `30*s/32*s`. Semua tab expanded memakai lebar `max(468)*s` agar kartu tidak berubah lebar saat pindah tab; label header dan jam Home benar-benar terpusat dengan item spacing nol; kolom Music disamakan dan wrap lirik dipatok agar scrollbar tidak menggeser teks. Gutter `26*s` + chevron tengah dipertahankan.
  2. **Home & animasi tab (`src/ui/mod.rs`):** Clock + timezone sebaris, placeholder "No music playing" dihapus, row `21*s → 18*s` dan gap dipadatkan. `TabSwitch`/`switch_tab()` menjadi satu jalur untuk keyboard, side nav, dan click-through; transisi fade + slide 180 ms hanya meminta repaint cepat selama transisi (idle tidak berubah). Home click-through kini ikut meminta repaint—sebelumnya tidak.
  3. **Settings & warna (`src/ui/settings.rs`, `src/ui/mod.rs`):** Judul tab duplikat dihapus, `settings_group()` jadi flat, margin/spacing diperketat, hint berulang dipangkas tanpa mengurangi kontrol. `soft_accent_rgb()` menarik warna album/aksen ke netral dengan kap `190`; gradient wash `0,35–0,85` → `0,25–0,55`, rose glow diganti highlight netral; base docked/pill dan warna donut/status diredupkan.
  4. **RAM (`src/ui/mod.rs`, `src/ui/settings.rs`):** Byte font dibagikan via `Arc<FontData::from_blob>` (tidak ada `to_vec()` duplicate), fallback font tidak menyimpan `Vec<u8>` tak terpakai, dan launcher Settings Unix memakai `exec` untuk hardware GL (menghapus proses launcher ~7 MB saat window aktif). Tidak ada dependensi baru.
- **Konsekuensi:** `cargo fmt --all -- --check`, `cargo test` **74 passed**, `cargo clippy --all-targets -- -D warnings`, dan clippy Windows-target bersih; AppImage/tar.gz + installer/zip v0.1.10 dibangun. **Target `< 10 MB` tidak tercapai**: overlay `VmRSS 63,1 MB` / `Pss 59,0 MB` (hampir sama dengan baseline), Settings `VmRSS 90,2 MB` / `Pss 78,2 MB` (turun dari `102,8 MB` tapi `RssFile 67,8 MB` driver NVIDIA/LLVM tetap dominan). Investigasi menemukan ~52 MB region anonymous private-dirty pada daemon headless tanpa egui—bukan font/atlas—sehingga font trimming saja tidak cukup. Jalan yang kredibel: renderer CPU untuk Settings (menggantikan `egui_glow`) + pemisahan binary/fitur + identifikasi pemilik region anonymous; diperkirakan 2–5 hari kerja dengan risiko input/IME/DPI tinggi. Utang teknis tetap terbuka di [[02 - Daftar Tugas & Backlog]]. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[04 - Changelog & Dev Log]].

---

## ADR-027: Idle Battery Mode & Expanded Layout Polish v0.1.9
- **Tanggal:** 2026-11-27
- **Status:** Diterima
- **Konteks:** Pengguna melaporkan ruang kosong di atas konten expanded (terutama sekitar arrow kiri/kanan), meminta tab Home lebih minimalis, dan meminta opsi tampilan idle berupa status baterai di Linux dan Windows.
- **Keputusan:**
  - `IdleDisplayMode::Battery` ditambahkan ke config/settings dan dirender di idle pill memakai `SystemMetrics.battery` dari sysmon (Linux sysfs + Windows Win32), dengan fallback `No battery` bila perangkat tidak punya baterai (`src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`).
  - Konten expanded ditempatkan dalam `content_rect` yang di-inset sebesar gutter arrow side-nav (`26*s`) agar tidak tertutup panah/ada ruang kosong di tepi kartu; header tetap minimal (label tab + Settings) dan Home diringkas.
- **Konsekuensi:** UX expanded lebih rapi dan mode idle battery tersedia lintas OS. `cargo test` 72 passed, clippy bersih. Tidak ada crate baru.

---

## ADR-026: Autostart, Tab Battery, Chart Donut, Monitor Pasif D-Bus & Batas RAM v0.1.8
- **Tanggal:** 2026-11-27
- **Status:** Accepted (dengan utang teknis RAM)
- **Konteks:** Pengguna meminta sekaligus: autostart di Windows & Linux, perbaikan layout Home, chart System menjadi round chart, RAM `< 10 MB`, ruang untuk arrow kiri/kanan, pengelompokan isi tiap tab Settings, deteksi penyimpanan Linux untuk keseluruhan folder sistem, perapian tab Tasks, tab Battery baru dengan animasi saat charging, dan perbaikan notifikasi Linux yang tidak masuk ke island. Investigasi mengonfirmasi akar masalah notifikasi Linux: `org.freedesktop.Notifications` dipegang quickshell (PID 8800, `:1.70`), sehingga server D-Bus aplikasi tidak pernah menerima `Notify`; PRD mensyaratkan listener tetap bekerja tanpa mematikan daemon pengguna.
- **Keputusan:**
  1. **Autostart tanpa hak istimewa (`src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`, `src/config.rs`, `src/ui/settings.rs`, `src/daemon.rs`):** Satu facade `set_autostart(bool)`. Windows memakai HKCU `...\CurrentVersion\Run` via `RegCreateKeyW`/`RegSetValueExW`/`RegDeleteValueW` (`RegCreateKeyW` dipilih daripada `RegCreateKeyExW` agar tidak menambah fitur `Win32_Security`); Linux menulis `~/.config/autostart/liebert-dynamic-island.desktop` dengan escaping `Exec` dan preferensi `APPIMAGE`. Field config `start_with_os` (default `false`) dan `show_battery` (default `true`) ditambahkan dengan fallback aman untuk TOML lama. Aplikasi hanya dipanggil saat nilai berubah, di tiga jalur simpan config.
  2. **UI (`src/ui/mod.rs`, `src/ui/settings.rs`):** Tab `Battery` baru (percent donut + status + sisa waktu + animasi charging) dan `No battery detected` bila sysfs kosong; CPU/RAM/Disk memakai donut vektor; inset tengah menyediakan ruang chevron; Home/Tasks dirapikan; panel kanan Settings memakai `settings_group()` dengan judul tab di atas. Tidak ada dependensi UI baru.
  3. **Notifikasi Linux (`src/platform/linux.rs`, `Cargo.toml`):** Bila klaim nama gagal, `BecomeMonitor` + `MessageStream` (`futures-util`) memantau `Notify`/`CloseNotification` dan memetakannya ke `PlatformEvent` yang sama; status `Listening` dipertahankan, `--spike-notifications` melaporkan owner + mode, dan kegagalan keduanya menghasilkan `Error` yang menjelaskan penyebabnya.
  4. **Storage Linux (`src/sysmon.rs`):** `df -P -k` tanpa argumen path, diagregasi untuk semua filesystem nyata (skip pseudo), cache 10 detik dipertahankan; unit test memakai output `df` tiruan.
  5. **RAM:** Hash album art dihitung sekali per sidik jari murah (`album_art_fingerprint`/`album_art_hash`) dan lebar clock di-cache per detik. Tidak ada perubahan renderer Settings pada rilis ini.
- **Konsekuensi:** `cargo test` **72 passed**, `cargo clippy --all-targets -- -D warnings` bersih, clippy Windows-target bersih (container Docker `rust:slim` + `gcc-mingw-w64-x86-64` + `nsis`), AppImage/tar.gz + installer/zip v0.1.8 dibangun. **Target `< 10 MB` tidak tercapai**: overlay Wayland/`--demo` `VmRSS 63,1 MB` / `Pss 59,0 MB` (RssAnon `46,1 MB`, RssFile `16,6 MB`); jendela Settings masih `102,8 MB` pada host NVIDIA, dan RSS Windows tidak terukur dari host Linux. Utang teknis tetap terbuka: untuk mencapai `< 10 MB` perlu renderer CPU tanpa `egui`/GL pada overlay dan Settings, bukan sekadar cache per-frame. Lihat [[01 - Arsitektur & Spesifikasi]], [[02 - Daftar Tugas & Backlog]], dan [[04 - Changelog & Dev Log]].
