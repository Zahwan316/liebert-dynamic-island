# AGENTS.md — Liebert Dynamic Island Collaboration Guide

Panduan kerja untuk manusia dan AI coding agent di repository **Liebert Dynamic Island**.

## 1. Project Overview & Scope
- **Tujuan:** Overlay Dynamic Island native Rust yang ringan dan non-intrusif di top-center layar untuk **Windows 10/11** dan **Linux (X11/Wayland)** (`edition = "2024"`).
- **Sumber Kebenaran Produk & Progres:**
  - `docs/PRD-LiebertDynamicIsland-MVP.md` — Cakupan MVP (P0 = System Notifications + Overlay + Interaksi + Tray/Config; P1/P2 di luar MVP). Jangan hapus atau timpa file PRD ini.
  - `docs/00 - Dashboard Proyek.md` — Ringkasan tujuan, navigasi vault Obsidian, dan milestones M1–M4.

## 2. Commands
```sh
cargo test                                 # Unit tests (config & state machine)
cargo clippy --all-targets -- -D warnings  # Lint wajib bersih sebelum selesai
cargo run -- --spike-notifications         # Uji akses listener notifikasi OS
cargo run -- --demo                        # Jalankan UI dengan notifikasi demo (1s delay)
cargo build --release                      # Binary release (lto = "thin", strip = true)
```

## 3. Module Boundaries
- `src/state.rs`: Pure OS-agnostic `NotificationItem`, state machine `IslandState` (`Idle` ↔ `Compact` ↔ `Expanded`), queue, deduplikasi ID, dan timer. Jangan masukkan tipe `windows`, `zbus`, atau `egui` ke sini.
- `src/platform/{mod,windows,linux}.rs`: Isolasi seluruh API spesifik OS (`windows 0.62` WinRT/Win32 dan `zbus 6.5` D-Bus) di balik facade `PlatformEvent` di `src/platform/mod.rs`.
- `src/ui/mod.rs`: Renderer pill transparan berbasis `eframe 0.37` (`logic` + `ui`, event-driven `request_repaint_after`).
- `src/tray.rs`: System tray (`tray-icon 0.27`) dan polling `MenuEvent`.
- `src/config.rs`: Persistensi `config.toml` lokal (`%APPDATA%` / `~/.config`) dengan fallback `Default` aman saat file hilang/rusak.

## 4. Engineering Standards & API Gotchas
- **Smallest Complete Change:** Hindari abstraksi berlebih, wrapper kosong, dependensi baru untuk hal kecil, Electron/WebView, telemetri, hak Administrator/root, atau log isi notifikasi persisten ke disk.
- **Target Performa MVP:** RAM idle `< 25 MB` (`< 50 MB` expanded), CPU idle `< 0.5%`, latency notifikasi `< 150 ms`, animasi `>= 60 FPS`.
- **Verifikasi Wajib:** Setiap perubahan kode Rust harus lolos `cargo test` dan `cargo clippy --all-targets -- -D warnings` setelah edit terakhir.
- **API Gotchas:**
  - `eframe 0.37`: Implementasi `eframe::App` memakai `fn ui(&mut self, ui: &mut egui::Ui, ...)` dan `fn logic(&mut self, ctx: &egui::Context, ...)` (bukan `update`), dengan `Renderer::Glow`.
  - `windows 0.62`: `IAsyncOperation` di-await secara sinkron di background thread dengan `.join()` (bukan `.get()`).
  - `zbus 6.5`: Menjalankan async D-Bus server via `zbus::block_on`; pada Linux `org.freedesktop.Notifications` hanya mengizinkan satu daemon aktif per session.
  - **Shortcut Aktif:** Wayland `wlr-layer-shell` belum didukung `winit`/`eframe` (`src/platform/linux.rs`). Kompromi sementara wajib diberi komentar `// shortcut: <the limit>, <when to upgrade>`.

## 5. Mandatory Obsidian Docs Sync (`/docs`)
Root vault Obsidian proyek berada di `/docs` (`.obsidian/` tetap di `.gitignore`). Setiap modifikasi kode atau konfigurasi **wajib** diikuti pembaruan sebelum selesai:
1. **`docs/04 - Changelog & Dev Log.md`:** Tambahkan entri terbaru di paling atas di bawah `## YYYY-MM-DD` menggunakan sub-heading `### Added` / `### Changed` / `### Fixed` / `### Removed`, cantumkan path file dalam inline code dan wikilink ke catatan terkait.
2. **`docs/02 - Daftar Tugas & Backlog.md` & `docs/00 - Dashboard Proyek.md`:** Ubah `- [ ]` menjadi `- [x]` untuk tugas atau milestone yang benar-benar selesai dan terverifikasi.
3. **`docs/01 - Arsitektur & Spesifikasi.md` & `docs/03 - Catatan Rapat & Keputusan.md`:** Perbarui bila struktur modul, skema data, atau keputusan teknis berubah.
4. **Konfirmasi Akhir:** Sebutkan daftar catatan `/docs` yang diperbarui di akhir respons.

## 6. Standing Release & Packaging Workflow (User Preference)
- Setiap perubahan selesai pada suatu request, **biasakan otomatis**:
  1. **Naikkan versi** (`X.Y.Z` → naikkan angka belakang `Z + 1`; jika `Z >= 10`, reset `Z = 0` dan naikkan angka tengah `Y += 1`; jika `Y >= 10`, reset `Y = 0` dan naikkan angka depan `X += 1`) pada `Cargo.toml`, `scripts/build-appimage.sh`, `installer/windows/installer.nsi`, `packaging/aur/{PKGBUILD,.SRCINFO}`, string versi internal (`src/platform/linux.rs`, `src/lyrics.rs`), dan `README.md` (plus `Cargo.lock` via Cargo).
  2. **Build AppImage Linux** (`./scripts/build-linux-release.sh` → `dist/*.AppImage` & `dist/*.tar.gz`) serta **Windows Installer** (`installer/windows/installer.nsi` — bangun langsung bila `makensis`/Windows toolchain tersedia pada host, atau laporkan jujur bila host Linux tidak memiliki toolchain Windows/NSIS sementara workflow CI `.github/workflows/release.yml` menanganinya).
  3. **Sinkronkan `/docs`**, buat commit bersih, dan **auto-push ke branch yang sedang aktif** (`git push origin <current-branch>`).

## 7. Standing RAM Budget Task (User Preference)
- **Setiap** request perubahan kode, jalankan juga task penurunan memori: target `< 10 MB` RAM untuk **overlay Dynamic Island** dan **jendela Settings**, di **Windows dan Linux**.
- Lakukan optimasi nyata (mis. mengurangi font/atlas, cache, alokasi per-frame, atau beban renderer), ukur RSS/Pss bila memungkinkan, dan laporkan hasilnya **jujur** — bila target belum tercapai, sebutkan pengukuran dan penghalangnya serta catat sebagai utang teknis di `/docs`. Jangan mengklaim `< 10 MB` tanpa pengukuran.
