//! Quét máy để biết AI nào dùng được, tự cài CLI còn thiếu và mở Terminal để đăng nhập.

use crate::db::{self, Provider};
use crate::providers::{resolve_path_env, strip_ansi, which};
use crate::state::Core;
use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct InstallSpec {
    pub npm: &'static str,
    /// Trình cài đặt chính thức không cần Node.js (nếu có).
    pub native: Option<&'static str>,
    /// Lệnh chạy trong Terminal để đăng nhập lần đầu.
    pub login: &'static str,
}

pub fn install_spec(id: &str) -> Option<InstallSpec> {
    Some(match id {
        "claude-cli" => InstallSpec { npm: "@anthropic-ai/claude-code", native: Some("curl -fsSL https://claude.ai/install.sh | bash"), login: "claude" },
        "codex-cli" => InstallSpec { npm: "@openai/codex", native: None, login: "codex login" },
        "gemini-cli" => InstallSpec { npm: "@google/gemini-cli", native: None, login: "gemini" },
        "qwen-cli" => InstallSpec { npm: "@qwen-code/qwen-code", native: None, login: "qwen" },
        _ => return None,
    })
}

fn home() -> PathBuf {
    crate::platform::home_dir()
}

fn is_local(p: &Provider) -> bool {
    p.base_url.contains("localhost") || p.base_url.contains("127.0.0.1")
}

async fn version_of(exe: &std::path::Path, path_env: &str) -> String {
    let out = tokio::time::timeout(
        Duration::from_secs(10),
        crate::platform::command(exe)
            .arg("--version")
            .env("PATH", path_env)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await;
    match out {
        Ok(Ok(o)) => strip_ansi(&String::from_utf8_lossy(&o.stdout)).lines().next().unwrap_or("").trim().to_string(),
        _ => String::new(),
    }
}

/// Máy chủ cục bộ (Ollama, LM Studio…) có đang chạy không; trả về số model nếu đọc được.
async fn local_server(p: &Provider) -> Option<usize> {
    let url = format!("{}/models", p.base_url.trim_end_matches('/'));
    let c = reqwest::Client::builder().timeout(Duration::from_millis(1500)).build().ok()?;
    let v: Value = c.get(url).send().await.ok()?.json().await.ok()?;
    Some(v["data"].as_array().map(|a| a.len()).unwrap_or(0))
}

#[derive(Serialize)]
pub struct ProviderState {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    /// ready | missing | need_key | offline | disabled
    pub status: String,
    pub path: Option<String>,
    pub version: String,
    pub installable: bool,
    pub can_login: bool,
    pub detail: String,
}

async fn state_of(p: &Provider, path_env: &str, with_version: bool) -> ProviderState {
    let mut s = ProviderState {
        id: p.id.clone(),
        name: p.name.clone(),
        kind: p.kind.clone(),
        enabled: p.enabled,
        status: "ready".into(),
        path: None,
        version: String::new(),
        installable: false,
        can_login: false,
        detail: String::new(),
    };
    match p.kind.as_str() {
        "cli" => match which(&p.command, path_env) {
            Some(exe) => {
                s.path = Some(exe.to_string_lossy().to_string());
                if with_version {
                    s.version = version_of(&exe, path_env).await;
                }
                s.can_login = install_spec(&p.id).is_some();
                s.detail = "Đã cài trên máy".into();
            }
            None => {
                s.status = "missing".into();
                s.installable = install_spec(&p.id).is_some();
                s.detail = format!("Chưa có lệnh \"{}\"", p.command);
            }
        },
        "mock" => s.detail = "Luôn sẵn sàng (trả lời mẫu)".into(),
        "openai" if is_local(p) => match local_server(p).await {
            Some(n) => s.detail = format!("Máy chủ cục bộ đang chạy, {n} model"),
            None => {
                s.status = "offline".into();
                s.detail = format!("Không kết nối được {}", p.base_url);
            }
        },
        _ => {
            if p.api_key.trim().is_empty() {
                s.status = "need_key".into();
                s.detail = "Chưa nhập API key".into();
            } else {
                s.detail = "Đã có API key".into();
            }
        }
    }
    if !p.enabled && s.status == "ready" {
        s.status = "disabled".into();
    }
    s
}

pub async fn scan(core: &Core) -> Result<Value> {
    let path_env = core.path_env();
    let list = db::list_providers(&core.db())?;
    let mut states = vec![];
    for p in &list {
        states.push(state_of(p, &path_env, true).await);
    }
    let node = which("node", &path_env);
    let node_ver = match &node {
        Some(n) => version_of(n, &path_env).await,
        None => String::new(),
    };
    let installing: Vec<String> = core.0.installing.lock().map(|s| s.keys().cloned().collect()).unwrap_or_default();
    Ok(json!({
        "providers": states,
        "tools": {
            "node": node_ver,
            "npm": which("npm", &path_env).map(|p| p.to_string_lossy().to_string()),
            "brew": which("brew", &path_env).map(|p| p.to_string_lossy().to_string()),
            "curl": which("curl", &path_env).is_some(),
        },
        "installing": installing,
        "os": std::env::consts::OS,
    }))
}

/// Các kết nối thật sự dùng được (đã bật và sẵn sàng), không tính chế độ mô phỏng.
pub async fn available(core: &Core) -> Result<Vec<Provider>> {
    let path_env = core.path_env();
    let list = db::list_providers(&core.db())?;
    let mut out = vec![];
    for p in list.into_iter().filter(|p| p.enabled && p.kind != "mock") {
        if state_of(&p, &path_env, false).await.status == "ready" {
            out.push(p);
        }
    }
    Ok(out)
}

// ───────────────────────────── Cài đặt ─────────────────────────────

/// Quá thời gian này không có dòng log mới thì coi như treo (mạng đứt giữa chừng…).
const IDLE_LIMIT: Duration = Duration::from_secs(5 * 60);
const HEARTBEAT: Duration = Duration::from_secs(20);
const STALL: &str = "[treo]";

fn fmt_secs(s: u64) -> String {
    if s >= 60 { format!("{} phút {} giây", s / 60, s % 60) } else { format!("{s} giây") }
}

async fn run_streaming(core: &Core, id: &str, program: &str, args: &[String], path_env: &str, cancel: &watch::Receiver<bool>) -> Result<String> {
    run_streaming_with(core, id, program, args, path_env, cancel, IDLE_LIMIT).await
}

async fn run_streaming_with(core: &Core, id: &str, program: &str, args: &[String], path_env: &str, cancel: &watch::Receiver<bool>, idle_limit: Duration) -> Result<String> {
    core.emit(json!({"type": "install", "id": id, "line": format!("$ {} {}", program, args.join(" "))}));
    let mut child = crate::platform::command(std::path::Path::new(program))
        .args(args)
        .env("PATH", path_env)
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .current_dir(home())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow!("Không chạy được {program}: {e}"))?;
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut err = BufReader::new(child.stderr.take().unwrap()).lines();
    let mut log = String::new();
    let (mut o_done, mut e_done) = (false, false);
    let started = Instant::now();
    let mut last = Instant::now();
    let mut tick = tokio::time::interval(HEARTBEAT.min(idle_limit / 2));
    tick.tick().await;
    let cancel = cancel.clone();
    while !(o_done && e_done) {
        let line = tokio::select! {
            l = out.next_line(), if !o_done => match l { Ok(Some(l)) => Some(l), _ => { o_done = true; None } },
            l = err.next_line(), if !e_done => match l { Ok(Some(l)) => Some(l), _ => { e_done = true; None } },
            _ = tick.tick() => {
                let idle = last.elapsed();
                if idle >= idle_limit {
                    let _ = child.kill().await;
                    return Err(anyhow!("{STALL} Không có tiến triển trong {} (mạng có thể bị treo).", fmt_secs(idle.as_secs())));
                }
                if idle >= HEARTBEAT {
                    core.emit(json!({"type": "install", "id": id, "line": format!(
                        "… vẫn đang chạy ({} đã trôi qua, {} chưa có tin mới — có thể đang tải gói lớn)",
                        fmt_secs(started.elapsed().as_secs()), fmt_secs(idle.as_secs()))}));
                }
                None
            }
            _ = crate::providers::cancelled(cancel.clone()) => {
                let _ = child.kill().await;
                return Err(anyhow!("Đã huỷ cài đặt"));
            }
        };
        if let Some(l) = line {
            last = Instant::now();
            let l = strip_ansi(&l);
            if !l.trim().is_empty() {
                core.emit(json!({"type": "install", "id": id, "line": l}));
                log.push_str(&l);
                log.push('\n');
            }
        }
    }
    let status = child.wait().await?;
    if status.success() {
        Ok(log)
    } else {
        Err(anyhow!("Lệnh kết thúc với mã {}\n{}", status.code().unwrap_or(-1), log.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")))
    }
}

/// Tham số npm: in từng lượt tải để người dùng thấy tiến độ, giới hạn thời gian chờ và tự thử lại khi mạng chập chờn.
fn npm_args(pkg: &str, prefix: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = ["install", "-g", "--no-audit", "--no-fund", "--loglevel=http", "--fetch-timeout=180000", "--fetch-retries=4", "--fetch-retry-mintimeout=5000"]
        .iter().map(|s| s.to_string()).collect();
    if let Some(p) = prefix {
        a.push("--prefix".into());
        a.push(p.into());
    }
    a.push(pkg.into());
    a
}

async fn npm_install(core: &Core, id: &str, npm: &str, pkg: &str, prefix: Option<&str>, path_env: &str, cancel: &watch::Receiver<bool>) -> Result<String> {
    match run_streaming(core, id, npm, &npm_args(pkg, prefix), path_env, cancel).await {
        Err(e) if e.to_string().starts_with(STALL) => {
            // Kết nối treo: thử lại một lần (phần đã tải nằm trong bộ nhớ đệm npm nên lần sau nhanh hơn).
            core.emit(json!({"type": "install", "id": id, "line": "Kết nối bị treo — tự động thử lại…"}));
            run_streaming(core, id, npm, &npm_args(pkg, prefix), path_env, cancel).await
        }
        r => r,
    }
}

async fn do_install(core: &Core, id: &str, cancel: &watch::Receiver<bool>) -> Result<String> {
    let path_env = core.path_env();
    if id == "node" {
        #[cfg(windows)]
        {
            // Windows 10/11: dùng winget (có sẵn) để cài Node.js LTS.
            let winget = which("winget", &path_env).ok_or_else(|| anyhow!("Máy chưa có winget. Hãy cài Node.js từ https://nodejs.org rồi bấm Quét lại."))?;
            let args: Vec<String> = ["install", "-e", "--id", "OpenJS.NodeJS.LTS", "--silent", "--accept-source-agreements", "--accept-package-agreements"].iter().map(|s| s.to_string()).collect();
            run_streaming(core, id, &winget.to_string_lossy(), &args, &path_env, cancel).await?;
            core.set_path_env(crate::providers::resolve_path_env());
            return Ok("Đã cài Node.js".into());
        }
        #[cfg(not(windows))]
        {
            let brew = which("brew", &path_env).ok_or_else(|| anyhow!("Máy chưa có Homebrew. Hãy cài Node.js từ https://nodejs.org rồi bấm Quét lại."))?;
            run_streaming(core, id, &brew.to_string_lossy(), &["install".into(), "node".into()], &path_env, cancel).await?;
            return Ok("Đã cài Node.js".into());
        }
    }
    let spec = install_spec(id).ok_or_else(|| anyhow!("Kết nối này không hỗ trợ cài tự động"))?;
    if let Some(npm) = which("npm", &path_env) {
        let npm = npm.to_string_lossy().to_string();
        match npm_install(core, id, &npm, spec.npm, None, &path_env, cancel).await {
            Ok(_) => {}
            Err(e) if e.to_string().contains("EACCES") || e.to_string().contains("permission") => {
                // Thư mục npm toàn cục cần quyền quản trị: cài vào ~/.npm-global (không cần sudo).
                let prefix = home().join(".npm-global");
                core.emit(json!({"type": "install", "id": id, "line": format!("Không đủ quyền, cài vào {}", prefix.display())}));
                npm_install(core, id, &npm, spec.npm, Some(&prefix.to_string_lossy()), &path_env, cancel).await?;
            }
            Err(e) => return Err(e),
        }
    } else if let Some(native) = spec.native.filter(|_| cfg!(unix)) {
        run_streaming(core, id, "/bin/bash", &["-lc".into(), native.into()], &path_env, cancel).await?;
    } else {
        return Err(anyhow!("Cần Node.js (npm) để cài CLI này. Bấm \"Cài Node.js\" hoặc tải từ https://nodejs.org"));
    }
    Ok(format!("Đã cài {}", spec.npm))
}

/// Cài nền một CLI; tiến trình được phát qua sự kiện `install`.
pub fn start_install(core: &Core, id: &str) -> Result<()> {
    let (tx, rx) = watch::channel(false);
    {
        let mut s = core.0.installing.lock().unwrap_or_else(|e| e.into_inner());
        if s.contains_key(id) {
            return Err(anyhow!("Đang cài đặt, vui lòng chờ"));
        }
        s.insert(id.to_string(), tx);
    }
    let core = core.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let res = tokio::time::timeout(Duration::from_secs(30 * 60), do_install(&core, &id, &rx))
            .await
            .unwrap_or_else(|_| Err(anyhow!("Quá thời gian cài đặt (30 phút)")));
        // PATH có thể vừa thay đổi (thư mục bin mới)
        if let Ok(p) = tokio::task::spawn_blocking(resolve_path_env).await {
            core.set_path_env(p);
        }
        let found = db::get_provider(&core.db(), &id)
            .ok()
            .flatten()
            .and_then(|p| which(&p.command, &core.path_env()))
            .map(|p| p.to_string_lossy().to_string());
        let (ok, msg) = match res {
            Ok(m) if id == "node" || found.is_some() => (true, m),
            Ok(_) => (false, "Đã chạy cài đặt nhưng chưa thấy lệnh trong PATH. Hãy bấm Quét lại hoặc mở lại ứng dụng.".into()),
            Err(e) => {
                let m = e.to_string().replace(STALL, "").trim().to_string();
                (false, if m.contains("treo") { format!("{m} Bấm Cài đặt để thử lại.") } else { m })
            }
        };
        core.0.installing.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        core.log(if ok { "ket_noi" } else { "loi" }, "Hệ thống", &format!("Cài đặt {id}: {}", msg.lines().next().unwrap_or("")), None, None);
        core.emit(json!({"type": "install", "id": id, "done": true, "ok": ok, "message": msg, "path": found}));
    });
    Ok(())
}

/// Huỷ cài đặt đang chạy.
pub fn cancel_install(core: &Core, id: &str) -> bool {
    core.0.installing.lock().map(|s| s.get(id).map(|tx| tx.send(true).is_ok()).unwrap_or(false)).unwrap_or(false)
}

/// Mở Terminal chạy lệnh đăng nhập của CLI (đăng nhập cần tương tác qua trình duyệt).
pub fn open_login(core: &Core, id: &str) -> Result<()> {
    let spec = install_spec(id).ok_or_else(|| anyhow!("Kết nối này không cần đăng nhập qua Terminal"))?;
    let path_env = core.path_env();
    let p = db::get_provider(&core.db(), id)?.ok_or_else(|| anyhow!("Không tìm thấy kết nối"))?;
    let exe = which(&p.command, &path_env).ok_or_else(|| anyhow!("Chưa cài \"{}\"", p.command))?;
    let args: Vec<&str> = spec.login.split_whitespace().skip(1).collect();
    let cmd = std::iter::once(shell_words::quote(&exe.to_string_lossy()).to_string())
        .chain(args.iter().map(|a| a.to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    #[cfg(target_os = "macos")]
    {
        let script = core.0.data_dir.join(format!("dang-nhap-{id}.command"));
        std::fs::write(
            &script,
            format!(
                "#!/bin/zsh\nexport PATH={}\ncd ~\necho 'Đăng nhập {} — làm theo hướng dẫn, xong thì đóng cửa sổ này và bấm Quét lại trong AI Office.'\n{}\n",
                shell_words::quote(&path_env),
                p.name,
                cmd
            ),
        )?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
        std::process::Command::new("open").args(["-a", "Terminal"]).arg(&script).spawn()?;
        Ok(())
    }
    #[cfg(windows)]
    {
        // Mở cửa sổ Command Prompt mới chạy lệnh đăng nhập.
        let _ = &cmd;
        let script = core.0.data_dir.join(format!("dang-nhap-{id}.cmd"));
        let line = std::iter::once(format!("\"{}\"", exe.display())).chain(args.iter().map(|a| a.to_string())).collect::<Vec<_>>().join(" ");
        std::fs::write(
            &script,
            format!(
                "@echo off\r\nchcp 65001 >nul\r\nset \"PATH={}\"\r\ncd /d \"%USERPROFILE%\"\r\necho Dang nhap {} - lam theo huong dan, xong thi dong cua so nay va bam Quet lai trong AI Office.\r\n{}\r\npause\r\n",
                path_env, p.name, line
            ),
        )?;
        std::process::Command::new("cmd").args(["/c", "start", ""]).arg(&script).spawn()?;
        Ok(())
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        Err(anyhow!("Hãy mở cửa sổ dòng lệnh và chạy: {cmd}"))
    }
}

// ───────────────────────────── Danh sách model ─────────────────────────────

fn static_models(p: &Provider) -> Vec<String> {
    let v: &[&str] = match (p.id.as_str(), p.kind.as_str()) {
        ("claude-cli", _) => &["opus", "sonnet", "haiku", "fable", "claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5", "claude-fable-5-2"],
        (_, "anthropic") => &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5", "claude-fable-5-2"],
        ("gemini-cli", _) | (_, "gemini") => &["gemini-2.5-pro", "gemini-3.5-flash", "gemini-3.1-flash-lite"],
        _ => &[],
    };
    v.iter().map(|s| s.to_string()).collect()
}

/// Danh sách model của một kết nối: hỏi trực tiếp nếu được (Gemini CLI, API), không thì danh sách gợi ý.
pub async fn list_models(p: &Provider, path_env: &str) -> (Vec<String>, bool) {
    let client = || reqwest::Client::builder().timeout(Duration::from_secs(8)).build().ok();
    let live: Option<Vec<String>> = match p.kind.as_str() {
        "cli" if p.id == "gemini-cli" => match which(&p.command, path_env) {
            Some(exe) => {
                let out = tokio::time::timeout(
                    Duration::from_secs(25),
                    crate::platform::command(&exe).arg("--list-models").env("PATH", path_env).stdin(Stdio::null()).kill_on_drop(true).output(),
                )
                .await;
                out.ok().and_then(|o| o.ok()).map(|o| {
                    strip_ansi(&String::from_utf8_lossy(&o.stdout))
                        .lines()
                        .filter_map(|l| {
                            let l = l.trim();
                            let name = l.split(' ').next()?;
                            (l.contains('(') && !name.is_empty() && !l.starts_with("Available")).then(|| name.to_string())
                        })
                        .collect::<Vec<_>>()
                })
            }
            None => None,
        },
        "anthropic" if !p.api_key.is_empty() => {
            let base = if p.base_url.is_empty() { "https://api.anthropic.com" } else { p.base_url.trim_end_matches('/') };
            let resp = match client() {
                Some(c) => c.get(format!("{base}/v1/models?limit=100")).header("x-api-key", p.api_key.trim()).header("anthropic-version", "2026-10-21").send().await.ok()
                    .and_then(|r| r.error_for_status().ok()),
                None => None,
            };
            match resp {
                Some(r) => r.json::<Value>().await.ok().map(|v| {
                    v["data"].as_array().cloned().unwrap_or_default().iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()
                }),
                None => None,
            }
        }
        "openai" | "gemini" => {
            let base = p.base_url.trim_end_matches('/');
            let req = client().map(|c| {
                let r = c.get(format!("{base}/models"));
                if p.kind == "gemini" { r.header("x-goog-api-key", p.api_key.trim()) } else if p.api_key.is_empty() { r } else { r.bearer_auth(p.api_key.trim()) }
            });
            match req {
                Some(r) => match r.send().await.ok().and_then(|r| r.error_for_status().ok()) {
                    Some(resp) => resp.json::<Value>().await.ok().map(|v| {
                        let arr = v["data"].as_array().or(v["models"].as_array()).cloned().unwrap_or_default();
                        arr.iter()
                            .filter_map(|m| m["id"].as_str().or(m["name"].as_str()).map(|s| s.trim_start_matches("models/").to_string()))
                            .filter(|s| p.kind != "gemini" || s.starts_with("gemini"))
                            .collect()
                    }),
                    None => None,
                },
                None => None,
            }
        }
        _ => None,
    };
    match live.filter(|v| !v.is_empty()) {
        Some(mut v) => {
            v.sort();
            v.dedup();
            (v, true)
        }
        None => (static_models(p), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_exist_for_known_clis() {
        for id in ["claude-cli", "codex-cli", "gemini-cli", "qwen-cli"] {
            assert!(install_spec(id).is_some(), "{id}");
        }
        assert!(install_spec("anthropic-api").is_none());
    }

    fn test_core() -> (Core, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let core = Core::new(db::open_memory().unwrap(), dir.path().into(), "/bin:/usr/bin".into(), Box::new(|_| {}));
        (core, dir)
    }

    #[tokio::test]
    async fn stalled_command_is_killed() {
        let (core, _d) = test_core();
        let (_tx, rx) = watch::channel(false);
        let t = Instant::now();
        let e = run_streaming_with(&core, "x", "/bin/sleep", &["30".into()], "/bin", &rx, Duration::from_secs(1)).await.unwrap_err();
        assert!(e.to_string().starts_with(STALL), "{e}");
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn install_can_be_cancelled() {
        let (core, _d) = test_core();
        let (tx, rx) = watch::channel(false);
        tokio::spawn(async move { tokio::time::sleep(Duration::from_millis(300)).await; tx.send(true).unwrap(); });
        let e = run_streaming_with(&core, "x", "/bin/sleep", &["30".into()], "/bin", &rx, Duration::from_secs(60)).await.unwrap_err();
        assert!(e.to_string().contains("huỷ"), "{e}");
    }

    #[tokio::test]
    async fn output_resets_idle_timer_and_success_returns_log() {
        let (core, _d) = test_core();
        let (_tx, rx) = watch::channel(false);
        let log = run_streaming_with(&core, "x", "/bin/sh", &["-c".into(), "for i in 1 2 3; do echo dong $i; sleep 0.4; done".into()], "/bin:/usr/bin", &rx, Duration::from_secs(1)).await.unwrap();
        assert!(log.contains("dong 3"));
    }

    #[test]
    fn npm_args_have_timeouts() {
        let a = npm_args("@google/gemini-cli", None);
        assert!(a.contains(&"--fetch-retries=4".to_string()) && a.last().unwrap() == "@google/gemini-cli");
    }

    #[tokio::test]
    async fn state_reports_missing_cli_and_keys() {
        let mut p = crate::providers::builtin_providers().into_iter().find(|p| p.id == "codex-cli").unwrap();
        p.command = "khong-co-lenh-nay".into();
        let s = state_of(&p, "/bin", false).await;
        assert_eq!(s.status, "missing");
        assert!(s.installable);
        let k = crate::providers::builtin_providers().into_iter().find(|p| p.id == "anthropic-api").unwrap();
        assert_eq!(state_of(&k, "/bin", false).await.status, "need_key");
    }
}
