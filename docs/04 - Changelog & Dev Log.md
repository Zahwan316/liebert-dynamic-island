---
type: changelog
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - dev-log
---

# 04 - Changelog & Dev Log

## 2026-11-27

### Added (v0.2.0)

- `src/ui/icons.rs`, `src/ui/mod.rs`: **Animasi maskot memutar musik (`paint_mascot_music`, `MascotMusicMotion`)** (Windows & Linux) — saat musik aktif diputar, mulut maskot berubah menjadi **3 bar equalizer animasi** (+ kapsul vertikal kanan), mengangguk ke kiri-kanan (`nod_x`), dan melakukan lonjakan pop pembesaran **"yepii"** berkala (`~2,2 s`), sambil tetap menjaga **pelacakan kursor mata** (`eye_offset`) dan **kedip alami** (`eye_open`) di pill musik idle, mode `Mascot`/`Clock`, serta tile Jam tab Home. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-032).
- `src/state.rs`, `src/ui/mod.rs`: **Countdown bar notifikasi mengecil ke tengah** (`IslandState::collapse_remaining_frac` + `render_compact`) — saat popup notifikasi tampil (`Compact`), bar berwarna brand aplikasi di tepi bawah menyusut simetris ke tengah seiring sisa waktu auto-dismiss (dan tertahan penuh saat hover/Expanded).
- `src/ui/mod.rs`: **Slide-to-dismiss pada tab `Notifications`** (`render_notifications_tab` + `notif_swipe`) — geser horizontal kartu notifikasi ke kiri atau kanan melewati ambang batas (`max(48px, 28%)`) menampilkan latar hapus merah + ikon `Close` lalu menghapus notifikasi dari riwayat maupun antrean aktif.
- `src/config.rs`, `src/ui/{mod,settings}.rs`: **Material `Glassmorphism` & peningkatan keterbacaan seluruh mode kaca** (Windows & Linux) — menambah varian kelima `BackgroundMaterial::Glassmorphism` (`theme` bloom dual-orb + specular highlight + border kaca crisp) di Settings (`Theme, Colors & Glass`) maupun pill/expanded overlay; menaikkan floor alpha scrim gelap (`178–224`) serta kecerahan `text_color32`/`muted_color32` pada semua mode kaca (`Acrylic`, `FrostedGlass`, `LiquidGlass`, `Glassmorphism`) agar teks tetap mudah terbaca di wallpaper terang.
- `src/ui/settings.rs`: **Daftar kontributor GitHub pada tab `About`** — menampilkan `Zahwan316` (Creator · 34 commits) dan `Gjenius20` / `Zjen` (Contributor · 8 commits) sesuai riwayat repositori GitHub.

### Changed (v0.2.0)

- `AGENTS.md`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi dirilis ke **`0.2.0`** dengan aturan carry-over semver dan alur rilis `dev-main` → `main` → changelog + upload GitHub Releases di `AGENTS.md` §6; `CHANGELOG.md` di root dibuat dan aset `AppImage`, `.tar.gz`, Windows `.zip`, NSIS `.exe`, serta `liebert-dynamic-island-aur-0.2.0.tar.gz` disiapkan. `cargo fmt --all -- --check`, `cargo test --offline` **80 passed**, `cargo clippy --offline --all-targets -- -D warnings`, dan `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings` bersih.

### Fixed (v0.2.0)

- `src/lyrics.rs`, `src/ui/mod.rs`: **Perbaikan bug lirik tidak sesuai lagu yang diputar** — menyertakan bucket durasi 5 detik ke kunci `raw_cache`/`enriched_cache` (agar pencarian awal saat `duration = 0` tidak mengunci hasil salah ketika durasi asli tiba), memperketat validasi hasil `/api/search` dan fallback `/api/get` (`lrc_candidate_matches`, `titles_match`, toleransi durasi ±25s, kecocokan artis), serta mereset & memvalidasi kecocokan track `(artist, title)` secara case-insensitive saat `PlatformEvent::Media`, `LyricsLoaded`, dan `apply_snapshot`.
- `src/state.rs`, `src/platform/windows.rs`: **Penyembunyian nama website/domain pada notifikasi browser Brave** (`is_brave_app`, `is_web_origin_line`, `strip_brave_origin_from_text`) — membuang origin web seperti `web.whatsapp.com`, `via chat.openai.com`, `youtube.com`, dan akhiran `(<domain>)`/`— <domain>` di jalur Linux D-Bus maupun Windows WinRT sebelum teks diringkas.

### Measured (v0.2.0)

- **Settings Wayland CPU (`--settings`, host Hyprland Linux):** **`18,21 MB` RSS (`18652 kB`), `13,47 MB` PSS (`13792 kB`)**; anon/heap private **`3,33 MB`** (`3412 kB`), binary `RssFile` `8,33 MB` PSS (`8526 kB`), shmem `1,81 MB` PSS, driver GPU `0 kB`.
- **Demo + Overlay Wayland (`--demo`, host Linux saat media CJK aktif):** **`20,01 MB` RSS (`20488 kB`), `16,25 MB` PSS (`16641 kB`)**; anon/heap private **`5,08 MB`** (`5200 kB`, turun dari `5,12 MB`), `NotoSansCJK-Bold`/`Regular` via mmap masing-masing hanya `537 kB` + `375 kB` PSS.
- **Status target `< 10 MB`:** **Belum tercapai pada total RSS/PSS** (meski anon/private `3,33 MB` & `5,08 MB`) karena halaman kode binary monolitik `RssFile` (`9,2–10,7 MB`) masih memuat `eframe`/`egui_glow`/`winit`/`rustls`/`zbus` dalam satu executable. Host Windows RSS belum dapat diukur pada host Linux. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-032) dan [[02 - Daftar Tugas & Backlog]].

### Added (v0.1.13)

- `src/ui/icons.rs`, `src/ui/mod.rs`: **Animasi kedip & glow maskot idle** (Windows & Linux) — `paint_mascot_fx()` menambah dua pass halo cahaya di belakang kapsul maskot putih serta kelenturan kedip alami ~170 ms setiap ~3,6–5,2 detik yang dikendalikan `step_logic` secara event-driven pada pill idle (`IdleDisplayMode::Mascot`/`Clock`/`Minimal`) maupun tile Jam di Home. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-031).
- `src/platform/{mod,linux,windows}.rs`, `src/ui/mod.rs`: **Ikon aplikasi notifikasi & tab `Notifications` independen alignment** (Windows & Linux) — ekstraksi ikon 28×28 RGBA di Linux (`hints["image-data"]`/`"icon_data"`/`"image-path"`/`app_icon`/lookup `.desktop`/tema `hicolor` dengan cache terikat 32 entri) dan Windows (`AppInfo().DisplayInfo().GetLogo` + shared `read_stream_ref_bytes`); tab **`NOTIFICATIONS`** (`IslandTab::Notifications`, `notification_history` max 30 entri) menampilkan riwayat lengkap notifikasi beserta kontrol inline (`Popups`, `DND`, `Test`, `Clear`, `Settings`). Baik Compact, Expanded aktif, maupun tab `Notifications` dipaksa rata kiri (`egui::Align::Min`) dan **tidak** terpengaruh `Config::text_align`.
- `src/ui/mod.rs`: **Komponen Kalender bulanan interaktif di tab Calendar** (`render_calendar_tab`) — tampilan 2 kolom dengan grid bulan 7-kolom `< Month Year >` (cincin hari ini, pill tanggal terpilih, titik indikator tanggal ber-event, klik untuk memfilter event di panel kanan) yang tetap tampil meski URL `.ics` belum diisi.
- `src/ui/icons.rs`, `src/ui/mod.rs`: **Ikon vektor di tab System Monitor** — menambah `IconKind::{Cpu, Memory, Disk, ArrowDown, ArrowUp, Bell, Info}` serta menempelkan ikon ke ring CPU/RAM/Disk dan baris kecepatan Network.
- `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: **Material background `Acrylic`, `Frosted Glass`, dan `Liquid Glass` + tab `About`** — menambah enum `BackgroundMaterial::{Solid, Acrylic, FrostedGlass, LiquidGlass}` dengan pemilih pill chip di Settings → Display (`Theme, Colors & Glass`) yang mewarnai latar pill/expanded (berikut tekstur grain 32×32 repeat untuk Acrylic, mist es untuk Frosted Glass, dan kilau Fresnel/meniskus atas-bawah untuk Liquid Glass termasuk pada mode Notch) serta latar dashboard Settings; ditambah tab **`About`** (`SettingsTab::About`) berisi info pembuat (`Zahwan316`), versi (`env!("CARGO_PKG_VERSION")`), URL repo (`Copy URL`), lisensi `GPL-3.0-or-later`, dan profil runtime aktif.

### Changed (v0.1.13)

- `src/ui/mod.rs`: **Perbaikan total layout tab Home Gallery Grid (`render_overview_tab`, tinggi expanded `208*s`), header expanded Bold tanpa garis pemisah, & jam font Bold**:
  - Mengganti widget nested `ui.horizontal`/`right_to_left` di dalam tiap tile Home dengan tata letak `Rect` eksplisit (`gallery_grid_layout` hingga 4×2 / baris akhir rata penuh + `paint_gallery_header_in_rect`) sehingga tidak ada tumpang-tindih atau clipping di Linux maupun Windows.
  - Menghilangkan `ui.separator()` di antara header dan konten saat mode Expanded, memakai `ui_bold_font()` pada judul header expanded (`HOME`, `NOTIFICATIONS`, dll.), judul notifikasi, serta jam (`build_clock_layout_job` & tile Jam Home) — dengan memuat 1 font Bold Latin ringan (~300 KB mmap) pada `apply_configured_fonts_light` tanpa membebani heap dengan font CJK Bold 20 MB.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi naik ke **`0.1.13`**; AppImage (`6,4 MB`) + `.tar.gz` (`6,2 MB`) Linux serta `.zip` (`4,2 MB`) + NSIS installer `liebert-dynamic-island-windows-x86_64-setup.exe` (`3,4 MB`) dibangun dan diverifikasi. `cargo fmt --all -- --check`, `cargo test --offline` **77 passed**, `cargo clippy --offline --all-targets -- -D warnings`, dan `cargo clippy --offline --target x86_64-pc-windows-gnu --all-targets -- -D warnings` (Docker `rust:slim` + `gcc-mingw-w64-x86-64` + `nsis`) bersih.

### Measured (v0.1.13)

- **Settings Wayland CPU (`--settings`, host Hyprland Linux):** **`19,58 MB` RSS (`20052 kB`), `13,83 MB` PSS (`14165 kB`)**; anon/heap private tetap rendah **`3,31 MB`** (`3392 kB`), binary `RssFile` `7,97 MB` PSS (`8163 kB`), shmem compositor `2,55 MB` PSS (`2610 kB`), `NotoSans-Regular.ttf` via mmap hanya `38 kB` PSS, dan driver GPU/LLVM **`0 kB`**.
- **Demo + Overlay Wayland (`--demo` settled idle + glow/blink maskot, host Linux):** **`18,95 MB` RSS (`19408 kB`), `15,07 MB` PSS (`15434 kB`)**; anon/heap private **`5,12 MB`** (`5248 kB`), binary `RssFile` `9,80 MB` PSS (`10032 kB`), shmem `0,15 MB` PSS, font Bold `NotoSans-Bold.ttf` via mmap hanya menambah **`71 kB` PSS**.
- **Status target `< 10 MB`:** **Belum tercapai pada total RSS/PSS** (meski private anon/heap sudah `3,31 MB` dan `5,12 MB`). Penghalang struktural tetap pada halaman kode/rodata executable tunggal `RssFile` (`~9,2–10,7 MB`) karena seluruh stack GUI, `eframe`/`egui_glow`/`winit`/`zbus`/`tray-icon`/`rustls` berada di satu binary; pemisahan daemon vs UI binary atau penghapusan jalur `eframe`/GL tetap menjadi syarat untuk menembus `< 10 MB` total RSS. Host Windows RSS belum diukur pada host Linux. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-031) dan [[02 - Daftar Tugas & Backlog]].

### Added (v0.1.12)

- `src/state.rs`: **Sanitasi notifikasi web/markup (`sanitize_notification_text`, `sanitized_notification_item`)** lintas OS untuk menghapus tag HTML/XML (seperti `<a href="...">...</a>`, `<b>`, `<i>`, `<br/>`, tag terpotong `<a href="...`) dan men-decode entitas HTML (`&amp;`, `&lt;`, `&quot;`, `&#x27;`, `&nbsp;`, numerik dec/hex) termasuk markup ter-escape ganda (`&lt;a href=...&gt;`) sambil mempertahankan teks non-HTML seperti `1 < 2`, `<3`, dan alamat `<user@example.com>`. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-030).

### Changed (v0.1.12)

- `src/ui/mod.rs`: **Desain ulang tab Home bergaya Gallery Grid** (`render_overview_tab`, tinggi expanded `Overview` `196*s`) menjadi grid simetris kartu berukuran seragam (`cols = 1/2/3`, 3×2 saat semua modul aktif) dengan `render_gallery_tile_header`, tile visual `Clock` (berikut hero maskot `paint_mascot`), `Music` (artwork thumbnail + lyric + pill bar), `System` (pill meter CPU/RAM/Disk), `Tasks`, `Focus`, `Events`, dan `Battery` opsional untuk melengkapi grid.
- `Cargo.toml`, `src/platform/wayland_settings.rs`: **Task RAM `< 10 MB`** — profile rilis memakai `lto = "fat"` + `opt-level = "z"` (ukuran binary Linux turun `17 MB → 15 MB`, installer NSIS payload turun ke ~9,8 MB uncompressed / ~3,29 MB LZMA) serta inisialisasi SHM `SlotPool` `wayland_settings.rs` dipangkas ke 1× buffer (`byte_len`) dan tumbuh sesuai kebutuhan compositor. Lihat **Measured (v0.1.12)**.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi naik ke **`0.1.12`**; AppImage (`6,4 MB`) + `.tar.gz` (`6,1 MB`) Linux serta installer NSIS + zip Windows dibangun ulang. `cargo fmt --all -- --check`, `cargo test` **76 passed**, `cargo clippy --all-targets -- -D warnings`, dan clippy `x86_64-pc-windows-gnu` (Docker `rust:slim`) bersih.

### Fixed (v0.1.12)

- `src/state.rs`, `src/platform/linux.rs`, `src/platform/windows.rs`: Notifikasi WhatsApp Web dan browser (Windows WinRT `ToastGeneric` + Linux D-Bus `Notify` & passive monitor `BecomeMonitor`) kini selalu disanitasi sebelum masuk `IslandState::push_notification`, sehingga tag `<a href=...>` maupun entitas HTML tidak lagi bocor ke UI.

### Measured (v0.1.12)

- **Settings Wayland CPU (`--settings`, host Hyprland Linux):** turun lagi dari `17,63 MB RSS` (`13,05 MB PSS`) → **`16,94 MB` RSS (`17348 kB`), `12,82 MB` PSS (`13124 kB`)**; anon/heap private hanya **`3,30 MB`** (`3380 kB`), binary `RssFile` turun ke `8,90 MB` PSS (`9116 kB`), shmem `1,27 MB` PSS.
- **Demo + Overlay Wayland (`--demo` settled idle + maskot, host Linux):** turun dari `24,63 MB RSS` (`19,36 MB PSS`) → **`18,85 MB` RSS (`19304 kB`), `15,08 MB` PSS (`15447 kB`)**; anon/heap private turun di bawah 5 MB (**`4,98 MB`** / `5096 kB`), binary `RssFile` turun dari `14024 → 10600 kB`.
- **Status target `< 10 MB`:** **Belum tercapai pada total RSS/PSS** (meski anon/private RAM sudah `3,30 MB` dan `4,98 MB`). Penghalang tersisa adalah `RssFile` kode/rodata dari satu executable monolitik (`eframe`/`egui_glow`/`winit`/`rustls`/`zbus`/`tray-icon`); menembus `< 10 MB` total membutuhkan pemisahan executable ringan atau menghapus fallback `eframe`/OpenGL. Host Windows RSS belum dapat diukur dari Linux. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-030) dan [[02 - Daftar Tugas & Backlog]].

### Added (v0.1.11)

- `src/config.rs`, `src/platform/{mod,linux,windows}.rs`, `src/ui/{icons,mod,settings}.rs`, `src/main.rs`: **Maskot idle pelacak kursor** (Windows & Linux). Ditambahkan mode `IdleDisplayMode::Mascot` dan toggle `Config::show_idle_mascot` (default `true`); `icons::paint_mascot()` menggambar wajah kapsul putih minimal (`L` + `I` mata bergerak mengikuti posisi kursor ternormalisasi via Hyprland/X11 `cursor_screen_info` di Linux dan `GetCursorPos` + DPI di Windows, serta mulut kapsul berlubang + kapsul vertikal kecil di kanan). Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-029).
- `src/platform/wayland_settings.rs`, `src/platform/windows_settings.rs`, `src/platform/mod.rs`, `src/ui/settings.rs`: **Renderer Settings CPU murni (`SoftCanvas`)** pada Linux Wayland (`wl_shm` + `xdg_shell` CSD/SSD) dan Windows Win32 (`CreateDIBSection` + `BitBlt`), mempertahankan `eframe` hanya untuk fallback X11 sehingga driver OpenGL/EGL/LLVM tidak lagi dimuat pada proses Settings utama.

### Changed (v0.1.11)

- `src/ui/mod.rs`, `src/ui/icons.rs`: **Tab Home bergaya Bento Grid** (Windows & Linux) dengan kartu metrik membulat dan `paint_pill_bar` chart (CPU, RAM, Disk, Tasks, Calendar/Timer) serta status ringkas bebas teks/ikon berlebih.
- `src/ui/settings.rs`: **Desain ulang dashboard Bento Grid** (Windows & Linux) dengan sidebar ikon + mini-maskot live di kiri, kartu 2 kolom adaptif (`bento_2col` / `bento_card`), pill toggle chip, pill chart indikator baterai/konfigurasi, dan pemindaian font terinstal secara lazy.
- `Cargo.toml`, `Cargo.lock`, `src/ui/mod.rs`, `src/ui/taskbar.rs`, `src/platform/{wayland_layer,windows_overlay}.rs`, `src/main.rs`: **Task RAM `< 10 MB`** — `memmap2::Mmap` (`load_font_blob`) memetakan font sistem secara shareable tanpa menumpuk blob `NotoSansCJK` 19–20 MB ke heap, fitur `default_fonts` `eframe` dinonaktifkan (`apply_configured_fonts_light` menunda pemuatan `LyricsBold` dan taskbar memakai label ASCII `CPU`/`RAM`/`DSK`), batas `max_texture_side` diturunkan dari `2048` ke `1024`, `SlotPool` Wayland menyusut ke 512 KB setelah collapse, dan Settings dipindah ke CPU rasterizer. Lihat **Measured (v0.1.11)**.
- `Cargo.toml`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi naik ke **`0.1.11`**; `Liebert_Dynamic_Island-0.1.11-x86_64.AppImage` + `.tar.gz` Linux dan `liebert-dynamic-island-windows-x86_64-setup.exe` + `.zip` Windows (via Docker `rust:slim` + `gcc-mingw-w64-x86-64` + `nsis`) dibangun ulang. `cargo test` **75 passed**, `cargo fmt --all -- --check` bersih, `cargo clippy --all-targets -- -D warnings` bersih, dan `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings` bersih.

### Fixed (v0.1.11)

- `src/ui/mod.rs`, `src/platform/linux.rs`: **Perbaikan bug putih lirik karaoke nge-blink** (Windows & Linux) — `paint_karaoke_in_rect` dan `render_wrapped_karaoke` kini memisahkan region clip kiri (putih) dan kanan (redup) secara saling lepas agar tepi anti-aliased tidak double-blend/pop di `p >= 0.999`; `scroll_x` menggunakan lebar galai utuh (bukan `.min(avail_w)`); `PillLyricTransition` melacak `prev_progress` alih-alih melompat ke `1.0`; mundur waktu media `< 750 ms` pada `PlatformEvent::Media` dan `periodic_lock` MPRIS2 disaring agar garis progress tidak berkedip mundur setiap 2,5 detik.
- Live Wayland smoke test & fix `epaint`: batas `max_texture_side` dinaikkan dari `512` ke `1024` karena `epaint::TextureAtlas::new` meng-assert `size[0] >= 1024`.

### Measured (v0.1.11)

- **Settings Wayland CPU (`--settings`, host Hyprland Linux):** turun drastis dari `VmRSS 90,2 MB` / `Pss 78,2 MB` → **`Rss 17,63 MB` (`18052 kB`), `Pss 13,05 MB` (`13363 kB`)**; heap + anon private (`Pss_Anon`/`Private_Dirty`) hanya **`3,51 MB`** (`[heap] 1240 kB` + `[anon] 576 kB`), shmem `memfd` **`1,27 MB` PSS**, dan `libnvidia`/`libLLVM` **0 KB**.
- **Demo + Overlay Wayland (`--demo` settled idle + mascot, host Linux):** turun dari `VmRSS 63,1 MB` / `Pss 59,0 MB` → **`Rss 24,63 MB` (`25220 kB`), `Pss 19,36 MB` (`19824 kB`)**; `Pss_Anon`/`Private_Dirty` hanya **`5,75 MB`**, `NotoSansCJK` via `mmap` hanya memetakan `428 kB` PSS (`Regular`) + `340 kB` PSS (`Bold`) halaman font terpakai.
- **Status target `< 10 MB`:** **Belum tercapai pada total RSS/PSS** (walaupun memori private anon/heap sudah `3,51 MB` dan `5,75 MB`). Penghalang utama kini adalah **halaman kode/rodata binary monolitik** (`RssFile` binary sendiri `9,60 MB` PSS di `--settings` dan `13,70 MB` PSS di `--demo`) karena `eframe`/`egui_glow`/`winit`/`rustls`/`zbus`/`tray-icon` masih dikompilasi ke satu executable 17 MB. RSS Windows belum terukur dari host Linux (Settings Win32 kini juga CPU `BitBlt` bebas GL). Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-029) dan [[02 - Daftar Tugas & Backlog]].

### Added (v0.1.10)

- `src/ui/mod.rs`: **Animasi pergantian tab** (both) untuk next/prev via arrow keyboard, chevron samping, dan click-through Home/tombol Battery. `TabSwitch` menyimpan arah + `Instant`; konten baru masuk dengan fade (180 ms) dan sedikit slide dari sisi navigasi. Hanya meminta repaint cepat selama transisi—idle tidak berubah. Test `tab_transition_slides_in_from_nav_side_and_finishes`. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-028).

### Changed (v0.1.10)

- `src/ui/mod.rs`: Expanded card memakai **satu lebar untuk semua tab** (`468*s`) sehingga perpindahan tab tidak menggeser kartu; header label tab kini benar-benar terpusat di kolomnya, spacer horizontal header dan jam Home dinolkan agar perhitungan centering presisi. Kolom Music kiri/kanan disamakan, lebar wrap lirik dipatok ke kolom agar scrollbar tidak menggeser teks, dan grup transport menghitung gap kelima. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-028).
- `src/ui/settings.rs`: Settings dibuat lebih minimalis (both): judul tab duplikat dihapus, `settings_group()` kini flat tanpa kartu bertingkat, margin panel `14 → 10`, rail tab `120 → 104`, spacing item `8×6 → 6×4`, serta hint/teks penjelas yang berulang dipangkas tanpa menghapus satu pun kontrol.
- `src/ui/mod.rs`: Palet gradient lebih lembut (both): `soft_accent_rgb()` menarik warna album/aksen ke abu netral dan membatasi kecerahan (`≤190`), wash dimulai lebih awal dengan kekuatan `0,55` (dulu `0,85`), rose glow dipaksa diganti highlight netral dingin, dan base pill/dock jadi abu gelap `[10,10,13]` / `[14,15,19]`. Warna donut/status neon juga diredupkan. Test `soft_accent_mutes_loud_colors_without_losing_hue`.
- `src/ui/mod.rs`, `src/ui/settings.rs`: Task RAM — blob font kini di-`Arc`-kan dan dibagikan antarface (menghapus salinan heap byte font), fallback `user_selected_regular_fallback` tidak lagi menyimpan `Vec<u8>` yang tak terpakai, dan launcher Settings Unix memakai `exec` untuk percobaan hardware GL sehingga tidak ada proses launcher tambahan ~7 MB. Lihat **Measured** di bawah.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi naik ke **`0.1.10`**; AppImage + tar.gz Linux dan installer + zip Windows dibangun ulang. `cargo test` **74 passed**, `cargo fmt --all -- --check` bersih, `cargo clippy --all-targets -- -D warnings` bersih, clippy Windows-target bersih (Docker `rust:slim` + `gcc-mingw-w64-x86-64`).

### Fixed (v0.1.10)

- `src/ui/mod.rs`: Ruang kosong di atas expanded hilang (both)—inset atas `9*s → 2*s` (kartu musik `10*s → 2*s`) dan separator dipadatkan; ambang `expand_alpha` diturunkan agar konten muncul lebih awal saat kartu membuka. Gutter arrow dan chevron tengah tidak berubah.
- `src/ui/mod.rs`: Jam Home kini menampilkan label zona waktu sebaris (menggantikan placeholder "No music playing" yang memakan satu baris), sehingga info tetap lengkap tanpa baris tambahan.

### Measured (v0.1.10)

- Overlay Wayland/`--demo` (host Linux): `VmRSS 63,1 MB` (63.100 kB), `Pss 59,0 MB`, `RssAnon 46,1 MB`, `RssFile 16,7 MB` — praktis sama dengan baseline sebelum perubahan. Dedup byte font tidak terlihat pada skala ini karena font sistem merupakan `RssFile`/blok heap kecil; Investigasi menunjukkan ~52 MB region anonymous private-dirty milik dependency di luar egui.
- Jendela Settings (host NVIDIA, `VmRSS 90,2 MB`, `Pss 78,2 MB`, `RssFile 67,8 MB`) — turun dari `102,8 MB` terutama karena proses launcher diganti `exec`; tetap didominasi `libnvidia-gpucomp`/`libnvidia-eglcore`/`libLLVM`. RSS Windows tidak terukur dari host Linux.
- **Target `< 10 MB` belum tercapai** untuk overlay maupun Settings. Rincian penghalang dan utang teknis dicatat di [[03 - Catatan Rapat & Keputusan]] (ADR-028) dan [[02 - Daftar Tugas & Backlog]].

### Added (v0.1.9)

- `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Menambah opsi idle **`Battery`** pada `IdleDisplayMode` (Settings → Idle Display & Clock) untuk menampilkan persentase/status baterai di pill saat idle pada Linux dan Windows; jika tidak ada baterai terdeteksi, pill menampilkan `No battery`. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-027).

### Changed (v0.1.9)

- `src/ui/mod.rs`: Layout expanded diberi gutter kiri/kanan mengikuti lebar tombol arrow side-nav sehingga konten tidak lagi tertutup panah; header tab dirapikan (label + settings), dan tab **Home** dibuat lebih minimalis (jam ringkas, baris info lebih padat). Lihat [[01 - Arsitektur & Spesifikasi]].

### Fixed (v0.1.9)

- `src/ui/mod.rs`: Memperbaiki ruang kosong atas pada konten expanded dengan menyelaraskan inset konten ke rect kartu dan gutter arrow (both).

### Added (v0.1.8)

- `src/config.rs`, `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`, `src/ui/settings.rs`, `src/daemon.rs`, `src/ui/mod.rs`: **Autostart "Start with OS"** untuk Windows **dan** Linux, tanpa Administrator/root. Windows menulis/ Menghapus nilai HKCU `...\CurrentVersion\Run\LiebertDynamicIsland` memakai API registry `windows 0.62` (`Win32_System_Registry` ditambahkan ke `Cargo.toml`); Linux menulis/menghapus `~/.config/autostart/liebert-dynamic-island.desktop` dengan `Exec` sadar `APPIMAGE` (path AppImage persisten, bukan mount AppDir sementara). Toggle ada di Settings → System → **Startup** dan hanya diterapkan saat nilainya berubah (jalur Settings mandiri, Settings inline, dan `UiToDaemon::ConfigSaved`). `installer/windows/installer.nsi`: uninstaller kini menghapus nilai Run tersebut agar tidak ada entri autostart yatim. Test murni: `windows_autostart_command_quotes_paths_with_spaces`, `desktop_autostart_entry_has_required_keys`, `desktop_exec_escapes_shell_metacharacters`. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-026).
- `src/ui/mod.rs`, `src/ui/settings.rs`, `src/config.rs`: **Tab Battery** baru (both), muncul saat `show_battery = true`, menampilkan donut persentase, status (Charging/Discharging/Full), sisa waktu, dan animasi charging berupa segmen berjalan + titik orbit (`request_repaint_after(33 ms)` hanya saat tab Battery aktif dan sedang charge/full). PC tanpa baterai menampilkan **`No battery detected`** sesuai preferensi pengguna. Settings kini punya 7 tab dengan grup **Battery Tab**, **Device Status**, dan **Low Battery Alerts**.
- `src/platform/linux.rs`: **Fallback notifikasi pasif** untuk Linux. Bila `org.freedesktop.Notifications` sudah dipegang daemon lain (dalam pengujian: quickshell), aplikasi tidak gagal—`BecomeMonitor` (`org.freedesktop.DBus.Monitoring`) memantau method call `Notify`/`CloseNotification` dan meneruskannya ke antrean yang sama dengan listener langsung. `--spike-notifications` kini melaporkan pemilik nama dan mode monitor; status listener tetap `Listening` bila fallback aktif, atau `Error` yang menjelaskan kedua kegagalan bila keduanya tidak tersedia. Test baru `monitored_notify_message_decodes_into_notification` membuktikan dekode method call `Notify` menjadi `NotificationItem`. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-026).

### Changed (v0.1.8)

- `src/ui/mod.rs`: Chart CPU/RAM/Disk di tab System kini **donut chart vektor** (`paint_donut` + `render_stat_donut`) menggantikan progress bar horizontal (both); label disk Linux menjadi `Disk /` dan Windows `Disk C:`. Baris baterai di tab System dipangkas—baterai kini punya tab sendiri dengan tombol lompat cepat.
- `src/ui/mod.rs`: **Ruang arrow kiri/kanan** di expanded disediakan dengan inset konten tengah (`draw_side_nav` tetap menggambar chevron di tengah tepi kartu), sehingga tombol tidak menimpa teks. Tab **Home** lebih rapi: separator + jarak antarbaris yang konsisten, dan tab **Tasks** kini menampilkan `{pending} pending • {done} done`, progress bar, baris kartu dengan latar halus, serta area scroll lebih tinggi (both).
- `src/ui/settings.rs`: Isi panel kanan dikelompokkan—judul tab di atas, lalu `settings_group()` (judul grup → isi) untuk **Startup**, **Taskbar System Monitor**, **Battery Tab**, **Device Status**, dan **Low Battery Alerts**; grup System/Battery lama dirapikan (both).
- `src/sysmon.rs`: Deteksi penyimpanan Linux kini mengagregasi **seluruh filesystem mount nyata** dari `df -P -k` (root + `/home` + `/var`, dll.), bukan hanya `/`; filesystem pseudo (`tmpfs`, `cgroup`, `sysfs`, …) dilewati agar tidak mengacaukan persentase. Cache kapasitas 10 detik dipertahankan. Test: `linux_df_capacity_aggregates_system_mounts_and_skips_pseudo_filesystems`.
- `src/ui/mod.rs`, `src/ui/settings.rs`: Optimasi RAM lanjutan—hash album art dihitung sekali per sidik jari pointer+byte tepi (`album_art_fingerprint`/`album_art_hash`) alih-alih setiap repaint, dan lebar idle clock di-cache per detik tampilan (`clock_width_cache`) agar tidak layout ulang glyph tiap frame. Lihat **Measured** di bawah untuk hasil sebenarnya.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi naik ke **`0.1.8`**; AppImage + tar.gz Linux dan installer + zip Windows dibangun ulang. `cargo test` **72 passed**, `cargo clippy --all-targets -- -D warnings` bersih, clippy Windows-target (container Docker) bersih.

### Fixed (v0.1.8)

- `src/platform/linux.rs`: Notifikasi Linux yang "tidak masuk ke Dynamic Island" kini tersampaikan lewat mode monitor pasif tanpa perlu menghentikan daemon notifikasi pengguna; `--spike-notifications` tidak lagi salah melaporkan "free to claim" ketika ada incumbent.
- `README.md`: Catatan troubleshooting Linux diperbarui—cukup pastikan sesi D-Bus tersedia; bila nama dipegang daemon lain, aplikasi otomatis memakai mode monitor (sebelumnya pengguna disuruh menghentikan daemon).

### Measured (v0.1.8)

- Overlay Wayland/`--demo`: `VmRSS 63,1 MB`, `Pss 59,0 MB`, `RssAnon 46,1 MB`, `RssFile 16,6 MB` (proses yang sama, setelah fitur baru). Target **`< 10 MB` belum tercapai**—didominasi halaman kode/font `egui` dan pustaka grafis host yang tersentuh saat startup, bukan oleh alokasi per-frame; cache baru menghapus pekerjaan berulang tetapi tidak mengecilkan resident set secara drastis.
- Jendela Settings pada host NVIDIA tidak diukur ulang pada rilis ini; nilai v0.1.6 (`VmRSS 102,8 MB` / `Pss 69,0 MB`) tetap menjadi rujukan. RSS Windows tidak dapat diukur dari host Linux. Utang teknis target `< 10 MB` tetap terbuka (lihat ADR-023/024/026).

### Added (v0.1.7) — Windows Installer

- `installer/windows/installer.nsi`, `README.md`, `dist/liebert-dynamic-island-windows-x86_64-setup.exe`, `dist/liebert-dynamic-island-windows-x86_64.zip`: Installer Windows **berhasil dibangun di host Linux** memakai container Docker `rust:slim` + `nsis` + `gcc-mingw-w64-x86-64`. Binary di-cross-compile ke `x86_64-pc-windows-gnu`, lalu `makensis -DAPP_VERSION=0.1.7 -DSTAGING_DIR=... -DOUT_FILE=...` menghasilkan installer NSIS per-user (Start Menu + uninstaller). Catatan: di Linux opsi `makensis` memakai `-D`, bukan `/D`; perintah lengkap ditambahkan ke `README.md`.
- Artefak terverifikasi: `dist/liebert-dynamic-island-windows-x86_64-setup.exe` **`4,06 MB`** (SHA256 `204b41f7508d3e591882fd96edd318b8ca252a73ffb18649ee4ba514ddf19b00`), `dist/liebert-dynamic-island-windows-x86_64.zip` **`5,0 MB`**. `file` mengenali installer sebagai `Nullsoft Installer self-extracting archive`, dan `7z l` menunjukkan payload `liebert-dynamic-island.exe`, `LICENSE.md`, `README.md`, serta `uninstall.exe`. Tidak ada perubahan kode Rust pada request ini; keduanya berada di `dist/` yang di-gitignore.

### Changed (v0.1.7)

- `src/ui/mod.rs`: Header mode Expanded kini **hanya** menampilkan tulisan tab aktif dan tombol Settings; strip tab, chevron header, dan toggle Notch/Island dihapus (toggle tetap tersedia di Settings → Display). Navigasi Left/Right sudah tersedia lewat keyboard, dan tombol chevron vektor dipindah ke **tengah tepi kiri/kanan kartu expanded** (`draw_side_nav`, `ui.interact` + `paint_icon`) sehingga tidak mengganggu baris header. Berlaku untuk Linux **dan** Windows karena keduanya memakai `render_island_ui`.
- `src/ui/mod.rs`, `src/ui/settings.rs`: Panel System/Battery menampilkan teks eksplisit **`No battery detected`** bila `/sys/class/power_supply` tidak memiliki baterai (PC desktop), baik di tab System overlay maupun di Settings → System. Bila baterai ada, persentase/status/estimasi waktu tetap ditampilkan seperti sebelumnya.
- `src/sysmon.rs`: Menambah test integrasi ringan `linux_sampler_reads_live_proc_metrics` yang memanggil sampler `/proc` nyata dan memastikan RAM/CPU terbaca, sebagai bukti system monitor Linux bekerja.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi naik ke **`0.1.7`**; AppImage + tar.gz Linux dibangun ulang. `cargo test` **67 passed**, clippy Linux + Windows-target bersih. Lihat [[03 - Catatan Rapat & Keputusan]] (ADR-025).

### Added (v0.1.6)

- `src/ui/icons.rs`, `src/ui/mod.rs`: Ikon panah kiri/kanan digambar sebagai **vektor** (`ChevronLeft`/`ChevronRight`) dan keduanya menjadi tombol klik yang benar-benar mengganti tab. Sebelumnya teks Unicode `◀ ▶` sering hilang pada instalasi Linux tanpa font yang mencakup U+25C0/U+25B6, sehingga indikator arrow tidak terlihat; tombol vektor juga memberi fallback bagi overlay click-through yang tidak bisa menerima fokus keyboard. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-024).
- `src/platform/mod.rs`: Unit test `decode_album_art_accepts_common_large_cover_resolutions` mendekode cover JPEG `1280×720` (resolusi `maxresdefault` YouTube Music) dan memastikan hasil resize `128×72`, mencegah regresi batas decode.

### Changed (v0.1.6)

- `src/ui/mod.rs`: Layout tab **Music** expanded dikembalikan ke versi sebelum v0.1.5 (kolom cover/now-playing + lirik seperti semula, termasuk thumbnail `≈118 px`) pada Linux **dan** Windows; perubahan lyrics-full-width v0.1.5 dibatalkan.
- `src/ui/mod.rs`: Cache tekstur album art tidak lagi memakai `Arc::as_ptr` sebagai kunci. Kini memakai hash konten (`width`, `height`, `rgba`, `dominant_rgb`) sehingga `Arc` baru dari IPC tidak memicu `tex.set()` berlebih, sekaligus mencegah alamat allocator yang dipakai ulang menyisakan cover lama saat piksel berubah.
- `src/ui/settings.rs`, `Cargo.toml`, `Cargo.lock`: Jendela Settings tetap memakai `eframe` + `Renderer::Glow` (grup kiri/konten kanan dipertahankan), tetapi kini **mencoba software GL lebih dulu** (`egui_glow::HardwareAcceleration::Off`) sebelum jatuh ke `Preferred`. Upaya ini dapat menghindari pemuatan tumpakan driver GPU/LLVM ratusan MB bila konfigurasi EGL software tersedia; bila tidak, launcher secara otomatis memulai ulang dengan GL normal agar UI tetap berfungsi.

### Fixed (v0.1.6)

- `src/platform/mod.rs`: Batas decode album art dikembalikan ke `2048 px` / `16 MiB` (dari `1024 px` / `8 MiB` di v0.1.5). Batas v0.1.5 menolak cover populer `1280×720`/`1400×1400`, dan kegagalan decode tersebut dikunci oleh `ArtCacheState` sehingga cover tidak pernah muncul. Penghematan RAM tetap dipertahankan lewat output `128×128`, bukan dengan menolak sumber gambar yang valid.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi dinaikkan ke **`0.1.6`**; AppImage + tar.gz Linux dibangun ulang. `cargo test` **66 passed**, clippy Linux + Windows-target bersih. Installer NSIS final tetap diproduksi CI Windows (`makensis` tidak tersedia di host Linux).

### Measured (v0.1.6)

- Jendela Settings (host Linux dengan driver NVIDIA): `VmRSS 102,8 MB`, `Pss 69,0 MB`; `RssFile ≈80 MB` didominasi `libnvidia-gpucomp`, `libnvidia-eglcore`, dan `libLLVM`. Launcher sendiri hanya `VmRSS 7,3 MB`. Percobaan software GL ditolak glutin pada host ini (`failed to find a matching configuration`, juga dengan `LIBGL_ALWAYS_SOFTWARE=1`/`EGL_PLATFORM=x11|wayland|surfaceless`), sehingga fallback hardware dipakai. **Target `< 10 MB` belum tercapai** untuk jendela Settings; lihat ADR-024.
- Overlay Wayland/Windows tidak berubah dari pengukuran v0.1.5 (`VmRSS ≈23,2 MB`); RSS Windows tidak dapat diukur dari host Linux.

### Added

- `src/sysmon.rs`, `src/battery.rs`, `src/main.rs`, `src/platform/wayland_layer.rs`, `src/platform/windows_overlay.rs`: Mengaktifkan tab **System** di Linux dengan sampler asli tanpa crate baru — CPU dari `/proc/stat`, RAM dari `/proc/meminfo`, disk dari `/proc/diskstats` + `df -P -k /`, jaringan dari `/proc/net/dev`, dan baterai dari `/sys/class/power_supply`. Worker sysmon kini di-spawn pada jalur Wayland layer-shell, klien `--ui` X11/eframe, dan klien overlay Windows; perubahan config diteruskan lewat `SysmonCommand::SetConfig`. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-023).
- `src/ui/mod.rs`, `src/ui/settings.rs`, `src/platform/windows_overlay.rs`, `src/platform/wayland_layer.rs`: Navigasi tab memakai **Left/Right** (wrap mengikuti modul aktif) di Linux dan Windows. Header strip tab atas dihapus — tersisa label tab aktif, tombol ganti Notch/Island, dan tombol Settings. Windows mendaftarkan hotkey `VK_LEFT`/`VK_RIGHT` hanya saat overlay tampil; Wayland memakai `KeyboardInteractivity::OnDemand` + `KeyboardHandler` (`// shortcut: ...`) karena winit/eframe belum memiliki keyboard grab `wlr-layer-shell`.

### Changed

- `src/ui/mod.rs`: Layout Home dipangkas (baris lebih rapat, timezoneBadge benar-benar terpusat), tab Music expanded kini **lyrics-first** (strip now-playing + playbar + kontrol transport, lalu lirik full-width), dan Settings memakai panel kiri bergrup (`Display`, `Lyrics`, `Features`, `System`, `Tasks`, `Calendar`) + panel kanan scrollable alih-alih strip horizontal.
- `src/ui/soft_render.rs`, `src/platform/mod.rs`, `src/platform/linux.rs`, `Cargo.toml`: Optimasi RAM — delta tekstur `egui` dipindahkan (`Arc::get_mut`/`mem::take`) alih-alih di-clone, batas decode album art diturunkan ke `1024 px` / `8 MiB` dengan resize `128×128`, serta fitur `xkbcommon` ditambahkan ke `smithay-client-toolkit` agar keyboard Wayland bekerja.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Versi dinaikkan ke **`0.1.5`**; AppImage + tar.gz Linux dibangun ulang. `cargo test` **65 passed**, `cargo clippy --all-targets -- -D warnings` bersih, dan cross-check `x86_64-pc-windows-gnu` bersih. Installer NSIS final tidak dibangun di host Linux (tanpa `makensis`); workflow CI Windows yang memproduksinya. Lihat [[00 - Dashboard Proyek]].

### Fixed

- `src/sysmon.rs`, `src/battery.rs`: Mengoreksi kalkulasi CPU Linux agar total jiffies mencakup idle (sebelum ini idle dikurangi dua kali) dan konversi satuan baterai sysfs (`time_to_empty_now` µs → detik; `energy_now / power_now` jam → detik), dengan unit test baru.
- `src/ui/soft_render.rs`, `src/platform/wayland_layer.rs`, `src/platform/windows_overlay.rs`: Membuat `full_output` mutable agar delta tekstur dapat dikonsumsi tanpa duplikasi, termasuk jalur taskbar Windows.

### Measured

- Overlay Wayland **`--demo`**: `VmRSS 23,2 MB`, `Pss 19,8 MB`, `RssAnon 7,6 MB` (proses overlay tunggal). Target batas maksimal `30 MB` tercapai, tetapi target idle `< 10 MB` **belum** tercapai karena halaman kode/font `egui` yang tetap tersentuh.
- Fallback X11/eframe: daemon `VmRSS 15,6 MB` + child UI `VmRSS 83,8 MB` (`Pss` gabungan ≈ `84,4 MB`). Jalur ini **belum** memenuhi batas `30 MB`; belum ada overlay X11 berbasis `SoftCanvas` yang setara dengan Wayland/Windows. RSS Windows tidak dapat diukur dari host Linux; penghematan alokasi diterapkan tetapi tidak diklaim terverifikasi.

### Fixed

- `.github/workflows/release.yml`, `installer/windows/installer.nsi`: Memperbaiki kegagalan berulang job Windows pada langkah **Build Windows release binary, zip & installer**. Penyebabnya: NSIS me-resolve path relatif terhadap file skrip (`installer/windows/`), sehingga `File "dist\staging\..."` tidak menemukan binary yang sudah disalin di `dist\staging\` repository root. Workflow kini meneruskan `STAGING_DIR`/`OUT_FILE` **absolut** ke `makensis`, dan default skrip diubah menjadi `..\..\dist\staging` / `..\..\dist\...-setup.exe`. Skrip divalidasi dengan `makensis` (payload dummy) di container Docker; installer asli tetap dibangun CI Windows. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]] (ADR-022).
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/windows_overlay.rs`, `src/main.rs`: Membersihkan error `clippy` khusus target Windows yang muncul setelah refactor overlay berlapis: helper eframe lama (`overlay_resizable`, `prepare_overlay_rules`, `set_overlay_visible`) ditandai `dead_code` untuk Windows, inisialisasi `BITMAPINFOHEADER` memakai struct literal alih-alih assignment `Default::default()`, parser `--ipc-addr`/`--ipc-sock` memakai `Option::or`, dan `return` berlebih di cabang layered client dihapus. `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings` kini bersih.
- `Cargo.toml`, `Cargo.lock`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/PKGBUILD`, `packaging/aur/.SRCINFO`, `src/platform/linux.rs`, `src/lyrics.rs`, `README.md`: Menaikkan versi menjadi **`0.1.4`** sesuai alur rilis standar dan membangun ulang `dist/Liebert_Dynamic_Island-0.1.4-x86_64.AppImage` (7,0 MB) + `dist/liebert-dynamic-island-linux-x86_64.tar.gz` (6,8 MB). `cargo test` 62 passed dan `cargo clippy --all-targets -- -D warnings` bersih.

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
