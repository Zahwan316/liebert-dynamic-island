---
type: backlog
status: active
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - tasks
---

# 02 - Daftar Tugas & Backlog

Referensi cakupan fitur: [[PRD-LiebertDynamicIsland-MVP]].

## P0 — Must Have (MVP)
### Fase 1: Setup Proyek & Technical Spike
- [x] Inisialisasi `Cargo.toml` dan skeleton modul (`src/main.rs`, `src/config.rs`, `src/state.rs`, `src/tray.rs`, `src/ui/`, `src/platform/`)
- [x] Spike Windows: uji `UserNotificationListener::Current()` pada `.exe` unpackaged vs Sparse Package/MSIX manifest *(lulus di `.exe` unpackaged: `Access: Allowed`, `21` active toasts terbaca)*
- [ ] Spike Linux: uji D-Bus `org.freedesktop.Notifications` (`zbus`) & overlay `wlr-layer-shell` (Wayland) / dock (X11) di environment Linux *(koneksi D-Bus sesi lolos via `--spike-notifications`, dan overlay Hyprland Wayland + `ksni` tray terverifikasi live; sejak v0.1.8 fallback monitor pasif aktif bila nama dipegang daemon lain, tetapi uji notifikasi nyata end-to-end masih pending)*
- [x] Finalisasi pilihan renderer Rust (`eframe 0.37` + `Renderer::Glow`) di [[03 - Catatan Rapat & Keputusan]]

### Fase 2: Core Features
- [x] Definisikan `NotificationItem`, state machine (`Idle` ↔ `Compact` ↔ `Expanded`), dan queue sederhana di `src/state.rs`
- [ ] Implementasikan bridge Windows WinRT & Linux D-Bus beserta error handling izin/daemon *(Windows terimplementasi & lolos spike; Linux `src/platform/linux.rs` kini lolos build release & spike D-Bus, dengan fallback monitor pasif saat daemon lain aktif; verifikasi runtime dengan notifikasi nyata masih pending)*
- [x] Implementasikan System Tray (`Pause/Resume`, `Auto-hide when idle`, `Exit`) dan persistensi `config.toml` (`%APPDATA%` / `~/.config`)

### Fase 3: UI & Animation
- [x] Bangun window overlay borderless transparan top-center yang non-focus-stealing (`src/ui/mod.rs` + Win32 `WS_EX_NOACTIVATE`)
- [x] Implementasikan layout 3 state (`Idle`, `Compact`, `Expanded`) beserta animasi transisi ukuran (`eframe` `Glow`)
- [x] Implementasikan interaksi: auto-collapse 5 detik, hover expand & pause timer, serta tombol `Dismiss`

### Fase 4: Testing & Release
- [x] Tambahkan unit test state machine, config, clock, lyrics, todo, calendar, pomodoro, album accent, fullscreen rect, & text truncation (`cargo test` — 35 tests passed)
- [ ] Uji manual edge cases: multi-notifikasi berurutan, teks panjang/Unicode, multi-DPI scaling, hover expand live, top-edge hover reveal saat auto-hide, Spotify lyrics, & Google Calendar `.ics`
- [ ] Verifikasi target performa `--release`: RAM idle `< 10 MB` (target yang diminta pengguna), CPU idle `< 0.5%`, latency `< 150 ms` *(v0.1.8: RSS Wayland/`--demo` `63,1 MB`, `Pss 59,0 MB`; Settings `102,8 MB` pada host NVIDIA; target `< 10 MB` dan fallback X11 `< 30 MB` **masih terbuka**, lihat ADR-026)*
- [x] Perbarui `README.md` (cara build, izin Windows, catatan daemon Linux) & lakukan smoke test binary Windows + Linux *(README.md diperbarui sesuai kode aktual; smoke test Linux masih pending)*
- [x] **Paket rilis v0.1.0:** AppImage + tar.gz Linux dibangun lokal; `PKGBUILD`/`.SRCINFO` AUR, skrip NSIS Windows, dan workflow GitHub Actions disiapkan untuk publikasi aset saat tag `v*` (installer Windows dibangun di CI; submit AUR masih membutuhkan akun maintainer)

## Fitur Tambahan & Ekspansi (Di Luar Scope Awal P0)
- [x] **Music & Synced Lyrics:** Pembaca media SMTC (Windows) & MPRIS2 (Linux) + LRC lyrics via LRCLIB (`src/lyrics.rs`)
- [x] **Karaoke, Romaji, Translate & Full Playbar:** Progressive karaoke highlight (baris/kata `<mm:ss.xx>`), romanisasi non-Latin + fallback Kana, terjemahan lirik, seekable playbar + Shuffle/Prev/Play/Next/Repeat, serta lazy CJK font fallback
- [x] **Reference Music Card UI & Album Art:** Redesign 2-kolom dengan cover album asli (SMTC/MPRIS2), gradient wash warna dominan dinamis, multi-line wrapped karaoke `LayoutJob`, dan upcoming lyric preview redup
- [x] **Top-Edge Notch Dock Mode, Offline Romaji Fix & UI Polish:** Mode notch menempel tepi atas layar (`y = 0`) dengan telinga cekung + mini cover + 3-bar equalizer (sembunyi saat fullscreen), romanisasi offline `kakasi`+`any_ascii` untuk Kanji/Kana/Cina/Korea/Arab, pemindahan toggle Karaoke/Romaji/Translate ke popup Settings saja, dan polish UI menyeluruh
- [x] **Seamless Win11 Notch, Romaji Karaoke Mode, Branded Notification Popups & Morphing Transition:** Penghapusan border/rounding DWM Windows 11 + `pin_window_top_y(..., 0)` agar menyatu sempurna di tepi atas layar, pilihan `KaraokeLyricMode` (`Original` vs `Romaji`), listener notifikasi WinRT + fallback polling `500 ms` dengan popup berwarna brand (`WhatsApp`, `Discord`, `Gmail`, browser) + `Test Popup`, serta morphing halus `Top Notch` ↔ `Floating Island` (`notch_morph`)
- [x] **Perbaikan Stabilitas Win32 Hover, Anti-Overlap UI, Sinkronisasi Romaji & Akses Settings:** Penghapusan `DWMNCRP_DISABLED` & tombol caption Win32 (`WS_SYSMENU|WS_MINIMIZEBOX|WS_MAXIMIZEBOX`) agar tidak glitch saat hover, ekstrapolasi `LastUpdatedTime` SMTC + match durasi LRCLIB + `end_ms` jeda instrumental + `lyric_offset_ms` + dual-galley wrapped karaoke, layout header/kolom terklip rapi (`ScrollArea` lirik), serta aktivasi foreground jendela Settings (`bring_window_to_front`)
- [x] **Fullscreen Notification Pass-Through, Mode Toggle Khusus Settings & Alignment UI:** Dynamic Island tetap tersembunyi saat fullscreen idle tetapi otomatis muncul di `HWND_TOPMOST` ketika notifikasi masuk, pergantian `Top Notch` ↔ `Floating Island` khusus di Settings, serta perataan presisi seluruh ikon (`render_icon_button`), tab, clock/minimal `LayoutJob`, playbar, dan baris ringkasan (`cargo test` 25 passed)
- [x] **Penghapusan Garis 1px Atas (`WM_NCCALCSIZE`), Envelope Smooth Morph/Hover, Display Mode Terlihat di Settings & Fullscreen Watcher:** Subclass `overlay_wndproc` untuk menghapus inset `1 px` `winit`, animasi halus di atas envelope canvas tetap + `is_cursor_in_screen_rect`, selector Display Mode menonjol di paling atas Settings, serta `SHQueryUserNotificationState` + background `spawn_fullscreen_watcher` (`250 ms`)
- [x] **Kanvas Permanen Anti-Flicker, Direct Win32 Visibility & Click-Through, Alignment Vektor & Fix Pengetesan:** `permanent_canvas_size` (`y = 0`) + `set_overlay_click_through` (`WS_EX_LAYERED | WS_EX_TRANSPARENT`) tanpa resize/move saat buka-tutup, `set_overlay_visible` (`SW_HIDE`/`SW_SHOWNOACTIVATE`) + `DWMWA_EXTENDED_FRAME_BOUNDS` + ambang taskbar `>= 20 px`, ikon vektor `paint_vector_icon`, `render_dot_label`, `render_centered_truncated_text`, tombol `Send Test Notification` unclipped + auto-unpause (`cargo test` 27 passed)
- [x] **Responsivitas Tombol Linux, Pemisahan Logika OS & Ikon Vektor (`src/ui/icons.rs`):** Pemisahan penuh bridge overlay/hover/fokus/font Windows vs Linux di `src/platform/`, perbaikan `.with_resizable` Wayland + fast-path hover nol-IPC & cache/throttle Hyprland IPC + hitbox tombol nyaman (`CursorIcon::PointingHand`), ikon vektor `Painter` bebas tofu, serta penemuan font lintas distro Linux (`NotoSans`, `NotoSansMono`, `NotoSansSymbols`, `NotoSansCJK` multi-index `.ttc` + `fc-match`)
- [x] **Menu Settings:** Secondary viewport untuk ukuran, warna, tema preset, corner radius, dan toggle modul (`src/ui/settings.rs`)
- [x] **Todo List:** Checklist lokal dengan persistensi `todos.toml` (`src/todo.rs`)
- [x] **Google Calendar:** Sync feed Secret iCal `.ics` + upcoming events & alert 5 menit (`src/calendar.rs`)
- [x] **Hover Fix:** Perbaikan hover-expand saat `Idle` & Win32 cursor hit-test untuk window `WS_EX_NOACTIVATE`
- [x] **Jam & Zona Waktu + Idle Display Mode:** Jam IANA (`chrono-tz`), format 12h/24h, detik, dan pilihan tampilan idle (`Auto`, `Clock`, `Lyrics`, `NextEvent`, `Minimal`) di `src/clock.rs` & `src/ui/`
- [x] **Delayed Auto-Hide + Top-Edge Hover Reveal:** Auto-hide setelah *N* detik idle (`auto_hide_delay_secs`) dan muncul kembali saat kursor mengarah ke tepi atas tengah layar (Windows)
- [x] **Fullscreen / Game Auto-Hide:** Sembunyikan pill otomatis saat aplikasi/game/video fullscreen aktif di monitor utama (Windows; toggle di Settings)
- [x] **Quick Timer / Pomodoro:** Countdown fokus langsung di pill idle, tab `⏱ Timer`, notifikasi selesai fase, & pengaturan durasi (`src/pomodoro.rs`)
- [x] **Device Font Picker & Karaoke Translate Mode:** Pilihan font terinstal perangkat di Settings (`list_installed_fonts`, `apply_configured_fonts`), opsi `KaraokeLyricMode::Translate`, serta perbaikan windowed-fullscreen + timer `auto_hide_delay_secs` pasca-fullscreen
- [x] **Text Alignment & Stabilitas Memori/Daur Hidup:** Tombol `Left` / `Center` / `Right` di Settings (diterapkan ke lirik, notifikasi, judul lagu, clock, todo, kalendar), root viewport Linux tidak lagi ikut tertutup saat WM mengirim close, layer-shell closed → fallback eframe, tekstur GPU selalu di-drain, antrean notifikasi/cache lirik/alert kalender dibatasi (`cargo test` 41 passed, clippy bersih)
- [x] **Resolusi Merge Conflict, CPU Rasterizer (`SoftCanvas`), Fix Hyprland `wl_shm`, & Win32 Layered Overlay:** Seluruh error merge PR #15 selesai; overlay utama Linux Wayland memakai `wl_shm` (`SlotPool`) + `SoftCanvas` + pemulihan `LayerSurface` otomatis pasca `closed`/hotplug + `spawn_fullscreen_watcher`; Windows memakai `UpdateLayeredWindow` + DIB (`src/platform/windows_overlay.rs`) + `--settings --ipc-addr` terpisah tanpa memuat OpenGL pada proses overlay utama; font & proxy MPRIS `CacheProperties::No` dioptimalkan (`cargo test` 48 passed, `cargo clippy` bersih, AppImage `7.0 MB` terbangun lokal, `Pss` daemon Linux `8.5 MB`)
- [x] **Fix Fetch Lirik & Sync MPRIS2 Linux + Rilis v0.1.1:** Pemilihan pemutar MPRIS aktif, pembacaan `Position`/`mpris:length` fleksibel (`i64`/`u64`/`f64`) & `xesam:artist` tunggal/array via `Properties.Get`, seeking `SetPosition`, kalibrasi drift & cover-art latency, normalisasi judul/artis LRCLIB multi-kandidat, tombol **Retry Fetch**, serta pembaruan versi & kemasan Linux `v0.1.1` (`cargo test` 53 passed, clippy bersih)
- [x] **Fix Album Cover Linux + Aturan Rilis v0.1.2:** Perbaikan cache cover tertunda `ArtCacheState`, dekoder `data:image/...;base64,...`, dukungan `file://localhost/` + path polos + `bmp`/`gif`, fallback URL Spotify, pembaruan tekstur cover di `sync_album_art_texture`, pencatatan alur rilis di `AGENTS.md`, serta pembaruan versi ke `v0.1.2` (`cargo test` 54 passed, AppImage terbangun)
- [x] **Clock Auto-Width & Centering, TextAlign Center Pill/Expand, Font Bold Lirik, & Blurred Full-Lyrics v0.1.3:** Auto-resize lebar & perataan tengah idle clock, perbaikan menyeluruh `TextAlign::Center` pill & expand (`LayoutJob::halign` + `galley_draw_pos`), registrasi keluarga font `LyricsBold` untuk lirik utama Linux/Windows, tampilan seluruh lirik di tab Music expand mode dengan blur bertingkat pada lirik sebelum/sesudah baris aktif, verifikasi 62 unit test + `cargo clippy` bersih, dan bump rilis ke `v0.1.3`
- [x] **Perbaikan Pipeline Rilis v0.1.4 (tanpa fitur baru):** NSIS menerima `STAGING_DIR`/`OUT_FILE` absolut dari CI sehingga `File` tidak lagi gagal mencari `dist\staging\liebert-dynamic-island.exe`; helper Windows lama dan kode overlay berlapis dibersihkan dari error `clippy` Windows-target; versi dan metadata AUR/README diselaraskan ke `v0.1.4`; AppImage + tar.gz dibangun ulang (`cargo test` 62 passed, clippy Linux & Windows bersih)
- [x] **Deteksi System Tab Linux, Navigasi Arrow & Layout v0.1.5:** Sampler `/proc` + sysfs (`src/sysmon.rs`, `src/battery.rs`) dihubungkan ke Wayland, `--ui`, dan overlay Windows; tab atas diganti label + arrow key (hotkey Win32 & `KeyboardHandler` Wayland); Home/Music/Settings ditata ulang; delta tekstur tidak di-clone dan decode album art dibatasi. `cargo test` 65 passed + clippy Linux/Windows bersih. Catatan: RSS Wayland `23,2 MB` (di bawah `30 MB`) namun idle `< 10 MB` dan fallback X11 `83,8 MB` belum tercapai. Lihat [[04 - Changelog & Dev Log]] (ADR-023).
- [x] **Perbaikan Arrow/Vektor, Kembali ke Layout Music Lama, Fetch Cover Linux & RAM Settings v0.1.6:** Chevron kiri/kanan kini tombol vektor yang selalu tampil, layout Music expanded dikembalikan, batas decode cover `2048 px`/`16 MiB` + cache tekstur berbasis hash konten, dan launcher Settings mencoba software GL sebelum fallback. `cargo test` 66 passed + clippy Linux/Windows bersih. Catatan: jendela Settings `VmRSS 102,8 MB` pada host NVIDIA — target `< 10 MB` belum tercapai. Lihat [[04 - Changelog & Dev Log]] (ADR-024).
- [x] **Header Expanded Only Label + Settings & Side Arrows, Battery "No battery detected" v0.1.7:** Tab strip/chevron header/toggle mode dihapus; chevron dipindah ke tengah tepi kartu. Settings tetap grup kiri/konten kanan. Sampler `/proc` Linux diuji live dan status baterai eksplisit saat perangkat tidak ada. `cargo test` 67 passed + clippy Linux/Windows bersih. Lihat [[04 - Changelog & Dev Log]] (ADR-025).
- [x] **Installer Windows v0.1.7 dibangun dari host Linux:** Docker `rust:slim` + `gcc-mingw-w64-x86-64` + `nsis` menghasilkan `dist/liebert-dynamic-island-windows-x86_64-setup.exe` dan `.zip`; `makensis` di Linux memakai `-D` (bukan `/D`). Artefak diverifikasi dengan `file` + `7z l`; README mendapat perintah lengkapnya.
- [x] **Autostart, Tab Battery, Donut Chart, Settings Bergrup & Notifikasi Linux v0.1.8:** Toggle autostart per-user (Windows HKCU Run + Linux XDG `.desktop` sadar `APPIMAGE`); tab Battery dengan animasi charging + `No battery detected`; donut chart CPU/RAM/Disk; inset arrow expanded; Home/Tasks dirapikan; panel Settings bergrup; disk Linux diagregasi lintas mount; fallback monitor pasif D-Bus. `cargo test` 72 passed, clippy Linux + Windows-target bersih, artefak v0.1.8 dibangun. Target RAM `< 10 MB` belum tercapai. Lihat [[04 - Changelog & Dev Log]] (ADR-026).
- [x] **Launch at Startup (v0.1.8):** Toggle autostart OS di Settings → System → Startup, diterapkan hanya saat nilai berubah (Windows HKCU Run / Linux XDG autostart), tanpa Administrator/root
- [x] **Idle Battery & Layout Polish v0.1.9:** Mode idle Battery (Linux/Windows), gutter arrow expanded agar konten tidak tertutup, dan tab Home minimalis. `cargo test` 72 passed, clippy bersih. Lihat [[04 - Changelog & Dev Log]] (ADR-027).
- [ ] **Multi-Monitor Picker:** Pilihan monitor target di Settings
- [ ] **System HUD (Volume & Brightness):** Bar indikator custom saat volume atau brightness berubah
- [ ] **Click-to-Focus App:** Klik notifikasi untuk memfokuskan aplikasi pengirimnya
- [ ] **Quick Drop / Clipboard Shelf:** Area drag-and-drop sementara untuk file atau teks antar window
- [ ] **Battery / System Mini Status:** Indikator baterai/charging di pill saat idle

## P2 — Could Have (Future)
- [ ] Timer / Pomodoro cepat di dalam pill
- [ ] Quick Drop / File Shelf sementara
- [ ] Multi-monitor picker & custom theme editor
