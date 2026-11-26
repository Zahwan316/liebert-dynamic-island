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
