---
type: dashboard
status: in-progress
deadline: 2026-12-09
tags:
  - project
  - liebert-dynamic-island
  - mvp
---

# 00 - Dashboard Proyek: Liebert Dynamic Island

## Ringkasan Tujuan Proyek
Membangun overlay **Dynamic Island** yang ringan (< 25 MB RAM idle), instan (< 500 ms startup), dan non-intrusif di bagian top-center layar untuk **Windows 10/11** dan **Linux (X11/Wayland)** menggunakan **Rust native**. Fokus MVP (1–2 minggu) adalah menangkap dan menampilkan notifikasi sistem secara real-time.

## Navigasi Dokumentasi Utama
- [[PRD-LiebertDynamicIsland-MVP]] — Product Requirements Document (MVP v1.0)
- [[01 - Arsitektur & Spesifikasi]] — Tech stack, struktur kode, dan alur sistem
- [[02 - Daftar Tugas & Backlog]] — Daftar tugas P0, P1, P2
- [[03 - Catatan Rapat & Keputusan]] — Architecture Decision Records (ADR)
- [[04 - Changelog & Dev Log]] — Riwayat perubahan kode kronologis

## Milestones Utama (MVP)
- [x] Inisialisasi PRD MVP ([[PRD-LiebertDynamicIsland-MVP]]) & struktur dokumentasi Obsidian
- [ ] **M1 — Technical Spike:** Windows WinRT, renderer `eframe`, koneksi D-Bus Linux, & overlay Hyprland (Wayland) terverifikasi; menunggu uji compositor non-Hyprland/X11
- [ ] **M2 — Core Overlay:** Implementasi `src/ui/mod.rs` & positioning Hyprland terverifikasi live (`140×30` → `340×58` top-center); menunggu uji visual penuh & DPI scaling Linux
- [ ] **M3 — Notification Bridge:** Listener Windows & state/queue selesai (7 unit tests pass); build release Linux lolos & spike D-Bus OK. Sejak v0.1.8, bila nama `org.freedesktop.Notifications` dipegang daemon lain, aplikasi otomatis memakai fallback **monitor pasif** (`BecomeMonitor`) sehingga notifikasi tetap masuk; verifikasi end-to-end pada desktop dengan notifikasi nyata masih pending.
- [ ] **M4 — Tray, Config & Release:** System tray & `config.toml` selesai; menunggu `README.md`, uji manual, & benchmark RAM `--release`

## Progres Tambahan (Terverifikasi)
- [x] **Text Alignment di Settings:** Tombol `Left` / `Center` / `Right` (`src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`).
- [x] **CPU Software Renderer (`SoftCanvas`), Fix Hyprland `wl_shm`, & Optimasi RAM:** Konflik merge PR #15 diselesaikan (`48 passed`, clippy bersih); overlay Linux Wayland pindah ke `wl_shm` + pemulihan `LayerSurface` otomatis + watcher fullscreen; Windows memakai `UpdateLayeredWindow` DIB (`src/platform/windows_overlay.rs`) + `--settings` on-demand; daemon headless Linux terukur `Pss 8.5 MB` / `VmRSS 11.2 MB`. Lihat [[04 - Changelog & Dev Log]].
- [x] **Distribusi & Pipeline Rilis v0.1.4:** `Liebert_Dynamic_Island-0.1.4-x86_64.AppImage` + `.tar.gz` Linux dibangun ulang dan terverifikasi lokal, paket AUR (`PKGBUILD` + `.SRCINFO` v0.1.4), NSIS Windows kini memakai `STAGING_DIR`/`OUT_FILE` absolut di CI (path rel NSIS diperbaiki, divalidasi dengan `makensis` pada payload dummy), dan lint Windows-target bersih. Lihat [[04 - Changelog & Dev Log]] (ADR-022).
- [x] **Tab System Linux, Navigasi Arrow, Layout Baru & Rilis v0.1.5:** Tab System kini membaca `/proc` + sysfs dan baterai tanpa crate baru (`src/sysmon.rs`, `src/battery.rs`), strip tab atas dihapus dengan navigasi Left/Right di Wayland/Windows, Home/Music/Settings ditata ulang, dan alokasi tekstur album art dipangkas. Verifikasi: `cargo test` 65 passed, clippy Linux + Windows-target bersih, AppImage/tar.gz v0.1.5 dibangun. Pengukuran RSS Wayland `23,2 MB` (Pss `19,8 MB`) berada di bawah batas `30 MB`, tetapi target idle `< 10 MB` dan fallback X11 (`83,8 MB` child UI) belum tercapai; lihat [[04 - Changelog & Dev Log]] (ADR-023).
- [x] **Perbaikan Arrow, Layout Music, Cover Linux & Optimasi Settings v0.1.6:** Indikator arrow kini berupa tombol vektor klik (`src/ui/icons.rs`), layout Music dikembalikan ke versi sebelum v0.1.5, batas decode cover dikembalikan ke `2048 px`/`16 MiB` dan cache tekstur memakai hash konten. Settings mencoba software GL lalu fallback hardware. `cargo test` 66 passed, clippy Linux + Windows-target bersih, AppImage/tar.gz v0.1.6 dibangun. Jendela Settings masih `VmRSS 102,8 MB` di host NVIDIA (target `< 10 MB` belum tercapai); lihat [[04 - Changelog & Dev Log]] (ADR-024).
- [x] **Header Expanded & Status Baterai v0.1.7:** Header hanya label tab + Settings; chevron navigasi dipindah ke tengah tepi kiri/kanan kartu. Settings tetap bergrup kiri/konten kanan. System monitor Linux diverifikasi dengan sampler `/proc` live, dan baterai tanpa perangkat menampilkan `No battery detected`. `cargo test` 67 passed, clippy Linux + Windows-target bersih, AppImage/tar.gz v0.1.7 dibangun. Lihat [[04 - Changelog & Dev Log]] (ADR-025).
- [x] **Installer Windows v0.1.7 dibangun lokal:** Cross-compile `x86_64-pc-windows-gnu` + `makensis` di container Docker menghasilkan `dist/liebert-dynamic-island-windows-x86_64-setup.exe` (4,06 MB) dan zip portabel 5,0 MB; payload diverifikasi dengan `7z l`. Perintah build untuk host Linux didokumentasikan di `README.md`. Lihat [[04 - Changelog & Dev Log]].
- [x] **Autostart, Tab Battery, Chart Donut & Notifikasi Linux v0.1.8:** Autostart per-user (HKCU Run di Windows, XDG `.desktop` di Linux), tab Battery dengan animasi charging + `No battery detected`, chart donut CPU/RAM/Disk, ruang arrow expanded, Home/Tasks dirapikan, settings bergrup, deteksi disk Linux lintas mount, dan fallback monitor pasif D-Bus saat daemon notifikasi lain aktif. `cargo test` 72 passed, clippy Linux + Windows-target bersih, AppImage/tar.gz + installer/zip v0.1.8 dibangun. Target RAM `< 10 MB` **belum** tercapai (Wayland `VmRSS 63,1 MB`); lihat [[04 - Changelog & Dev Log]] (ADR-026).
- [x] **Idle Battery & Layout Polish v0.1.9:** Mode idle Battery di Settings (Linux/Windows), konten expanded diberi gutter kiri/kanan sesuai arrow side-nav, dan Home dirapikan minimalis. `cargo test` 72 passed, clippy bersih; AppImage Linux dibangun. Lihat [[04 - Changelog & Dev Log]] (ADR-027).
- [x] **Fix Fetch Lirik & Sync MPRIS2 Linux (v0.1.1):** Multi-player selection (`Playing` > `Paused`), parsing `i64`/`u64` `mpris:length` & `Position` via `Properties.Get`, `SetPosition`, kalibrasi timestamp sebelum/sesudah download cover art, normalisasi judul/artis multi-kandidat, serta retry otomatis saat durasi tersedia (`cargo test` 53 passed).
- [x] **Fix Album Cover Linux (v0.1.2):** Cache per-`(key, art_url)` dengan retry saat `mpris:artUrl` terlambat/gagal sementara, dekoder `data:image/...;base64,...`, `file://localhost/...` + path polos, fallback `open.spotify.com` → `i.scdn.co`, format `bmp`/`gif`, dan fix `sync_album_art_texture`.
- [x] **Clock Auto-Width & Centering, Center Align Pill/Expand, Bold Lyrics, & Blurred Full-Lyrics (v0.1.3):** Lebar idle clock adaptif + rata tengah simetris, perbaikan `TextAlign::Center` lintas pill & expand via `LayoutJob::halign` + `galley_draw_pos`, font `LyricsBold` lintas Linux/Windows, serta daftar seluruh lirik scrollable dengan baris sebelum/sesudah ber-blur di tab Music (`cargo test` 62 passed, `cargo clippy` bersih).
