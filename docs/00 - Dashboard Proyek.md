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
- [ ] **M3 — Notification Bridge:** Listener Windows & state/queue selesai (7 unit tests pass); build release Linux lolos & spike D-Bus OK, menunggu verifikasi runtime listener Linux (daemon notifikasi lain masih aktif)
- [ ] **M4 — Tray, Config & Release:** System tray & `config.toml` selesai; menunggu `README.md`, uji manual, & benchmark RAM `--release`

## Progres Tambahan (Terverifikasi)
- [x] **Text Alignment di Settings:** Tombol `Left` / `Center` / `Right` (`src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`).
- [x] **CPU Software Renderer (`SoftCanvas`), Fix Hyprland `wl_shm`, & Optimasi RAM:** Konflik merge PR #15 diselesaikan (`48 passed`, clippy bersih); overlay Linux Wayland pindah ke `wl_shm` + pemulihan `LayerSurface` otomatis + watcher fullscreen; Windows memakai `UpdateLayeredWindow` DIB (`src/platform/windows_overlay.rs`) + `--settings` on-demand; daemon headless Linux terukur `Pss 8.5 MB` / `VmRSS 11.2 MB`. Lihat [[04 - Changelog & Dev Log]].
- [x] **Distribusi v0.1.2:** `Liebert_Dynamic_Island-0.1.2-x86_64.AppImage` + `.tar.gz` Linux terverifikasi lokal, paket AUR (`PKGBUILD` + `.SRCINFO` v0.1.2), dan skrip installer NSIS Windows (`installer/windows/installer.nsi` v0.1.2) siap di CI (host Linux tidak memiliki `makensis`/`mingw`).
- [x] **Fix Fetch Lirik & Sync MPRIS2 Linux (v0.1.1):** Multi-player selection (`Playing` > `Paused`), parsing `i64`/`u64` `mpris:length` & `Position` via `Properties.Get`, `SetPosition`, kalibrasi timestamp sebelum/sesudah download cover art, normalisasi judul/artis multi-kandidat, serta retry otomatis saat durasi tersedia (`cargo test` 53 passed).
- [x] **Fix Album Cover Linux (v0.1.2):** Cache per-`(key, art_url)` dengan retry saat `mpris:artUrl` terlambat/gagal sementara, dekoder `data:image/...;base64,...`, `file://localhost/...` + path polos, fallback `open.spotify.com` → `i.scdn.co`, format `bmp`/`gif`, dan fix `sync_album_art_texture`.
