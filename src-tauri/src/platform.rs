//! Khác biệt giữa các hệ điều hành: thư mục nhà, thư mục cài CLI, chạy lệnh không bật cửa sổ
//! console và "shim" .cmd của npm trên Windows.

use std::path::{Path, PathBuf};

/// Thư mục nhà của người dùng (macOS/Linux: $HOME, Windows: %USERPROFILE%).
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Các thư mục thường chứa CLI (npm toàn cục, trình cài đặt chính thức…) ngoài PATH hiện có.
pub fn extra_bin_dirs() -> Vec<PathBuf> {
    let home = home_dir();
    let mut v: Vec<PathBuf> = [".local/bin", ".cargo/bin", ".npm-global/bin", ".bun/bin", ".volta/bin", "bin"]
        .iter()
        .map(|d| home.join(d))
        .collect();
    #[cfg(windows)]
    {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        if let Some(a) = env("APPDATA") {
            v.push(a.join("npm"));
        }
        for k in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(p) = env(k) {
                v.push(p.join("nodejs"));
            }
        }
        if let Some(l) = env("LOCALAPPDATA") {
            v.push(l.join("Programs").join("nodejs"));
            v.push(l.join("Microsoft").join("WindowsApps"));
        }
        v.push(home.join(".npm-global"));
    }
    #[cfg(unix)]
    {
        // Node cài qua nvm: thêm thư mục bin của các phiên bản đã cài (mới nhất trước).
        if let Ok(rd) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut vs: Vec<_> = rd.flatten().map(|e| e.path().join("bin")).collect();
            vs.sort();
            vs.reverse();
            v.extend(vs);
        }
        for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
            v.push(PathBuf::from(d));
        }
    }
    v
}

/// Tên tệp thực thi có thể có của một lệnh trong một thư mục.
pub fn exe_candidates(dir: &Path, cmd: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        // Trên Windows tệp không đuôi do npm tạo là script sh — không chạy được, nên ưu tiên .exe/.cmd.
        let mut v: Vec<PathBuf> = ["exe", "cmd", "bat", "com"].iter().map(|e| dir.join(format!("{cmd}.{e}"))).collect();
        if Path::new(cmd).extension().is_some() {
            v.insert(0, dir.join(cmd));
        }
        v
    }
    #[cfg(not(windows))]
    {
        vec![dir.join(cmd)]
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Lệnh tokio chạy `exe`. Trên Windows: shim .cmd của npm được thay bằng `node script.js`
/// (cmd.exe không nhận đối số nhiều dòng như lời nhắc) và không bật cửa sổ console.
pub fn command(exe: &Path) -> tokio::process::Command {
    let (prog, pre) = resolve(exe);
    let mut c = tokio::process::Command::new(prog);
    c.args(pre);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Như [`command`] nhưng dùng std::process.
pub fn std_command(exe: &Path) -> std::process::Command {
    let (prog, pre) = resolve(exe);
    let mut c = std::process::Command::new(prog);
    c.args(pre);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

fn resolve(exe: &Path) -> (PathBuf, Vec<PathBuf>) {
    #[cfg(windows)]
    {
        let ext = exe.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if ext == "cmd" || ext == "bat" {
            if let (Some(dir), Ok(text)) = (exe.parent(), std::fs::read_to_string(exe)) {
                if let Some(r) = parse_npm_shim(dir, &text) {
                    return r;
                }
            }
        }
    }
    (exe.to_path_buf(), Vec::new())
}

/// Đọc shim .cmd do npm (cmd-shim) tạo: `"%_prog%" "%dp0%\node_modules\…\cli.js" %*`.
/// Trả về (chương trình, đối số đứng trước) để chạy trực tiếp không qua cmd.exe.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_npm_shim(dir: &Path, text: &str) -> Option<(PathBuf, Vec<PathBuf>)> {
    const MARK: &str = "\"%dp0%\\";
    let line = text.lines().rev().find(|l| l.contains("%*") && l.contains(MARK))?;
    let start = line.rfind(MARK)? + MARK.len();
    let rel = &line[start..start + line[start..].find('"')?];
    if rel.eq_ignore_ascii_case("node.exe") || rel.is_empty() {
        return None;
    }
    let target = rel.split('\\').fold(dir.to_path_buf(), |p, s| p.join(s));
    let ext = target.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if ext == "exe" {
        return Some((target, Vec::new()));
    }
    if !matches!(ext.as_str(), "js" | "mjs" | "cjs" | "") {
        return None;
    }
    let local_node = dir.join("node.exe");
    let node = if local_node.is_file() { local_node } else { PathBuf::from("node") };
    Some((node, vec![target]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_shim_is_unwrapped_to_node_script() {
        let shim = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\cli.js\" %*\r\n";
        let dir = Path::new("/npm");
        let (prog, pre) = parse_npm_shim(dir, shim).unwrap();
        assert_eq!(prog, PathBuf::from("node"));
        assert_eq!(pre, vec![dir.join("node_modules").join("@anthropic-ai").join("claude-code").join("cli.js")]);

        let native = "@ECHO off\r\n\"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.exe\"   %*\r\n";
        let (prog, pre) = parse_npm_shim(dir, native).unwrap();
        assert!(prog.ends_with("codex.exe"));
        assert!(pre.is_empty());

        // npm.cmd của Node.js có định dạng khác → để nguyên, chạy qua cmd.exe
        assert!(parse_npm_shim(dir, "@ECHO OFF\r\n\"%NODE_EXE%\" \"%NPM_CLI_JS%\" %*\r\n").is_none());
    }

    #[test]
    fn home_dir_is_not_empty() {
        assert!(!home_dir().as_os_str().is_empty());
    }
}
