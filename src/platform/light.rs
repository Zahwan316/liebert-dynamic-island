use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FontFamilyTarget {
    Proportional,
    #[cfg_attr(windows, allow(dead_code))]
    Monospace,
    Both,
    Bold,
    All,
}

#[derive(Debug, Clone)]
pub struct FontFileSpec {
    pub name: String,
    pub path: String,
    pub family: FontFamilyTarget,
    pub face_indices: &'static [u32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFont {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CjkScript {
    JapaneseOrHan,
    Korean,
}

/// Tunes glibc allocator arenas & trims pages on Linux to minimize background RAM footprint.
pub fn configure_low_ram_allocator() {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
        libc::mallopt(libc::M_MMAP_THRESHOLD, 64 * 1024);
    }
}

#[allow(dead_code)]
pub fn trim_heap() {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Resolves a sibling helper binary (`liebert-settings`, `liebert-x11`, `liebert-dynamic-island`)
/// located alongside the running executable.
#[allow(dead_code)]
pub fn sibling_binary_path(bin_base_name: &str) -> Option<PathBuf> {
    let cur = std::env::current_exe().ok()?;
    #[cfg(windows)]
    let file_name = format!("{bin_base_name}.exe");
    #[cfg(not(windows))]
    let file_name = bin_base_name.to_owned();
    let cand = cur.with_file_name(file_name);
    cand.is_file().then_some(cand)
}

pub fn collect_fonts_from_dirs(dirs: &[PathBuf]) -> Vec<InstalledFont> {
    let mut seen = std::collections::HashSet::new();
    let mut fonts = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let sub = collect_fonts_from_dirs(&[path]);
                for f in sub {
                    if seen.insert(f.name.to_ascii_lowercase()) {
                        fonts.push(f);
                    }
                }
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let display = stem.replace(['_', '-'], " ").trim().to_owned();
            if display.is_empty() {
                continue;
            }
            if seen.insert(display.to_ascii_lowercase()) {
                fonts.push(InstalledFont {
                    name: display,
                    path: path.to_string_lossy().into_owned(),
                });
            }
        }
    }
    fonts.sort_by_key(|a| a.name.to_ascii_lowercase());
    fonts
}

pub fn list_installed_fonts() -> Vec<InstalledFont> {
    #[cfg(windows)]
    {
        let mut dirs = vec![PathBuf::from(r"C:\Windows\Fonts")];
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            dirs.push(
                PathBuf::from(local_app_data)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts"),
            );
        }
        collect_fonts_from_dirs(&dirs)
    }

    #[cfg(target_os = "linux")]
    {
        let mut dirs = vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
        ];
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/fonts"));
        }
        collect_fonts_from_dirs(&dirs)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    Vec::new()
}

#[cfg(target_os = "linux")]
fn first_existing_path(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|p| Path::new(p).is_file())
        .map(|p| (*p).to_owned())
}

#[cfg(target_os = "linux")]
fn fc_match_file(pattern: &str) -> Option<String> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!path.is_empty() && Path::new(&path).is_file()).then_some(path)
}

pub fn base_fallback_fonts() -> Vec<FontFileSpec> {
    #[cfg(windows)]
    {
        let mut specs = vec![FontFileSpec {
            name: "win_segoe_ui".to_owned(),
            path: "C:\\Windows\\Fonts\\segoeui.ttf".to_owned(),
            family: FontFamilyTarget::Proportional,
            face_indices: &[0],
        }];
        if Path::new("C:\\Windows\\Fonts\\consola.ttf").is_file() {
            specs.push(FontFileSpec {
                name: "win_consolas".to_owned(),
                path: "C:\\Windows\\Fonts\\consola.ttf".to_owned(),
                family: FontFamilyTarget::Monospace,
                face_indices: &[0],
            });
        } else {
            specs[0].family = FontFamilyTarget::Both;
        }
        specs
    }

    #[cfg(target_os = "linux")]
    {
        let mut specs = Vec::new();
        if let Some(path) = first_existing_path(&[
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ])
        .or_else(|| fc_match_file("sans-serif"))
        {
            specs.push(FontFileSpec {
                name: "linux_sans".to_owned(),
                path,
                family: FontFamilyTarget::Proportional,
                face_indices: &[0],
            });
        }
        if let Some(path) = first_existing_path(&[
            "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        ])
        .or_else(|| fc_match_file("monospace"))
        {
            specs.push(FontFileSpec {
                name: "linux_mono".to_owned(),
                path,
                family: FontFamilyTarget::Monospace,
                face_indices: &[0],
            });
        }
        specs
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    Vec::new()
}

pub fn bold_fallback_fonts() -> Vec<FontFileSpec> {
    #[cfg(windows)]
    {
        vec![
            FontFileSpec {
                name: "win_segoe_ui_bold".to_owned(),
                path: "C:\\Windows\\Fonts\\segoeuib.ttf".to_owned(),
                family: FontFamilyTarget::Bold,
                face_indices: &[0],
            },
            FontFileSpec {
                name: "win_arial_bold".to_owned(),
                path: "C:\\Windows\\Fonts\\arialbd.ttf".to_owned(),
                family: FontFamilyTarget::Bold,
                face_indices: &[0],
            },
        ]
    }

    #[cfg(target_os = "linux")]
    {
        let mut specs = Vec::new();
        if let Some(path) = first_existing_path(&[
            "/usr/share/fonts/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
        ])
        .or_else(|| fc_match_file("sans-serif:bold"))
        {
            specs.push(FontFileSpec {
                name: "linux_sans_bold".to_owned(),
                path,
                family: FontFamilyTarget::Bold,
                face_indices: &[0],
            });
        }
        specs
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    Vec::new()
}

#[allow(dead_code)]
pub fn cjk_fallback_fonts_for(script: CjkScript) -> Vec<FontFileSpec> {
    #[cfg(windows)]
    return match script {
        CjkScript::JapaneseOrHan => vec![FontFileSpec {
            name: "win_msgothic".to_owned(),
            path: "C:\\Windows\\Fonts\\msgothic.ttc".to_owned(),
            family: FontFamilyTarget::All,
            face_indices: &[0],
        }],
        CjkScript::Korean => vec![
            FontFileSpec {
                name: "win_malgun_bold".to_owned(),
                path: "C:\\Windows\\Fonts\\malgunbd.ttf".to_owned(),
                family: FontFamilyTarget::Bold,
                face_indices: &[0],
            },
            FontFileSpec {
                name: "win_malgun".to_owned(),
                path: "C:\\Windows\\Fonts\\malgun.ttf".to_owned(),
                family: FontFamilyTarget::All,
                face_indices: &[0],
            },
        ],
    };

    #[cfg(target_os = "linux")]
    {
        let (lang, bold_lang, ttc_face, suffix): (&str, &str, &'static [u32], &str) = match script {
            CjkScript::JapaneseOrHan => (":lang=ja", ":lang=ja:bold", &[0], "jp"),
            CjkScript::Korean => (":lang=ko", ":lang=ko:bold", &[1], "kr"),
        };
        let mut specs = Vec::new();
        if let Some(path) = first_existing_path(&[
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Bold.ttc",
        ])
        .or_else(|| fc_match_file(bold_lang))
        {
            let faces = if path.ends_with(".ttc") {
                ttc_face
            } else {
                &[0][..]
            };
            specs.push(FontFileSpec {
                name: format!("linux_noto_cjk_{suffix}_bold"),
                path,
                family: FontFamilyTarget::Bold,
                face_indices: faces,
            });
        }
        if let Some(path) = first_existing_path(&[
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        ])
        .or_else(|| fc_match_file(lang))
        {
            let faces = if path.ends_with(".ttc") {
                ttc_face
            } else {
                &[0][..]
            };
            specs.push(FontFileSpec {
                name: format!("linux_noto_cjk_{suffix}"),
                path,
                family: FontFamilyTarget::All,
                face_indices: faces,
            });
        }
        specs
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = script;
        Vec::new()
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_autostart_command(exe: &Path) -> String {
    format!("\"{}\"", exe.to_string_lossy())
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn desktop_autostart_entry(exec: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Liebert Dynamic Island\n\
         Comment=Lightweight native Rust Dynamic Island notification overlay\n\
         Exec={exec}\n\
         Icon=liebert-dynamic-island\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(target_os = "linux")]
pub(crate) fn escape_desktop_exec(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{escaped}\"")
}

fn resolve_main_executable() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    if let Some(main_sibling) = sibling_binary_path("liebert-dynamic-island") {
        return Ok(main_sibling);
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Enable or disable per-user start-with-OS. Never requires Administrator/root.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
        use windows::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegCreateKeyW,
            RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
        };
        use windows::core::HSTRING;

        const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
        const VALUE_NAME: &str = "LiebertDynamicIsland";

        let subkey = HSTRING::from(RUN_KEY);
        let value_name = HSTRING::from(VALUE_NAME);
        let mut hkey = HKEY::default();

        if !enabled {
            let status = unsafe {
                RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_SET_VALUE, &mut hkey)
            };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            if status.0 != 0 {
                return Err(format!("RegOpenKeyExW failed: {}", status.0));
            }
            let status = unsafe { RegDeleteValueW(hkey, &value_name) };
            let _ = unsafe { RegCloseKey(hkey) };
            if status != ERROR_FILE_NOT_FOUND && status.0 != 0 {
                return Err(format!("RegDeleteValueW failed: {}", status.0));
            }
            return Ok(());
        }

        let exe = resolve_main_executable()?;
        let command = windows_autostart_command(&exe);
        let mut units: Vec<u16> = command.encode_utf16().collect();
        units.push(0);
        let bytes: Vec<u8> = units.into_iter().flat_map(u16::to_le_bytes).collect();

        let status = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, &subkey, &mut hkey) };
        if status.0 != 0 {
            return Err(format!("RegCreateKeyW failed: {}", status.0));
        }
        let status = unsafe { RegSetValueExW(hkey, &value_name, None, REG_SZ, Some(&bytes)) };
        let _ = unsafe { RegCloseKey(hkey) };
        if status.0 != 0 {
            return Err(format!("RegSetValueExW failed: {}", status.0));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let config_root = directories::BaseDirs::new()
            .map(|dirs| dirs.config_dir().to_path_buf())
            .ok_or_else(|| "Cannot resolve XDG config directory".to_owned())?;
        let dir = config_root.join("autostart");
        let file = dir.join("liebert-dynamic-island.desktop");

        if !enabled {
            match std::fs::remove_file(&file) {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e.to_string()),
            }
        }

        let exe = resolve_main_executable()?;
        let entry = desktop_autostart_entry(&escape_desktop_exec(&exe));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(&file, entry).map_err(|e| e.to_string())
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = enabled;
        Err("Autostart is only supported on Windows and Linux".to_owned())
    }
}

#[allow(dead_code)]
pub fn bring_window_to_front(window_title: &str) -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, HWND_NOTOPMOST, HWND_TOPMOST, SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE,
            SetForegroundWindow, SetWindowPos, ShowWindow,
        };
        use windows::core::HSTRING;

        let title = HSTRING::from(window_title);
        let Ok(hwnd) = (unsafe { FindWindowW(None, &title) }) else {
            return false;
        };
        if hwnd.is_invalid() {
            return false;
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
            let _ = SetForegroundWindow(hwnd);
        }
        true
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        use std::time::Duration;

        let Some(sig) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") else {
            return true;
        };
        let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
            return true;
        };
        let mut sock_path = PathBuf::from(runtime);
        sock_path.push("hypr");
        sock_path.push(sig);
        sock_path.push(".socket.sock");
        if !sock_path.exists() {
            return true;
        }
        let send_cmd = |cmd: &str| -> Option<String> {
            let mut stream = UnixStream::connect(&sock_path).ok()?;
            let t = Some(Duration::from_millis(100));
            let _ = stream.set_read_timeout(t);
            let _ = stream.set_write_timeout(t);
            stream.write_all(cmd.as_bytes()).ok()?;
            let mut out = String::new();
            stream.read_to_string(&mut out).ok()?;
            Some(out)
        };
        let escaped = window_title.replace('\\', "\\\\").replace('"', "\\\"");
        let lua = format!("/eval hl.dispatch(hl.dsp.focus({{window=\"title:^{escaped}$\"}}))");
        if let Some(resp) = send_cmd(&lua)
            && resp.trim() == "ok"
        {
            return true;
        }
        let legacy = format!("/dispatch focuswindow title:^({window_title})$");
        let _ = send_cmd(&legacy);
        true
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = window_title;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_autostart_command_quotes_paths_with_spaces() {
        let cmd = windows_autostart_command(Path::new(
            r"C:\Program Files\Liebert\liebert-dynamic-island.exe",
        ));
        assert_eq!(
            cmd,
            r#""C:\Program Files\Liebert\liebert-dynamic-island.exe""#
        );
    }

    #[test]
    fn desktop_autostart_entry_has_required_keys() {
        let entry = desktop_autostart_entry("\"/opt/My Apps/liebert\"");
        assert!(entry.contains("[Desktop Entry]\n"));
        assert!(entry.contains("Exec=\"/opt/My Apps/liebert\"\n"));
        assert!(entry.contains("X-GNOME-Autostart-enabled=true\n"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn desktop_exec_escapes_shell_metacharacters() {
        let exec = escape_desktop_exec(Path::new("/tmp/My $Apps/liebert\""));
        assert_eq!(exec, "\"/tmp/My \\$Apps/liebert\\\"\"");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn base_and_cjk_font_discovery_finds_existing_files() {
        let base = base_fallback_fonts();
        for spec in &base {
            assert!(
                Path::new(&spec.path).is_file(),
                "missing font {}",
                spec.path
            );
            assert!(!spec.face_indices.is_empty());
        }
        let cjk = cjk_fallback_fonts_for(CjkScript::JapaneseOrHan);
        for spec in &cjk {
            assert!(
                Path::new(&spec.path).is_file(),
                "missing cjk font {}",
                spec.path
            );
            assert!(!spec.face_indices.is_empty());
        }
    }
}
