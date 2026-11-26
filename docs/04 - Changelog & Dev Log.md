---
type: changelog
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - dev-log
---

# 04 - Changelog & Dev Log

## 2026-11-27

### Added

- `src/platform/{mod,linux,windows}.rs`, `src/ui/mod.rs`: Menambahkan dukungan huruf **Bold** untuk lirik utama dan daftar lirik lengkap dengan efek blur di mode expanded (**v0.1.3**):
  - Menambahkan keluarga font khusus `LyricsBold` (`apply_configured_fonts`, `platform::bold_fallback_fonts`) dengan penemuan otomatis varian Bold font sistem (`NotoSans-Bold.ttf`/`DejaVuSans-Bold.ttf`/`fc-match sans-serif:bold` di Linux; `segoeuib.ttf`/`arialbd.ttf` + `malgunbd.ttf` di Windows) serta fallback CJK `FontFamilyTarget::All` (`Bold` + `Proportional` + `Monospace`) agar lirik utama di pill maupun kartu Music selalu tampil tebal lintas aksara.
  - Memperbarui kolom kanan `render_music_tab` di mode **Expanded** agar otomatis memicu fetch lirik lagu berjalan (`ensure_lyrics_requested_for_media`), menampilkan **seluruh baris lirik** (`LyricsData::Synced` dan `LyricsData::Plain`) dalam `ScrollArea`, melakukan auto-scroll halus baris aktif ke tengah viewport, serta merender baris **sebelum dan sesudah** baris aktif dengan blur isotropik multi-pass + peluruhan opasitas berbasis jarak (`render_blurred_aligned_text`) yang identik pada `SoftCanvas` (Windows/Wayland) dan OpenGL.

### Fixed

- `src/ui/mod.rs`, `src/battery.rs`, `src/sysmon.rs`, `src/ui/taskbar.rs`, `src/platform/mod.rs`: Memperbaiki tampilan **Idle Clock** dan perataan **`TextAlign::Center`** pada Linux/Windows (**v0.1.3**):
  - Mengganti kalkulasi lebar jam tetap di `target_size` dengan pengukuran lebar aktual `LayoutJob` (`render_island_ui` + estimator awal `estimate_clock_pill_width` dengan digit stabil `stabilized_clock_measure_text`), sehingga lebar pill otomatis menyesuaikan format `12h`/`24h`, detik, dan zona waktu, sementara konten `[dot + jam + tz]` dipusatkan presisi via `render_dot_label(..., Align::Center)` dan monitor width `eframe` membaca `viewport().monitor_size`.
  - Memperbaiki penerapan **`TextAlign::Center`** (serta `Left`/`Right`) di pill mode (`IdleDisplayMode::Lyrics` karaoke & fallback non-karaoke + terjemahan, `NextEvent`, `Minimal`, `render_compact` badge+judul+body) dan mode expanded (`render_expanded` notifikasi multi-baris, `render_overview_tab`, `render_todo_tab`, `render_calendar_tab`, `render_music_tab`, dan `render_wrapped_karaoke` via `LayoutJob::halign` + `galley_draw_pos` beserta 3 unit test baru `clock_width_scales_automatically_and_stabilizes_digits`, `galley_draw_pos_centers_single_and_wrapped_lines_in_column`, dan `apply_configured_fonts_registers_bold_lyrics_family_and_line_resolution`).
  - Memperbaiki warning `clippy` Linux (`question_mark`, `UiControlEvent::ConfigUpdated(Box<Config>)`, dan `#[cfg_attr(not(windows), allow(dead_code))]` pada kode khusus Windows).

### Changed

- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `src/ui/mod.rs`, `README.md`: Menaikkan versi rilis menjadi **`0.1.3`** dan membangun ulang paket Linux (`AppImage` & `.tar.gz`). Lihat [[00 - Dashboard Proyek]], [[01 - Arsitektur & Spesifikasi]], dan [[03 - Catatan Rapat & Keputusan]] (ADR-021).

## 2026-11-26

### Fixed

- `src/platform/linux.rs`, `src/ui/mod.rs`: Memperbaiki bug album cover yang gagal ter-fetch atau gagal tampil pada pemutar musik Linux (**v0.1.2**):
  - Memperbaiki kunci cache album (`ArtCacheState` di `src/platform/linux.rs`) yang sebelumnya mengunci `(title, artist)` walau `mpris:artUrl` masih kosong di poll awal; kini perubahan `art_url` (`None -> Some` atau URL baru) langsung diproses dan kegagalan sementara di-retry otomatis (backoff 2 s / 4 s, hingga 3 percobaan) memakai satu `ureq::Agent` bersama (timeout 6 s).
  - Menambahkan dekoder URI `data:image/...;base64,...` (`decode_data_image_uri`, `decode_base64`) untuk cover inline dari Chromium/Firefox/Electron, dukungan `file://localhost/...` dan path lokal polos `/...` (plus fallback strip `?`/`#`), serta fallback URL Spotify `open.spotify.com/image/*` → `i.scdn.co/image/*` dan header `User-Agent`/`Accept` gambar (+1 unit test di `src/platform/linux.rs`).
  - Memperbaiki `IslandApp::sync_album_art_texture` di `src/ui/mod.rs` agar perubahan `album_art` (`None -> Some` atau `Arc` baru) untuk lagu yang sama selalu memperbarui tekstur UI alih-alih terkunci oleh cache `(artist, title)`.

### Changed

- `AGENTS.md`, `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `src/ui/mod.rs`, `README.md`: Menambahkan fitur decoder `bmp` + `gif` pada crate `image`, mencatat preferensi alur rilis pengguna di `AGENTS.md` (auto bump versi, AppImage/installer, sync docs, auto-push branch aktif), serta menaikkan versi ke **`0.1.2`**. Lihat [[00 - Dashboard Proyek]], [[01 - Arsitektur & Spesifikasi]], dan [[03 - Catatan Rapat & Keputusan]].

### Fixed

- `src/platform/linux.rs`, `src/lyrics.rs`, `src/daemon.rs`, `src/ui/mod.rs`: Memperbaiki bug lirik gagal di-fetch dan gagal tersinkronisasi dengan pemutar musik di Linux (**v0.1.1**):
  - **Pemilihan Player & Parsing Metadata MPRIS2 (`src/platform/linux.rs`):** Memilih bus `org.mpris.MediaPlayer2.*` yang benar-benar sedang `Playing` (atau `Paused` ber-judul, dengan preferensi non-browser / `playerctld` dibanding tab browser menganggur), membaca `mpris:length` dan `Position` untuk varian `i64`/`u64`/`i32`/`u32`/`f64`, mendukung `xesam:artist` bertipe `String` maupun `Array` (plus fallback `xesam:albumArtist`/`xesam:creator` & stem `xesam:url`), serta men-decode percent-encoding `%XX` pada `mpris:artUrl` (`file://`).
  - **Sinkronisasi Timeline & Seeking (`src/platform/linux.rs`, `src/ui/mod.rs`):** Memanggil `org.freedesktop.DBus.Properties.Get(..., "Position")` secara eksplisit setiap `350 ms` saat playback, mencatat `updated_at` sebelum fetch `album_art` HTTP (dan mengambil ulang `Position` bila download cover memakan waktu `> 40 ms`), memakai tracker drift `evaluate_mpris_sync` dengan konfirmasi 2-poll untuk mencegah lompatan mundur pada player step 1 detik sekaligus mengoreksi seek maju/mundur/pause secara langsung, serta memakai `SetPosition(mpris:trackid, target_us)` (fallback `Seek` memakai posisi live terbaru). Posisi interpolasi di `current_playback_ms` turut di-clamp ke `duration_ms`.
  - **Fetch & Ranking LRCLIB (`src/lyrics.rs`, `src/daemon.rs`, `src/ui/mod.rs`):** Menambahkan normalisasi `strip_bracket_noise`/`clean_artist_name`/`primary_artist`/`split_artist_and_title` (kasus YouTube `- Topic`, tag `(Official Video)`/`(feat. ...)`/`| ...`, & multi-artis), mengutamakan hasil `synced_lyrics` bahkan ketika durasi awal `0`, menaikkan timeout HTTP ke `10 s`, memicu ulang request otomatis saat `duration_ms` baru terisi dari `0`, serta menambah tombol **Retry Fetch** pada tab Music saat status `NotFound`.

### Changed

- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `src/ui/mod.rs`, `README.md`: Menaikkan versi paket & metadata rilis dari `0.1.0` ke **`0.1.1`** dan memperbarui bundel Linux `Liebert_Dynamic_Island-0.1.1-x86_64.AppImage`. Lihat [[00 - Dashboard Proyek]], [[01 - Arsitektur & Spesifikasi]], dan [[03 - Catatan Rapat & Keputusan]].

### Added

- `src/ui/soft_render.rs`, `src/ui/mod.rs`: Menambahkan rasterizer CPU pure-Rust (`SoftCanvas`, dengan 4 unit test untuk blending premultiplied alpha BGRA/RGBA, aturan diagonal top-left anti-seam, clip-rect HiDPI, dan delta tekstur parsial/free) untuk menghilangkan kebutuhan inisialisasi OpenGL/EGL/WGL dari proses overlay utama. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-018).
- `src/platform/windows_overlay.rs`, `src/platform/mod.rs`, `src/main.rs`: Menambahkan backend overlay native Win32 berbasis `UpdateLayeredWindow` + DIB section top-down 32-bit BGRA (`try_run_layered_direct` & `run_layered_ipc_client`) yang bekerja langsung tanpa `eframe`/OpenGL GPU driver, sementara jendela Settings (`--settings --ipc-addr`) diluncurkan on-demand sebagai child process terpisah yang membebaskan seluruh memorinya saat ditutup. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-018).

### Fixed

- `src/state.rs`, `src/lyrics.rs`, `src/platform/linux.rs`, `src/daemon.rs`, `src/main.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Memperbaiki seluruh error kompilasi & regresi akibat merge PR #15 (`0c9daa4`):
  - Memindahkan atribut `#[derive(...)]` kembali ke `NotificationItem` dan `LyricWord`; menyatukan batas antrean `MAX_QUEUED_NOTIFICATIONS` serta memulihkan `BoundedCache` (kapasitas 8/8 tanpa cache `NotFound`).
  - Memulihkan `TrayController::new(auto_hide, notch_dock_mode)`, `TrayAction::ToggleDockMode`, `sync_tray`, tanda tangan `is_cursor_in_top_trigger`, dan `detect_fullscreen_state` (`(exclusive, windowed)`).
  - Menyatukan `installed_fonts` (`Option<Vec<InstalledFont>>`) dan `applied_font_name` pada `StandaloneSettingsApp`, mengembalikan `TextAlign`, `PillLyricTransition`, `allow_close` pada `IslandApp::ui` (mencegah deadlock `CancelClose` saat `--ui` menutup diri), serta menghapus kode duplikat hasil merge di `src/ui/mod.rs` & `src/main.rs`.
- `src/platform/wayland_layer.rs`, `src/platform/linux.rs`: Memperbaiki crash dan bug Dynamic Island menghilang pada **Linux Hyprland** sementara ikon tray masih hidup:
  - Mengganti `glutin`/`glow`/`egui_glow` dengan `smithay-client-toolkit` `wl_shm` (`SlotPool` `Argb8888`) + `SoftCanvas`, sehingga kegagalan EGL/NVIDIA `egl-wayland` dan error `wl_surface::invalid_size` akibat commit `set_buffer_scale` sebelum buffer baru hilang total.
  - Menambahkan pemulihan `LayerSurface` otomatis (`WaylandState::create_layer_surface` dengan backoff dan `new_output` wake) ketika compositor mengirim `ZwlrLayerSurfaceV1::Closed` (mis. DPMS sleep, monitor hotplug, atau `hyprctl reload`) alih-alih jatuh ke fallback `eframe` `xdg_toplevel`.
  - Memasang `platform::spawn_fullscreen_watcher` serta `ui::apply_configured_fonts` agar fitur auto-hide fullscreen dan font pilihan ikut berjalan penuh di Wayland.

### Changed

- `Cargo.toml`, `Cargo.lock`, `src/platform/{linux,mod,windows}.rs`, `src/ui/{mod,settings}.rs`: Mengoptimalkan penggunaan RAM & D-Bus di Linux maupun Windows:
  - Menghapus dependensi langsung Linux `glutin`, `egui_glow`, `glow`, dan `raw-window-handle`; binary `--release` (`17 MB`) kini bebas link langsung `libGL`/`libEGL` (`ldd` hanya `libc`/`libm`/`libgcc_s`).
  - Memangkas font eager Linux ke sans + mono (`linux_sans`, `linux_mono`), memuat tepat satu face TTC per `CjkScript` (`[0]` untuk Han/JP, `[1]` untuk Korea), serta meniadakan muatan eager `seguisym.ttf` di Windows dan melepaskan `album_art_tex`/`gradient_tex`/`installed_fonts` ketika musik berhenti / Settings ditutup.
  - Menggunakan ulang proxy D-Bus MPRIS dengan `zbus::proxy::Builder` + `CacheProperties::No` di `spawn_media_listener` dan memanjangkan jeda poll saat pause/tanpa player (`900 ms`/`1200 ms`).
  - Terverifikasi dengan `cargo test` (`48 passed; 0 failed`), `cargo clippy --all-targets -- -D warnings`, headless daemon Linux (`VmRSS: 11.2 MB`, `Pss: 8.5 MB`, `RssAnon: 2.0 MB`), dan packaging Linux (`dist/Liebert_Dynamic_Island-0.1.0-x86_64.AppImage` `7.0 MB` + `.tar.gz` `6.7 MB`). Lihat [[00 - Dashboard Proyek]], [[01 - Arsitektur & Spesifikasi]], dan [[03 - Catatan Rapat & Keputusan]].

### Changed

- `src/main.rs`, `src/daemon.rs`, `src/ipc.rs`, `src/ui/mod.rs`, `src/platform/{mod,windows}.rs`, `src/platform/wayland_layer.rs`, `src/state.rs`, `src/lyrics.rs`, `src/calendar.rs`, `src/pomodoro.rs`: Merefaktor arsitektur menjadi **Daemon + UI On-Demand** — proses utama kini berjalan sebagai daemon headless ringan (`src/daemon.rs`, hanya `TrayController`, listener OS, worker lirik/kalender, `PomodoroTimer`, dan server IPC TCP loopback `127.0.0.1:0` di `src/ipc.rs`) tanpa memuat `eframe`/OpenGL saat idle/tersembunyi. Proses `--ui --ipc-addr <addr>` hanya di-spawn saat dibutuhkan (notifikasi masuk, hover area atas layar, menu tray Settings/Test, atau `auto_hide_when_idle = false`), dan otomatis keluar setelah 1,5 detik tersembunyi untuk membebaskan RAM GPU/OpenGL sepenuhnya.
- `README.md`, `assets/*`: Memperbarui dokumentasi fitur dan menyematkan seluruh galeri preview UI terbaru (`ui-notch-clock.png`, `ui-island-clock.png`, `ui-notch-music.png`, `ui-island-music.png`, `ui-home.jpg` [diperbarui], `ui-music-lyrics.jpg`, `ui-todo.jpg`, `ui-calendar.jpg`, `ui-timer.jpg`, `ui-settings.png`).
- `src/ui/mod.rs`, `src/ui/settings.rs`: Mengoptimalkan frame pacing dan alokasi memori UI — menaikkan refresh karaoke lirik ke `16 ms` (~60 FPS) dan visualizer idle ke `33 ms` (~30 FPS) saat terlihat, menurunkan repaint saat overlay tersembunyi ke `250 ms`, serta menghindari kloning ulang `installed_fonts` dan `calendar_events` setiap frame pada `DeferredSettingsState`.
### Fixed
- `src/ui/settings.rs`, `src/platform/mod.rs`: Memperbaiki kegagalan `clippy` khusus target Windows yang menggagalkan job CI `Build Windows (zip + NSIS installer)`: import `PathBuf` untuk `run_standalone_settings` kini hanya dikompilasi di Linux, varian `FontFamilyTarget` dan field `HoverPollInput::animating` yang memang tidak dipakai di Windows diberi `#[cfg_attr(windows, allow(dead_code))]`, dan `apply_overlay_window_setup` memakai blok ekspresi `#[cfg]` tanpa `return` yang berlebih. `cargo test` (41 passed), `cargo clippy --all-targets -- -D warnings`, serta cross-check `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings` semuanya bersih. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].

### Added
- `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Menambahkan preferensi **`TextAlign`** (`Left` / `Center` / `Right`, default `Left`) dengan tombol pilihan **Text Alignment** di tab **Appearance** Settings. Alignment diterapkan pada lirik idle/kartu musik (termasuk karaoke, romaji, dan terjemahan yang digambar manual), judul/body notifikasi compact & expanded, judul/artis lagu, clock/minimal dot, serta daftar todo/kalendar. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].
- `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `installer/windows/installer.nsi`, `assets/`, `scripts/build-linux-release.sh`: Menambahkan paket AUR (build dari tag `v0.1.0`, pasang binary + `.desktop` + ikon), installer NSIS Windows (shortcut Start Menu & uninstaller), aset desktop/icon bersama, dan skrip kemasan Linux yang menghasilkan `.tar.gz` alongside AppImage. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-017).

### Fixed
- `src/ui/mod.rs`, `src/platform/wayland_layer.rs`: Memperbaiki bug Linux yang membuat aplikasi terasa selalu **force-close**:
  - Root viewport kini membatalkan `close_requested()` dari window manager (`ViewportCommand::CancelClose`); keluar hanya lewat menu **Exit** di tray (`TrayAction::Exit` → `ViewportCommand::Close` di mode eframe, `want_exit` di layer-shell).
  - Saat auto-hide/fullscreen di Linux, jendela tidak lagi disembunyikan (`Visible(false)`) melainkan dikecilkan ke `1×1` seperti layer-shell, sehingga proses tetap hidup dan bisa muncul kembali lewat hover.
  - Compositor yang menutup `wl_surface` layer-shell kini memicu **fallback eframe** (`Err` → `main.rs`) alih-alih dianggap sukses dan menghentikan proses. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-016).
- `src/platform/wayland_layer.rs`: Mengatasi kebocoran tekstur GPU dengan selalu meneruskan `textures_delta` ke `painter.paint_and_update_textures` walau pill sedang tersembunyi (sebelumnya delta `free` dibuang, sehingga tekstur GL tidak pernah dilepaskan sampai proses mati). Loop Wayland juga kini melewati tessellation/GL saat tidak ada redraw/animasi.
- `src/state.rs`, `src/lyrics.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Membatasi pertumbuhan RAM jangka panjang: antrean notifikasi dibatasi 32 item (FIFO, `MAX_QUEUED_NOTIFICATIONS`), cache lirik mentah/enriched memakai `BoundedCache` (32/64 entri, kunci tanpa `duration_secs` yang sering berubah), key alert kalender dipangkas ke 1 hari, gradient texture cache dikuantisasi, dan pemindaian font hanya dilakukan sekali (sentinel `Option`) agar tidak mengulang walk direktori tiap frame.

### Changed
- `docs/02 - Daftar Tugas & Backlog.md`, `docs/00 - Dashboard Proyek.md`, `docs/01 - Arsitektur & Spesifikasi.md`, `docs/03 - Catatan Rapat & Keputusan.md`: Mencatat fitur alignment, perbaikan daur hidup Linux, dan pembatasan cache. `cargo test` 41 passed & `cargo clippy --all-targets -- -D warnings` bersih.
- `.github/workflows/release.yml`, `scripts/build-appimage.sh`, `README.md`: Job Linux kini memakai `build-linux-release.sh` (AppImage + tar.gz), job Windows memasang NSIS dan menghasilkan `*-setup.exe` di samping `.zip`, serta release mencakup bundel AUR. Tabel instalasi prebuilt dan opsi `text_align` ditambahkan ke README. AppImage & tar.gz dibangun dan diverifikasi lokal; installer Windows dibangun oleh CI.

### Added
- `LICENSE.md`, `Cargo.toml`: Menambahkan berkas lisensi resmi **GNU General Public License v3.0** (`GPL-3.0-or-later`, selaras dengan dependensi `kakasi 0.1`).
- `src/lyrics.rs`, `src/ui/mod.rs`: Menambahkan animasi transisi **fade-up / fade-down** (`240 ms`, cubic ease-out, terklip di dalam baris pill) saat berganti baris lirik dan terjemahan di pill mode (`IdleDisplayMode::Lyrics`: naik ke atas saat maju ke baris berikutnya dan turun ke bawah saat mundur/seek ke baris sebelumnya) beserta unit test `lyric_transition_fades_up_and_down_and_finishes`.
- `.github/workflows/release.yml`: Menambahkan workflow CI/CD GitHub Actions untuk membangun dan menerbitkan release Windows (`x86_64-pc-windows-msvc` `.zip`) dan Linux (`AppImage` + `.tar.gz`) saat push tag `v*` atau `workflow_dispatch`.

### Fixed
- `src/main.rs`, `src/tray.rs`, `src/platform/mod.rs`, `src/platform/linux.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`, `src/ui/icons.rs`: Memperbaiki seluruh error kompilasi, kurung kurawal hilang, dan duplikasi blok hasil merge PR (`#10` & `#12`):
  - `src/platform/mod.rs`, `src/platform/linux.rs`: Menutup blok `pin_window_top_y` & `list_installed_fonts`, memisahkan `detect_overlay_hover` & `is_cursor_in_top_trigger` dari `is_cursor_over_window`, menghapus referensi `linux::position_overlay_window` yang tidak ada, serta menambahkan `spawn_fullscreen_watcher` di Linux (Hyprland IPC).
  - `src/main.rs`, `src/tray.rs`: Memulihkan `TrayAction::ToggleDockMode` & `update_dock_mode` di `TrayController`, menyatukan signature `TrayController::new`, membersihkan channel duplikat di `main()`, serta memilih kanvas permanen di Windows (`permanent_canvas_size`) dan ukuran dinamis di Linux.
  - `src/ui/mod.rs`, `src/ui/icons.rs`: Menutup `apply_configured_fonts`, membersihkan duplikasi kode di `step_logic`, `render_island_ui`, `render_idle`, `render_expanded`, `render_overview_tab`, `render_music_tab`, `render_todo_tab`, `paint_island_surface`, `render_pill_tab`, dan `render_playbar`, menambahkan `IconKind::Timer`, serta menjaga `SETTINGS_WINDOW_TITLE` tersedia di Windows dan Linux.
  - `src/ui/settings.rs`: Menghapus blok tab duplikat tak tertutup di `render_settings_viewport`, meneruskan `installed_fonts` & `font_search` ke `SettingsBodyState` agar pemilih font juga berfungsi di `run_standalone_settings`, serta meneruskan `self.config.accent_color` ke `apply_settings_theme`. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].

### Changed
- `src/ui/mod.rs`, `src/platform/wayland_layer.rs`: Mengganti interpolasi ukuran linier-eksponensial dengan fisika pegas harmonik teredam ala Dynamic Island macOS (`step_spring`, 4 sub-langkah per frame, kekakuan/redaman X & Y sedikit bertahap untuk efek elastis saat expand/collapse/reveal serta `notch_morph`, clip rect konten saat pantulan, dan interval repaint `6 ms` pada `wlr-layer-shell`). Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].

### Added

- `src/ui/mod.rs`, `src/platform/windows_overlay.rs`: Menambahkan tab **Sys** (`IslandTab::System`) dan ringkasan CPU/RAM/Battery di tab **Home** pada panel Dynamic Island, memindahkan sinkronisasi `SysmonCommand::SetConfig` ke `sync_runtime_config` agar perubahan dari Settings langsung aktif, serta menjaga overlay taskbar tetap `HWND_TOPMOST`.
- `src/battery.rs`, `src/sysmon.rs`, `src/ui/taskbar.rs`, `src/config.rs`, `src/ui/settings.rs`, `src/platform/windows.rs`, `src/platform/windows_overlay.rs`, `src/platform/mod.rs`, `src/ui/mod.rs`, `src/main.rs`: Menambahkan **Taskbar System Monitor** (CPU via `GetSystemTimes`, RAM via `GlobalMemoryStatusEx`, Disk via `GetDiskFreeSpaceExW` + PDH, Network via `GetIfTable2`) dan **Laptop Battery Support** (`GetSystemPowerStatus` dengan deteksi desktop tanpa baterai, 4 status pengisian, estimasi sisa waktu, serta notifikasi low-battery) lengkap dengan tab **System** di Settings.
- `src/ui/mod.rs`: Menambahkan animasi transisi `visibility_anim` (slide vertikal + fade opacity halus) saat fitur **Auto-hide island when idle** menyembunyikan atau memunculkan kembali pill/notch.
- `src/config.rs`, `src/platform/windows.rs`, `src/platform/mod.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Menambahkan opsi **`hide_in_windowed_fullscreen`** (_"Also hide on windowed fullscreen / maximized apps"_) di Settings di bawah `Hide when a fullscreen app/game is active`, dengan klasifikasi `detect_fullscreen_state` (`exclusive` vs `windowed`) melalui `PlatformEvent::Fullscreen(bool, bool)`.
- `Cargo.toml`: Menambahkan blok `[package.metadata.packager]` (`product-name`, `identifier`, `version`, `out-dir`, `formats = ["nsis", "wix"]`, `resources = ["README.md"]`) untuk packaging installer Windows `.exe` (NSIS) dan `.msi` (WiX) dengan uninstaller bawaan.
- `installer/windows/liebert-dynamic-island.iss`, `installer/windows/install.ps1`, `installer/windows/build-setup.ps1`: Menambahkan skrip setup Windows per-user (`%LOCALAPPDATA%\Programs\Liebert Dynamic Island`, tanpa hak Administrator) berupa konfigurasi Inno Setup 6 (`.iss`), builder `build-setup.ps1`, dan installer/uninstaller PowerShell mandiri `install.ps1` (shortcut Start Menu, opsi Desktop & Autostart `HKCU\...\Run`).
- `src/pomodoro.rs`, `src/config.rs`, `src/main.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Menambahkan fitur **Pomodoro Timer** (`Focus` / `Short Break` / `Long Break`, siklus 4 sesi, countdown di pill idle `IdleDisplayMode::Pomodoro` & otomatis saat `Auto` berjalan, tab `⏱ Timer`, ringkasan di `Overview`, notifikasi selesai fase, serta slider durasi di Settings) beserta 2 unit test di `src/pomodoro.rs`.
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`, `src/config.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Menambahkan pilihan **Font** di Settings Appearance (`font_name`) yang memindai font terinstal pada perangkat (`list_installed_fonts`, filter pencarian, dan `apply_configured_fonts`).
- `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Menambahkan opsi **`KaraokeLyricMode::Translate`** di Settings sehingga lirik terjemahan dapat dipilih sebagai teks karaoke utama pada idle notch maupun Music Card.

### Fixed

- `src/calendar.rs`: Menambahkan ekspansi event berulang (`RRULE` untuk `DAILY`, `WEEKLY` + `BYDAY`, `MONTHLY`, `YEARLY`, `INTERVAL`, `COUNT`, `UNTIL`, `EXDATE`, `RECURRENCE-ID`, serta dukungan `TZID`) pada `parse_ics_in_range` sehingga jadwal mingguan berulang dari Google Calendar ikut tampil, dan membatasi `upcoming_events` ke **minggu berjalan ini saja** (`current_week_end_utc`).
- `Cargo.toml`, `src/ui/settings.rs`: Mengaktifkan fitur `"clipboard"` pada `eframe` (agar `Ctrl+C`/`Ctrl+V`/`Ctrl+X` di `egui::TextEdit` terhubung ke clipboard OS) serta menambahkan tombol **Copy** pada tab **Google Calendar** di Settings.
- `src/ui/settings.rs`, `src/ui/mod.rs`: Memperbaiki panic `egui` (`egui backend is implemented incorrectly - the user callback was never called`) dan Dynamic Island yang tidak muncul kembali setelah masuk windowed-fullscreen/auto-hide — mengganti `show_viewport_immediate` Settings menjadi `show_viewport_deferred` dengan state bersama `Arc<Mutex<...>>`, mengeluarkan pemanggilan `render_settings_viewport` dari early-return `vis <= 0.005`, dan menjaga jendela `eframe` tetap aktif (`SW_SHOWNOACTIVATE` + transparan & click-through `WS_EX_TRANSPARENT`) alih-alih `SW_HIDE` yang menekan `WM_PAINT`/`RedrawRequested`.
- `src/platform/windows.rs`: Memperbaiki `is_foreground_fullscreen` dan `is_rect_covering_monitor_as_fullscreen` agar jendela **windowed-fullscreen / maximized** (`IsZoomed(hwnd)` atau menutupi penuh `rcWork`/`rcMonitor`, termasuk browser/aplikasi saat taskbar auto-hide) ikut menyembunyikan Dynamic Island ketika `hide_in_fullscreen` aktif.
- `src/ui/mod.rs`: Memperbaiki layout baris lirik idle dari `Layout::top_down` ke `Layout::left_to_right(Align::Center)` agar teks lirik/judul lagu tampil sejajar di samping ikon `♪` dan tidak terpotong di bawah clip-rect.
- `src/ui/mod.rs`: Menghapus blok `paint_island_surface` duplikat pada `render_island_ui` yang sebelumnya menggambar kotak latar penuh di atas kanvas permanen transparan di belakang notch/pill.
- `src/platform/mod.rs`, `src/main.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`, `src/ui/icons.rs`: Merapikan seluruh sisa konflik merge pasca-PR #11 (`pin_window_top_y`, `detect_overlay_hover`, `is_cursor_in_top_trigger`, `apply_configured_fonts`, `paint_island_surface`, serta `render_settings_viewport`) sehingga kompilasi, 32 unit test, dan Clippy bersih kembali.
- `src/ui/mod.rs`, `src/ui/settings.rs`: Memperbaiki pengaturan **Accent** & **Colors** agar `config.bg_color` diterapkan langsung pada state `Idle` maupun `Music Card` (menghapus warna hardcode pada mode notch), memisahkan **Background RGB** (`color_edit_button_srgb`) dengan slider **Background Opacity** agar tidak terdampak premultiplied alpha `Color32`, menyinkronkan tema Settings (`apply_settings_theme` & tombol `Display Mode`) dengan `config.accent_color`, serta menambahkan catatan ketika `dynamic_album_color` aktif saat musik diputar.
- `src/pomodoro.rs`, `src/ui/mod.rs`: Memperbaiki tombol **Reset** Pomodoro agar mengembalikan fase ke `Focus` dengan `completed_focus = 0`, memicu `request_repaint()` langsung saat diklik, serta meratakan posisi baris tombol `Start`/`Reset`/`Skip` tepat di tengah (`allocate_ui_with_layout`).
- `src/ui/mod.rs`: Menambahkan `seam_overlap = 0.75 px` dan meningkatkan segmen kurva pada telinga cekung kiri/kanan di `paint_island_surface` untuk menghilangkan celah subpixel tipis antara telinga notch dan badan pill.
- `src/platform/windows.rs`, `src/state.rs`: Memperbaiki deteksi aplikasi **windowed-fullscreen** & **fullscreen** (`covers_autohide_edge` di `is_rect_covering_monitor_as_fullscreen` untuk aplikasi fullscreen ber-`WS_CAPTION` pada auto-hide taskbar), serta me-reset `idle_since` pada `IslandState::set_fullscreen_active` agar saat keluar fullscreen notch/island muncul mengikuti `auto_hide_delay_secs` di Settings (+1 unit test baru di `src/state.rs`).
- `src/ui/mod.rs`, `src/main.rs`, `src/platform/windows.rs`, `src/platform/mod.rs`: Menghilangkan flicker saat membuka/menutup **Floating Island** dan **Top Notch** dengan kanvas permanen tetap (`permanent_canvas_size` pada `y = 0` tanpa `ViewportCommand::InnerSize`/`OuterPosition` saat animasi) serta toggle click-through Win32 dinamis (`set_overlay_click_through` yang mengatur `WS_EX_LAYERED | WS_EX_TRANSPARENT` di luar area pill).
- `src/platform/windows.rs`, `src/platform/mod.rs`, `src/ui/mod.rs`: Memperbaiki auto-hide fullscreen agar tidak menghapus style `WS_EX_NOACTIVATE`/`WS_POPUP` dan tidak mencuri fokus foreground (`set_overlay_visible` memanggil `ShowWindow(SW_HIDE / SW_SHOWNOACTIVATE)` langsung, menggantikan `ViewportCommand::Visible`), serta menambahkan `DWMWA_EXTENDED_FRAME_BOUNDS` dan ambang batas taskbar `>= 20 px` (`is_rect_covering_monitor_as_fullscreen`) agar jendela maximized dengan auto-hide taskbar Windows 11 tidak salah terdeteksi sebagai fullscreen.
- `src/ui/settings.rs`, `src/ui/mod.rs`: Memperbaiki perataan UI dan alur pengetesan: menyamakan lebar penuh tombol `Display Mode` di Settings, memperpendek label checkbox notifikasi agar tombol `"Send Test Notification"` tidak terpotong di lebar jendela `450 px`, serta auto-unpause di `push_test_notification` agar tes popup selalu tampil.

### Changed

- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/ui/mod.rs`, `src/lyrics.rs`, `src/calendar.rs`, `src/state.rs`, `src/main.rs`, `Cargo.toml`: Optimasi RAM & performa — pemuatan font CJK per-script (`CjkScript::JapaneseOrHan` vs `Korean`, hemat ~10–13 MB saat lagu JP/KR), `LyricsData` berbasis `Arc<[T]>` + cache FIFO terikat `8` entri tanpa cache `NotFound` + batas HTTP `512 KiB`, batas decode `AlbumArt` (`2048×2048` / `16 MiB` + `into_rgba8()`), batas ICS `2 MiB` + retensi `[-1d..+90d]` + `upcoming_events` tanpa kloning, batas antrean notifikasi `32` item & `body <= 2000` byte, `panic = "abort"` di `[profile.release]`, serta perbaikan argumen `TrayController::new` pada cabang Wayland Linux.
- `README.md`: Menyusun ulang dokumentasi lengkap proyek menggunakan skill `readme` (Table of Contents, Tech Stack, Prerequisites, Getting Started, Architecture & Data Flow, tabel lengkap `config.toml`, Commands, Testing, Packaging, dan Troubleshooting) sambil mempertahankan preview gambar UI & Settings.
- `README.md`: Menambahkan bagian preview gambar **UI** dan **Settings** di bagian atas README.
- `README.md`: Memperbarui tabel konfigurasi (`hide_in_windowed_fullscreen`), struktur modul (`src/ui/icons.rs`), profil release (`codegen-units = 1`), dan instruksi packaging installer Windows via `cargo-packager`.
- `Cargo.toml`, `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`, `src/ui/mod.rs`: Mengoptimalkan alokasi heap & hot-path rendering dengan berbagi buffer `AlbumArt` via `Arc<AlbumArt>` (zero-copy 102 KB antar thread & frame), menghapus `self.media.clone()` / `self.state.active.clone()` / `ui.painter().clone()` per-frame di `render_idle`, `render_expanded`, `render_music_tab`, dan `render_centered_truncated_text`, serta menambahkan `codegen-units = 1` pada `[profile.release]`.
- `README.md`: Memperbarui dokumentasi utama proyek agar mencerminkan arsitektur, dependensi, fitur (Notch/Floating Island, Karaoke/Romaji/Translate, Pomodoro, Font Picker, Todo, Google Calendar), konfigurasi `config.toml`, CLI flags, dan perintah test/lint aktual.
- `src/ui/mod.rs`: Mengganti ikon karakter dengan ikon vektor geometris presisi (`paint_vector_icon` untuk `⚙`, `▶`, `⏸`, `⏮`, `⏭`), menyamakan tinggi tombol/tab header (`24 * scale`), menambahkan `render_dot_label` (dot geometris + centerline teks) untuk mode idle Clock/Minimal, meratakan kotak `♪` pada mode idle Lyrics, serta memusatkan dan memangkas judul/artis Music Card dengan `render_centered_truncated_text` / `truncate_with_ellipsis` (+2 unit test baru di `src/ui/mod.rs` & `src/platform/windows.rs`).
- `src/ui/settings.rs`: Memindahkan dan mempertegas tombol pilihan **Display Mode** (**Top Notch (Seamless Top)** vs **Floating Dynamic Island**) + `Top Offset (px)` ke paling atas tab Appearance Settings dengan frame aksen aktif agar langsung terlihat dan mudah diakses.

### Fixed

- `src/platform/windows.rs`: Menambahkan Win32 subclass `overlay_wndproc` (`GWLP_WNDPROC`) yang memotong `WM_NCCALCSIZE` untuk menghilangkan garis tipis `1 px` di atas window borderless `winit`.
- `Cargo.toml`, `src/platform/windows.rs`, `src/platform/mod.rs`, `src/main.rs`, `src/ui/mod.rs`: Mengaktifkan `Win32_UI_Shell` (`SHQueryUserNotificationState`), `GetAncestor(GA_ROOT)`, serta background thread `spawn_fullscreen_watcher` (`250 ms` + `PlatformEvent::Fullscreen` + `on_wake()`) agar auto-hide/restore saat fullscreen selalu bereaksi meski overlay sedang `Visible(false)`.

### Changed (Earlier)

- `src/tray.rs`, `src/main.rs`, `src/ui/mod.rs`: Membatasi pergantian mode tampilan (**Top Notch** ↔ **Floating Island**) hanya melalui menu **Settings** (`Display Mode`), menghapus tombol switch mode di header expanded island dan menu system tray.
- `src/ui/mod.rs`: Merapikan seluruh perataan ikon & elemen antarmuka dengan helper `render_icon_button` (ikon terpusat secara matematis pada `Align2::CENTER_CENTER` untuk `⚙`, kontrol transport musik `🔀`/`⏮`/`▶`/`⏭`/`🔁`, & tombol `×`), `LayoutJob` terpusat untuk clock/minimal idle notch, vertical centering di `render_karaoke_line`, kolom ikon tetap `render_overview_summary_row`, timestamp simetris `render_playbar`, serta baris seragam pada tab Todo dan Calendar.

### Fixed

- `src/state.rs`, `src/platform/windows.rs`, `src/platform/mod.rs`, `src/ui/mod.rs`: Memperbaiki mode `hide_in_fullscreen` agar menyembunyikan island saat `Idle`, namun tetap memunculkan popup notifikasi ke atas layar (`should_be_visible` mengecek `self.active.is_none()`, `raise_overlay_topmost` memanggil `SetWindowPos(HWND_TOPMOST)` saat tampil, serta toleransi `2 px` pada deteksi fullscreen borderless), lalu kembali bersembunyi setelah notifikasi ditutup.

## 2026-11-25

### Fixed

- `Cargo.toml`, `Cargo.lock`: Mengganti `tray-icon` ke `default-features = false, features = ["ksni"]` untuk mengatasi panic startup di Linux/Hyprland (`GTK has not been initialized. Call gtk::init first`) sekaligus menghilangkan ketergantungan runtime GTK3/libappindicator. Lihat [[03 - Catatan Rapat & Keputusan]].
- `src/main.rs`, `src/platform/mod.rs`, `src/platform/linux.rs`, `src/ui/mod.rs`: Menambahkan integrasi Hyprland UNIX IPC socket (`.socket.sock`, mendukung `eval` Lua Hyprland `>= 0.57` dan fallback `keyword`/`dispatch` klasik) untuk `prepare_overlay_rules` (`float`, `pin`, `no_initial_focus`, `no_blur`, `no_shadow`, `border_size=0`, `no_anim`, `move`), sinkronisasi `resize` + `move` via `position_overlay_window`, deteksi fullscreen (`j/activewindow`), serta pelacakan kursor (`is_cursor_over_window`, `is_cursor_in_top_trigger`); sekaligus mengaktifkan `.with_resizable(cfg!(target_os = "linux"))` agar `xdg_toplevel` Wayland tidak mengunci ukuran awal `140×30`. Terverifikasi live di Hyprland 0.57.2 (`Idle [890, 10] 140×30` → `Compact [790, 10] 340×58`, `floating: true`, `pinned: true`, tanpa mencuri fokus). Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].
- `src/platform/linux.rs`: Menyesuaikan listener notifikasi, kontrol MPRIS2 (`Shuffle`/`LoopStatus` memakai `zbus::Variant`), pembacaan `mpris:artUrl` (`dict.get::<&str, &str>`), dan pembaca MPRIS2 (`read_mpris_track`) dengan API `zbus 6.5` (`Builder::session()`, `name()`, dan `serve_at()` mengembalikan `Self`, bukan `Result`; mengganti `OwnedValue`, `Value`, dan `zvariant::Dict` dengan `OwnedVariant`, `Variant`, dan `Dict`). `cargo test` (23 tests), `cargo clippy --all-targets -- -D warnings`, dan `cargo build --release` lolos di Linux. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].
- `src/state.rs`: `ListenerStatus::AccessDenied` ditandai `allow(dead_code)` di luar Windows agar `cargo clippy --all-targets -- -D warnings` bersih (varian hanya diproduksi backend WinRT; kegagalan D-Bus Linux dipetakan ke `Error`). Lihat [[01 - Arsitektur & Spesifikasi]].

### Changed

- `.gitignore`: Menambahkan `dist/` dan `*.AppImage` untuk output kemasan AppImage Linux.
- `docs/00 - Dashboard Proyek.md`, `docs/01 - Arsitektur & Spesifikasi.md`, `docs/02 - Daftar Tugas & Backlog.md`, `docs/03 - Catatan Rapat & Keputusan.md`: Menyelesaikan konflik merge dengan `origin/main` (`ad1cd2d`), mencatat integrasi `image 0.25` + MPRIS2 Shuffle/Loop/Seek/`mpris:artUrl` dan verifikasi build Linux (`zbus 6.5` + `rustc 1.98.1`), serta mempertahankan `ADR-004`–`ADR-007`. Lihat [[04 - Changelog & Dev Log]].

### Added

- `dist/Liebert_Dynamic_Island-0.1.0-x86_64.AppImage` (gitignored): Memaketkan binary release Linux (`x86_64`) menjadi AppImage menggunakan `appimagetool` + `type2-runtime` beserta `.desktop` entry tervalidasi dan ikon `.DirIcon`.

### Added

- `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Menambahkan opsi **`KaraokeLyricMode`** (`Original` vs `Romaji`) di Settings (`Music & Lyrics`) sehingga pengguna bisa memilih Romaji sebagai teks karaoke utama baik di idle notch maupun Music Card expanded.
- `src/config.rs`, `src/tray.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Menambahkan dukungan popup notifikasi kaya warna brand aplikasi (`WhatsApp`, `Discord`, `Gmail`, `Chrome`, `Edge`, `Firefox`, `Telegram`, `Slack`, `Spotify`), toggle `show_notifications` di Settings, serta aksi `Test Notification Popup` (via tombol Settings & menu tray).
- `src/ui/mod.rs`, `src/ui/settings.rs`, `src/tray.rs`: Menambahkan transisi morphing halus **`notch_morph`** (`Top Notch` ↔ `Floating Island`) yang menginterpolasi `top_y`, lebar telinga cekung, corner radius atas/bawah, dan opacity border, dilengkapi tombol ganti mode cepat di header expanded island, segmented selector di Settings, dan menu tray.

### Fixed

- `src/platform/windows.rs`, `src/main.rs`: Menghapus panggilan `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED` (penyebab berkedipnya tombol minimize/maximize/close Win32 saat hover/resize), menghapus `WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX` pada `GWL_STYLE`, serta menonaktifkan tombol caption via `with_minimize_button(false).with_maximize_button(false).with_close_button(false)` di `main.rs`.
- `src/platform/windows.rs`, `src/lyrics.rs`, `src/config.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Memperbaiki sinkronisasi lirik & karaoke Romaji dengan ekstrapolasi `LastUpdatedTime` pada SMTC Windows (`current_filetime_ticks`), pencocokan `duration` di LRCLIB (`/api/get` & sort `/api/search`), pencatatan `end_ms` pada jeda instrumental LRC kosong (`parse_lrc`), `lyric_offset_ms` (`-5000..=5000` ms) di Settings, serta `render_wrapped_karaoke` dual-galley per baris tanpa jeda tengah kata.
- `src/ui/mod.rs`: Menghilangkan tumpang-tindih elemen UI saat transisi expand (`height < 95 * scale`), badge & judul `render_compact`, pemisahan slot kiri/kanan ber-clip pada header notifikasi & dashboard (`Home`, `♪ Music`, `☑ N`, `Cal`), serta membungkus kolom lirik kanan dengan `ScrollArea::vertical()`.
- `src/platform/windows.rs`, `src/platform/mod.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Memperbaiki tombol **Settings** (`⚙`) yang sulit diakses / jendela Settings terbuka di belakang window lain dengan memberi ruang klik dedicated `24×22`, menjaga root viewport tetap visible saat Settings terbuka, serta memanggil `bring_window_to_front` (`HWND_TOPMOST` -> `HWND_NOTOPMOST` + `SetForegroundWindow`).

### Fixed (Earlier)

- `Cargo.toml`, `src/platform/windows.rs`, `src/platform/mod.rs`, `src/ui/mod.rs`: Menghapus garis/border bawaan Windows 11 DWM & celah `1–2 px` di tepi atas monitor dengan fitur `Win32_Graphics_Dwm` (`DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND`, `DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE`), pembersihan `WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME` pada `GWL_STYLE`, serta `pin_window_top_y(..., 0)` + overlap `top_y = rect.top() - 1.0` saat `notch_dock_mode` aktif agar menyatu seamless dengan bezel atas.
- `src/platform/windows.rs`: Memperbaiki listener notifikasi Windows (`UserNotificationListener`) untuk `.exe` unpackaged dengan fallback polling `500 ms` (`recv_timeout`) dan pembacaan `visual.Bindings()` bila `ToastGeneric` kosong, sehingga popup notifikasi dari WhatsApp, Discord, browser, Gmail, dll. selalu tertangkap meski event `NotificationChanged` tidak dipicu oleh WinRT.

### Changed

- `README.md`, `assets/*`: Memperbarui dokumentasi fitur dan menyematkan seluruh galeri preview UI terbaru (`ui-notch-clock.png`, `ui-island-clock.png`, `ui-notch-music.png`, `ui-island-music.png`, `ui-home.jpg` [diperbarui], `ui-music-lyrics.jpg`, `ui-todo.jpg`, `ui-calendar.jpg`, `ui-timer.jpg`, `ui-settings.png`).
- `src/ui/mod.rs`, `src/ui/settings.rs`: Mengoptimalkan frame pacing dan alokasi memori UI — menaikkan refresh karaoke lirik ke `16 ms` (~60 FPS) dan visualizer idle ke `33 ms` (~30 FPS) saat terlihat, menurunkan repaint saat overlay tersembunyi ke `250 ms`, serta menghindari kloning ulang `installed_fonts` dan `calendar_events` setiap frame pada `DeferredSettingsState`. (Earlier)
- `src/ui/mod.rs`, `src/config.rs`, `src/main.rs`: Menambahkan **Notch Dock Mode** (`notch_dock_mode`, default `true`) yang menempel pada tepi atas layar (`y = 0`) dengan telinga cekung atas (`paint_island_surface` + `egui::Mesh`), mini cover/disc badge + live lyric + 3-bar equalizer animasi saat idle musik, serta tetap otomatis tersembunyi saat aplikasi fullscreen aktif.
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`: Menambahkan ekstraksi cover album (`Storage_Streams::DataReader` pada WinRT SMTC & `mpris:artUrl` pada MPRIS2 Linux, cache per lagu, decode `image 0.25`) serta `extract_dominant_accent()` untuk mengekstrak warna aksen dominan album.
- `src/ui/mod.rs`, `src/ui/settings.rs`, `src/config.rs`: Mendesain ulang Music & Lyrics card mengikuti referensi visual 2-kolom (cover album membulat + title/artist + playbar + transport di kiri, multi-line wrapped `LayoutJob` karaoke original & romaji + terjemahan + upcoming preview redup di kanan, gradient wash atmosferik di bagian bawah, tombol pill `Translate` & `Romaji` di bawah, opsi `dynamic_album_color`, serta preset `"Crimson"`).
- `src/lyrics.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Menambahkan **Karaoke progressive highlight** (dukungan LRC baris & word-level `<mm:ss.xx>` via painter clip-rect), **Romaji / romanisasi** untuk lirik non-abjad (Jepang, Cina, Korea, Arab, dll. dengan fallback Kana offline), **Terjemahan lirik** ke bahasa pilihan (`id`, `en`, `ja`, `ko`, `zh-CN`, `ar`, `es`), serta lazy fallback font sistem (CJK & simbol media).
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`, `src/ui/mod.rs`: Menambahkan **Playbar interaktif** (seek posisi playback) beserta tombol transport lengkap (**Shuffle**, **Previous**, **Play/Pause**, **Next**, dan **Repeat** `Off`/`All`/`One`) melalui WinRT SMTC & D-Bus MPRIS2.
- `src/platform/windows.rs`, `src/platform/mod.rs`, `src/platform/linux.rs`, `src/state.rs`: Menambahkan deteksi aplikasi/game fullscreen di monitor utama (`GetForegroundWindow` + `GetMonitorInfoW`) dan otomatis menyembunyikan Dynamic Island (serta menonaktifkan top-edge hover reveal) selama fullscreen aktif; Linux saat ini menggunakan shortcut `false`.
- `src/clock.rs`: Menambahkan modul jam digital dengan dukungan zona waktu IANA (`chrono-tz 0.10`), daftar zona waktu umum (`Local`, `UTC`, `Asia/Jakarta`, `Asia/Tokyo`, dll.), format 12h/24h, opsi detik, serta 2 unit test.
- `src/state.rs`, `src/platform/mod.rs`, `src/platform/windows.rs`: Menambahkan delayed auto-hide (`auto_hide_delay_secs`, `idle_since`) dan `is_cursor_in_top_trigger` untuk memunculkan kembali notch saat kursor diarahkan ke tepi atas tengah layar.
- `src/lyrics.rs`: Menambahkan parser LRC (`[mm:ss.xx]`), sinkronisasi baris lirik aktif/berikutnya terhadap posisi playback, dan worker background (`ureq 3.5` + `serde_json`) ke `lrclib.net` dengan cache per lagu.
- `src/todo.rs`: Menambahkan `TodoStore` dan persistensi lokal `todos.toml` (add, toggle, remove, clear completed) beserta unit test.
- `src/calendar.rs`: Menambahkan parser `.ics` RFC 5545 (`parse_ics`), filter jadwal terdekat (`upcoming_events`), countdown, worker sinkronisasi berkala 5 menit untuk Google Calendar Secret iCal URL, dan alert 5 menit sebelum acara.
- `src/ui/settings.rs`: Menambahkan window sekunder yang bisa menerima focus keyboard (`show_viewport_immediate`) dengan tab **Appearance** (preset warna, scale, lebar idle/expanded, corner radius, offset, toggle modul), **Todo List**, dan **Google Calendar**.
- `src/platform/windows.rs`, `src/platform/linux.rs`: Menambahkan pembacaan media & kontrol playback (`Play/Pause`, `Next`, `Previous`) via WinRT SMTC (`Media_Control`) di Windows dan D-Bus MPRIS2 di Linux.
- `AGENTS.md`: Menambahkan panduan kolaborasi agent/developer (perintah build/test, batas modul, standar kualitas, API gotchas, dan aturan wajib sinkronisasi Obsidian `/docs`). Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].

### Changed

- `Cargo.toml`, `src/lyrics.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Mengganti romanisasi jaringan/manual dengan `kakasi 0.1` (Kanji+Kana Jepang) + `any_ascii 0.3` (Cina, Korea, Arab, Cyrillic, dll.) secara 100% offline; memindahkan seluruh kontrol Karaoke, Romaji, dan Translate khusus ke popup Settings; serta memperhalus tampilan pill tab, avatar notifikasi, tombol Dismiss, dan tema gelap Settings.
- `Cargo.toml`, `src/lyrics.rs`: Menambahkan `image 0.25` (`png`, `jpeg`, `webp`) & fitur WinRT `Storage_Streams` pada `windows 0.62`, serta memperluas `ActiveKaraoke` dengan `next_romaji` dan `next_translation`.
- `Cargo.toml`, `src/config.rs`, `src/main.rs`: Menambahkan fitur WinRT `Media` pada `windows 0.62`, opsi `karaoke_lyrics`, `romaji_lyrics`, `translate_lyrics`, dan `lyric_translate_lang` di `Config` & Settings, serta inisialisasi fallback font dasar di `main.rs`.
- `Cargo.toml`, `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Menambahkan fitur `Win32_Graphics_Gdi` pada `windows 0.62`, opsi `hide_in_fullscreen` (default `true`) di `Config` & Settings, serta polling fullscreen tiap `250 ms`.
- `Cargo.toml`: Menambahkan `chrono-tz 0.10` untuk resolusi zona waktu IANA.
- `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`, `src/tray.rs`, `src/main.rs`: Menambahkan `IdleDisplayMode` (`Auto`, `Clock`, `Lyrics`, `NextEvent`, `Minimal`), pengaturan zona waktu/format jam, slider `auto_hide_delay_secs`, sinkronisasi checkmark tray, koreksi lebar monitor berbasis `pixels_per_point()`, serta logika tampil-kembali saat hover di tepi atas layar.
- `Cargo.toml`: Menambahkan `ureq 3.5`, `serde_json 1.0`, `chrono 0.4`, dan fitur WinRT `Media_Control` pada `windows 0.62`.
- `src/config.rs`: Memperluas `Config` dengan pengaturan ukuran, warna, corner radius, toggle fitur, URL `.ics`, serta 4 tema preset (`obsidian`, `midnight`, `amoled`, `slate`) dengan backward-compatible `#[serde(default)]`.
- `src/tray.rs`, `src/ui/mod.rs`, `src/main.rs`: Menambahkan menu tray `Settings...`, hub tab expanded (`Overview`, `Lyrics`, `Todo`, `Cal`), tampilan lirik ringkas di state idle saat musik diputar, serta wiring seluruh background worker.

### Fixed

- `src/state.rs`, `src/platform/windows.rs`, `src/ui/mod.rs`: Memperbaiki bug di mana Dynamic Island tidak membesar saat di-hover — `set_hovered(true)` kini memicu `IslandVisualState::Expanded` meski sedang `Idle`, `dismiss_active` menjaga state `Expanded` selama kursor masih hover, dan deteksi hover di Windows memakai Win32 `GetCursorPos` + `GetWindowRect` agar tidak gagal pada window `WS_EX_NOACTIVATE`.

### Changed

- `README.md`, `assets/*`: Memperbarui dokumentasi fitur dan menyematkan seluruh galeri preview UI terbaru (`ui-notch-clock.png`, `ui-island-clock.png`, `ui-notch-music.png`, `ui-island-music.png`, `ui-home.jpg` [diperbarui], `ui-music-lyrics.jpg`, `ui-todo.jpg`, `ui-calendar.jpg`, `ui-timer.jpg`, `ui-settings.png`).
- `src/ui/mod.rs`, `src/ui/settings.rs`: Mengoptimalkan frame pacing dan alokasi memori UI — menaikkan refresh karaoke lirik ke `16 ms` (~60 FPS) dan visualizer idle ke `33 ms` (~30 FPS) saat terlihat, menurunkan repaint saat overlay tersembunyi ke `250 ms`, serta menghindari kloning ulang `installed_fonts` dan `calendar_events` setiap frame pada `DeferredSettingsState`. (Initial Bootstrap)
- `Cargo.toml`: Inisialisasi manifest proyek Rust (`edition = "2024"`) dengan dependensi `eframe 0.37` (`glow`), `windows 0.62`, `zbus 6.5` (target Linux), `tray-icon 0.27`, `serde`, `toml`, dan `directories`, serta profil release hemat ukuran (`lto = "thin"`, `strip = true`, `opt-level = "s"`).
- `src/config.rs`: Menambahkan loader/saver konfigurasi lokal `config.toml` (`auto_hide_when_idle`, `top_offset_px`, `collapse_timeout_secs`) beserta 3 unit test fallback & round-trip TOML.
- `src/state.rs`: Menambahkan model `NotificationItem`, state machine `IslandState` (`Idle`, `Compact`, `Expanded`), queue notifikasi dengan deduplikasi ID, timer auto-collapse, pause/auto-hide logic, serta 4 unit test alur state.
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`: Menambahkan platform bridge untuk Windows WinRT `UserNotificationListener` + Win32 `WS_EX_NOACTIVATE`/`WS_EX_TOOLWINDOW` dan backend D-Bus `org.freedesktop.Notifications` (`zbus`) untuk Linux, beserta mode CLI `--spike-notifications`.
- `src/tray.rs`: Menambahkan `TrayController` (ikon pill RGBA in-memory dan menu `Status`, `Pause/Resume`, `Auto-hide when idle`, `Exit`).
- `src/ui/mod.rs`, `src/main.rs`: Mengimplementasikan overlay window transparan top-center berbasis `eframe 0.37`, animasi lerp ukuran pill (`Idle` ↔ `Compact` ↔ `Expanded`), hover expand/pause timer, tombol `Dismiss`, serta flag `--demo` dan `--spike-notifications`. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].
- `docs/PRD-LiebertDynamicIsland-MVP.md`: Menyusun PRD MVP v1.0 untuk target Windows 10/11 dan Linux (X11/Wayland) dengan cakupan P0 System Notifications.
- `docs/00 - Dashboard Proyek.md`, `docs/01 - Arsitektur & Spesifikasi.md`, `docs/02 - Daftar Tugas & Backlog.md`, `docs/03 - Catatan Rapat & Keputusan.md`, `docs/04 - Changelog & Dev Log.md`: Inisialisasi struktur dokumentasi standar Obsidian Markdown.
- `.gitignore`: Menambahkan `.obsidian/`, `.obsidian/workspace.json`, dan `target/` agar konfigurasi lokal Obsidian serta artifact build Rust tidak masuk ke repository.

### Changed

- `docs/00 - Dashboard Proyek.md`: Menyelaraskan frontmatter `tags` (`project`, `liebert-dynamic-island`, `mvp`) sesuai standar workspace Obsidian proyek.
- `docs/01 - Arsitektur & Spesifikasi.md`: Menambahkan tautan eksplisit ke [[PRD-LiebertDynamicIsland-MVP]], rincian state machine overlay (`Idle` ↔ `Compact` ↔ `Expanded`), alur komponen, dan target non-fungsional (`< 500 ms` startup, `< 150 ms` latency, `< 25 MB` RAM idle, `>= 60 FPS`).
- `docs/02 - Daftar Tugas & Backlog.md`: Menyusun ulang tugas P0 ke dalam 4 fase pengerjaan (Setup Proyek & Technical Spike, Core Features, UI & Animation, Testing & Release).
- `docs/03 - Catatan Rapat & Keputusan.md`: Menambahkan `ADR-003` terkait 3 technical spike wajib di Fase 1 (WinRT identity, Linux D-Bus mode, dan pemilihan renderer Rust).
