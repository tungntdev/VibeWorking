//! Kết nối tới các AI: CLI (Claude Code, Codex, Gemini, Qwen...), API (Anthropic, OpenAI-tương thích,
//! Gemini) và chế độ mô phỏng không cần AI.

use crate::db::Provider;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::watch;

pub type Cancel = watch::Receiver<bool>;

/// Chờ đến khi có lệnh huỷ. Nếu kênh đã đóng thì chờ mãi (không coi là huỷ).
pub async fn cancelled(mut rx: Cancel) {
    if *rx.borrow() {
        return;
    }
    if rx.wait_for(|v| *v).await.is_err() {
        std::future::pending::<()>().await;
    }
}

fn p(id: &str, name: &str, kind: &str) -> Provider {
    Provider {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        command: String::new(),
        args: String::new(),
        model_args: String::new(),
        edit_args: String::new(),
        dir_args: String::new(),
        effort_args: String::new(),
        web_args: String::new(),
        use_stdin: false,
        base_url: String::new(),
        api_key: String::new(),
        default_model: String::new(),
        timeout_secs: 900,
        enabled: true,
        builtin: true,
        notes: String::new(),
    }
}

/// Danh sách nhà cung cấp dựng sẵn. Người dùng có thể sửa hoặc thêm loại "cli"/"openai" tuỳ ý.
pub fn builtin_providers() -> Vec<Provider> {
    let mut v = Vec::new();

    let mut x = p("mock", "Mô phỏng (không cần AI)", "mock");
    x.timeout_secs = 60;
    x.notes = "Trả lời mẫu để chạy thử văn phòng khi chưa kết nối AI.".into();
    v.push(x);

    let mut x = p("claude-cli", "Claude Code CLI", "cli");
    x.command = "claude".into();
    x.args = "-p {prompt} --output-format stream-json --verbose {model_args}".into();
    x.model_args = "--model {model}".into();
    x.edit_args = "--permission-mode acceptEdits".into();
    x.dir_args = "--add-dir {dir}".into();
    x.effort_args = "--effort {effort}".into();
    x.web_args = "--allowedTools WebSearch,WebFetch".into();
    x.notes = "Cần cài Claude Code và đăng nhập (lệnh `claude`).".into();
    v.push(x);

    let mut x = p("codex-cli", "OpenAI Codex CLI", "cli");
    x.command = "codex".into();
    x.args = "exec --skip-git-repo-check --output-last-message {output_file} {model_args} {prompt}".into();
    x.model_args = "-m {model}".into();
    x.edit_args = "--sandbox workspace-write".into();
    x.effort_args = "-c model_reasoning_effort={effort}".into();
    x.notes = "Cần cài Codex CLI (`npm i -g @openai/codex`) và đăng nhập.".into();
    v.push(x);

    let mut x = p("gemini-cli", "Google Gemini CLI", "cli");
    x.command = "gemini".into();
    x.args = "-p {prompt} --output-format stream-json {model_args}".into();
    x.model_args = "-m {model}".into();
    x.edit_args = "--approval-mode auto_edit".into();
    x.dir_args = "--include-directories {dir}".into();
    x.notes = "Cần cài Gemini CLI (`npm i -g @google/gemini-cli`) và đăng nhập.".into();
    v.push(x);

    let mut x = p("qwen-cli", "Qwen Code CLI", "cli");
    x.command = "qwen".into();
    x.args = "-p {prompt} {model_args}".into();
    x.model_args = "-m {model}".into();
    x.edit_args = "--approval-mode auto_edit".into();
    x.enabled = false;
    x.notes = "Tuỳ chọn: `npm i -g @qwen-code/qwen-code`.".into();
    v.push(x);

    let mut x = p("anthropic-api", "Claude API (Anthropic)", "anthropic");
    x.base_url = "https://api.anthropic.com".into();
    x.default_model = "claude-opus-5-5".into();
    x.timeout_secs = 600;
    x.notes = "Nhập API key từ console Anthropic.".into();
    v.push(x);

    let mut x = p("openai-api", "OpenAI API", "openai");
    x.base_url = "https://api.openai.com/v1".into();
    x.default_model = "gpt-5".into();
    x.timeout_secs = 600;
    x.notes = "Dùng được cho mọi dịch vụ tương thích OpenAI (OpenRouter, DeepSeek, Groq...) bằng cách đổi URL.".into();
    v.push(x);

    let mut x = p("gemini-api", "Gemini API (Google)", "gemini");
    x.base_url = "https://generativelanguage.googleapis.com/v1beta".into();
    x.default_model = "gemini-3.5-flash".into();
    x.timeout_secs = 600;
    x.notes = "Nhập API key từ Google AI Studio.".into();
    v.push(x);

    let mut x = p("ollama", "Ollama (máy cục bộ)", "openai");
    x.base_url = "http://localhost:11434/v1".into();
    x.default_model = "llama3.2".into();
    x.notes = "Chạy AI ngay trên máy, không cần Internet. Cần cài Ollama và tải model.".into();
    v.push(x);

    v
}

// ───────────────────────────── PATH cho ứng dụng GUI ─────────────────────────────

/// Ứng dụng mở từ Finder không có PATH của shell, nên lấy PATH từ login shell của người dùng
/// và bổ sung các thư mục cài đặt phổ biến.
pub fn resolve_path_env() -> String {
    let mut parts: Vec<String> = Vec::new();
    #[cfg(unix)]
    if let Some(sp) = login_shell_path() {
        parts.extend(sp.split(':').map(String::from));
    }
    if let Ok(cur) = std::env::var("PATH") {
        parts.extend(std::env::split_paths(&cur).map(|p| p.to_string_lossy().to_string()));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for d in [".local/bin", ".cargo/bin", ".npm-global/bin", ".bun/bin", ".volta/bin", "bin"] {
            parts.push(home.join(d).to_string_lossy().to_string());
        }
        // Node cài qua nvm: thêm thư mục bin của các phiên bản đã cài.
        if let Ok(rd) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut vs: Vec<_> = rd.flatten().map(|e| e.path().join("bin")).collect();
            vs.sort();
            vs.reverse();
            parts.extend(vs.into_iter().map(|p| p.to_string_lossy().to_string()));
        }
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
        parts.push(d.into());
    }
    let mut seen = std::collections::HashSet::new();
    parts.retain(|p| !p.is_empty() && seen.insert(p.clone()));
    std::env::join_paths(parts.iter())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    use std::process::Command;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", "printf '__P__%s__P__' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(6) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let a = s.find("__P__")? + 5;
    let b = s[a..].find("__P__")? + a;
    Some(s[a..b].to_string())
}

/// Tìm đường dẫn đầy đủ của lệnh trong PATH.
pub fn which(cmd: &str, path_env: &str) -> Option<PathBuf> {
    if cmd.is_empty() {
        return None;
    }
    let direct = Path::new(cmd);
    if cmd.contains('/') || cmd.contains('\\') {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    for dir in std::env::split_paths(path_env) {
        let cand = dir.join(cmd);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat"] {
            let c = dir.join(format!("{cmd}.{ext}"));
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

// ───────────────────────────── Gọi AI ─────────────────────────────

pub struct Call<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub prompt: &'a str,
    pub workdir: &'a Path,
    pub path_env: &'a str,
    /// Tên vai để mô phỏng trả lời sát hơn.
    pub persona: &'a str,
    /// Tham số bổ sung (quyền sửa file, thư mục được phép đọc) chèn trước nội dung yêu cầu.
    pub extra_args: &'a [String],
    /// Mức suy luận: '' | low | medium | high | xhigh | max
    pub effort: &'a str,
    /// Cho phép tra cứu web (API: bật công cụ tìm kiếm; CLI: dùng web_args của kết nối)
    pub web: bool,
    /// Nhận từng dòng tiến trình khi AI đang làm (đọc tệp, tìm kiếm, viết…)
    pub on_progress: Option<Progress>,
}

pub type Progress = std::sync::Arc<dyn Fn(String) + Send + Sync>;

/// Các mẫu args mặc định cũ (để nâng cấp sang dạng luồng sự kiện khi khởi động).
pub const LEGACY_ARGS: &[(&str, &str)] = &[
    ("claude-cli", "-p {prompt} --output-format text {model_args}"),
    ("gemini-cli", "-p {prompt} {model_args}"),
];

#[derive(Default)]
pub struct StreamAcc {
    pub json_lines: usize,
    pub text: String,
    pub result: Option<String>,
}

fn short(s: &str, n: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > n { format!("{}…", one.chars().take(n).collect::<String>()) } else { one }
}

fn tool_desc(name: &str, input: &Value) -> String {
    let arg = ["file_path", "path", "pattern", "query", "url", "command", "absolute_path", "dir_path", "description"]
        .iter()
        .find_map(|k| input.get(*k).and_then(|v| v.as_str()))
        .unwrap_or("");
    let verb = match name {
        "Read" | "read_file" | "read_many_files" => "Đọc tệp",
        "Write" | "write_file" => "Ghi tệp",
        "Edit" | "MultiEdit" | "replace" | "edit" => "Sửa tệp",
        "Glob" | "glob" | "LS" | "list_directory" => "Xem thư mục",
        "Grep" | "search_file_content" | "grep" => "Tìm trong tệp",
        "WebSearch" | "google_web_search" | "web_search" => "Tìm kiếm web",
        "WebFetch" | "web_fetch" => "Mở trang web",
        "Bash" | "run_shell_command" | "shell" => "Chạy lệnh",
        "Task" | "Agent" => "Giao việc phụ",
        "TodoWrite" | "write_todos" => "Lập danh sách việc",
        _ => "Dùng công cụ",
    };
    if arg.is_empty() { format!("🔧 {verb} ({name})") } else { format!("🔧 {verb}: {}", short(arg, 90)) }
}

/// Hiểu một dòng JSON từ chế độ stream-json (Claude Code, Gemini CLI). Trả về dòng tiến trình nếu có.
pub fn parse_stream_line(v: &Value, acc: &mut StreamAcc) -> Option<String> {
    acc.json_lines += 1;
    match v["type"].as_str().unwrap_or("") {
        // Claude Code
        "assistant" => {
            let mut out = vec![];
            for c in v.pointer("/message/content").and_then(|c| c.as_array()).cloned().unwrap_or_default() {
                match c["type"].as_str() {
                    Some("text") => {
                        let t = c["text"].as_str().unwrap_or("");
                        acc.text.push_str(t);
                        if !t.trim().is_empty() {
                            out.push(format!("✍️ {}", short(t, 140)));
                        }
                    }
                    Some("tool_use") => out.push(tool_desc(c["name"].as_str().unwrap_or("?"), &c["input"])),
                    Some("thinking") => out.push("💭 Đang suy nghĩ…".into()),
                    _ => {}
                }
            }
            (!out.is_empty()).then(|| out.join("\n"))
        }
        "result" => {
            if let Some(r) = v["result"].as_str() {
                acc.result = Some(r.to_string());
            }
            if v["is_error"].as_bool() == Some(true) || v["status"].as_str() == Some("error") {
                return Some(format!("⚠️ Kết thúc với lỗi: {}", short(&v["result"].as_str().unwrap_or("").to_string(), 160)));
            }
            Some("✅ Hoàn tất".into())
        }
        "system" if v["subtype"] == "init" => Some(format!("▶️ Bắt đầu phiên{}", v["model"].as_str().map(|m| format!(" · {m}")).unwrap_or_default())),
        "init" => Some(format!("▶️ Bắt đầu phiên{}", v["model"].as_str().map(|m| format!(" · {m}")).unwrap_or_default())),
        // Gemini CLI
        "message" if v["role"] == "assistant" => {
            let t = v["content"].as_str().unwrap_or("");
            acc.text.push_str(t);
            None
        }
        "tool_use" => Some(tool_desc(v["tool_name"].as_str().or(v["name"].as_str()).unwrap_or("?"), v.get("parameters").or(v.get("input")).unwrap_or(&Value::Null))),
        "error" => Some(format!("⚠️ {}", short(v["message"].as_str().unwrap_or(""), 160))),
        _ => None,
    }
}

pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Dấu hiệu nhân viên AI cần hỏi lại quản lý trước khi làm tiếp.
pub const QUESTION_MARK: &str = "[[HỎI]]";

/// Nếu câu trả lời là một câu hỏi lại thì trả về nội dung câu hỏi.
pub fn extract_question(out: &str) -> Option<String> {
    let t = out.trim();
    let pos = t.find(QUESTION_MARK)?;
    // chỉ coi là câu hỏi khi dấu hiệu ở đầu, hoặc câu trả lời ngắn (tránh nhầm khi AI chỉ nhắc tới dấu hiệu)
    if pos > 0 && t.len() > 800 {
        return None;
    }
    let q = t[pos + QUESTION_MARK.len()..].trim().trim_start_matches(':').trim().to_string();
    (!q.is_empty()).then_some(q)
}

pub fn effort_label(e: &str) -> &'static str {
    match e {
        "low" => "thấp",
        "medium" => "trung bình",
        "high" => "cao",
        "xhigh" => "rất cao",
        "max" => "tối đa",
        _ => "mặc định",
    }
}

/// Quy đổi mức suy luận chung sang từ vựng của từng hãng.
pub fn effort_value(p: &Provider, level: &str) -> Option<String> {
    if !EFFORTS.contains(&level) {
        return None;
    }
    let openai_like = p.id == "codex-cli" || (p.kind == "openai");
    Some(if openai_like {
        match level { "xhigh" | "max" => "high", l => l }.to_string()
    } else {
        level.to_string()
    })
}

/// Ghi chú thêm vào yêu cầu khi kết nối không có tham số tương ứng (VD Gemini CLI không có cờ mức suy luận).
pub fn prompt_hints(p: &Provider, level: &str, web: bool) -> String {
    let mut h = vec![];
    let cli_effort = p.kind == "cli" && !p.effort_args.trim().is_empty();
    let api_effort = matches!(p.kind.as_str(), "anthropic" | "openai" | "gemini");
    if EFFORTS.contains(&level) && !cli_effort && !api_effort {
        h.push(match level {
            "low" => "Mức suy luận: THẤP — trả lời nhanh, ngắn gọn, đi thẳng vào kết quả.".to_string(),
            "medium" => "Mức suy luận: TRUNG BÌNH — cân nhắc vừa đủ trước khi trả lời.".to_string(),
            l => format!("Mức suy luận: {} — suy nghĩ thật kỹ từng bước, kiểm tra lại lập luận và số liệu trước khi trả lời.", effort_label(l).to_uppercase()),
        });
    }
    let web_ok = (p.kind == "cli" && (!p.web_args.trim().is_empty() || p.id == "gemini-cli")) || matches!(p.kind.as_str(), "anthropic" | "gemini");
    if web {
        h.push(if web_ok {
            "Hãy dùng công cụ tìm kiếm web để tra cứu thông tin mới nhất và ghi rõ nguồn (đường dẫn) cho từng thông tin.".to_string()
        } else {
            "Nếu có công cụ tra cứu web thì hãy dùng và ghi rõ nguồn; nếu không, nói rõ thông tin nào cần kiểm chứng thêm.".to_string()
        });
    }
    if h.is_empty() { String::new() } else { format!("\n\n# YÊU CẦU CÁCH LÀM\n{}", h.join("\n")) }
}

pub fn combined_prompt(system: &str, prompt: &str) -> String {
    if system.trim().is_empty() {
        prompt.to_string()
    } else {
        format!("# VAI TRÒ CỦA BẠN\n{}\n\n# YÊU CẦU\n{}", system.trim(), prompt.trim())
    }
}

pub fn effective_model(p: &Provider, model: &str) -> String {
    if model.trim().is_empty() {
        p.default_model.trim().to_string()
    } else {
        model.trim().to_string()
    }
}

/// Dựng danh sách tham số dòng lệnh từ mẫu. Hỗ trợ {prompt} {model} {model_args} {output_file} {workdir}.
pub fn build_cli_args(p: &Provider, model: &str, prompt: &str, output_file: &str, workdir: &str, extra: &[String]) -> Result<Vec<String>> {
    let tmpl = shell_words::split(&p.args).map_err(|e| anyhow!("Mẫu tham số không hợp lệ: {e}"))?;
    let model_args: Vec<String> = if model.is_empty() || p.model_args.trim().is_empty() {
        vec![]
    } else {
        shell_words::split(&p.model_args)
            .map_err(|e| anyhow!("Mẫu tham số model không hợp lệ: {e}"))?
            .into_iter()
            .map(|a| a.replace("{model}", model))
            .collect()
    };
    let mut out = Vec::new();
    for t in tmpl {
        match t.as_str() {
            "{model_args}" => out.extend(model_args.iter().cloned()),
            "{prompt}" => {
                out.extend(extra.iter().cloned());
                if !p.use_stdin {
                    out.push(prompt.to_string())
                }
            }
            _ => {
                if t.contains("{model}") && model.is_empty() {
                    continue;
                }
                out.push(
                    t.replace("{model}", model)
                        .replace("{output_file}", output_file)
                        .replace("{workdir}", workdir),
                )
            }
        }
    }
    if !extra.is_empty() && !p.args.contains("{prompt}") {
        out.extend(extra.iter().cloned());
    }
    Ok(out)
}

/// Tham số bổ sung cho một kết nối CLI: quyền sửa file và các thư mục được phép đọc thêm.
pub fn extra_cli_args(p: &Provider, allow_edit: bool, dirs: &[String]) -> Vec<String> {
    cli_extra(p, allow_edit, dirs, "", false)
}

/// Tham số CLI bổ sung đầy đủ: quyền sửa file, thư mục đọc thêm, mức suy luận, tra cứu web.
pub fn cli_extra(p: &Provider, allow_edit: bool, dirs: &[String], effort: &str, web: bool) -> Vec<String> {
    let mut v = vec![];
    if p.kind != "cli" {
        return v;
    }
    if let Some(ev) = effort_value(p, effort) {
        for t in shell_words::split(&p.effort_args).unwrap_or_default() {
            v.push(t.replace("{effort}", &ev));
        }
    }
    if web {
        v.extend(shell_words::split(&p.web_args).unwrap_or_default());
    }
    if allow_edit {
        v.extend(shell_words::split(&p.edit_args).unwrap_or_default());
    }
    if !p.dir_args.trim().is_empty() {
        for d in dirs {
            for t in shell_words::split(&p.dir_args).unwrap_or_default() {
                v.push(t.replace("{dir}", d));
            }
        }
    }
    v
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                while let Some(&n) = it.peek() {
                    it.next();
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        format!("…{}", chars[chars.len() - n..].iter().collect::<String>())
    }
}

pub async fn invoke(p: &Provider, call: &Call<'_>, cancel: Cancel) -> Result<String> {
    if !p.enabled {
        return Err(anyhow!("Kết nối \"{}\" đang bị tắt", p.name));
    }
    let timeout = Duration::from_secs(p.timeout_secs.clamp(10, 7200) as u64);
    let work = async {
        match p.kind.as_str() {
            "cli" => invoke_cli(p, call).await,
            "anthropic" => invoke_anthropic(p, call, timeout).await,
            "openai" => invoke_openai(p, call, timeout).await,
            "gemini" => invoke_gemini(p, call, timeout).await,
            "mock" => invoke_mock(call).await,
            k => Err(anyhow!("Loại kết nối không hỗ trợ: {k}")),
        }
    };
    tokio::select! {
        r = work => r.map(|s| s.trim().to_string()),
        _ = tokio::time::sleep(timeout) => Err(anyhow!("Quá thời gian chờ ({} giây)", timeout.as_secs())),
        _ = cancelled(cancel) => Err(anyhow!("Đã huỷ")),
    }
}

async fn invoke_cli(p: &Provider, call: &Call<'_>) -> Result<String> {
    let exe = which(&p.command, call.path_env).ok_or_else(|| {
        anyhow!("Không tìm thấy lệnh \"{}\". Hãy cài đặt hoặc nhập đường dẫn đầy đủ trong mục Kết nối AI.", p.command)
    })?;
    std::fs::create_dir_all(call.workdir).ok();
    let model = effective_model(p, call.model);
    let full = combined_prompt(call.system, &format!("{}{}", call.prompt, prompt_hints(p, call.effort, call.web)));
    let out_file = std::env::temp_dir().join(format!(
        "ai-office-{}-{}.txt",
        std::process::id(),
        rand::random::<u32>()
    ));
    let args = build_cli_args(
        p,
        &model,
        &full,
        &out_file.to_string_lossy(),
        &call.workdir.to_string_lossy(),
        call.extra_args,
    )?;
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.args(&args)
        .current_dir(call.workdir)
        .env("PATH", call.path_env)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        // Gemini CLI từ chối chạy ở thư mục chưa "tin cậy" khi không tương tác
        .env("GEMINI_CLI_TRUST_WORKSPACE", "true")
        .stdin(if p.use_stdin { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().with_context(|| format!("Không chạy được {}", exe.display()))?;
    if p.use_stdin {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(full.as_bytes()).await?;
            drop(stdin);
        }
    }
    let report = |s: String| {
        if let Some(cb) = &call.on_progress {
            cb(s);
        }
    };
    report(format!("▶️ Chạy {}{}", p.name, if model.is_empty() { String::new() } else { format!(" · {model}") }));
    // đọc stdout/stderr từng dòng để báo tiến trình trực tiếp
    let mut out_lines = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
    let mut err_lines = tokio::io::BufReader::new(child.stderr.take().unwrap()).lines();
    let (mut stdout, mut stderr) = (String::new(), String::new());
    let mut acc = StreamAcc::default();
    let (mut o_done, mut e_done) = (false, false);
    while !(o_done && e_done) {
        tokio::select! {
            l = out_lines.next_line(), if !o_done => match l {
                Ok(Some(l)) => {
                    let l = strip_ansi(&l);
                    match serde_json::from_str::<Value>(l.trim()) {
                        Ok(v) if v.is_object() => {
                            if let Some(msg) = parse_stream_line(&v, &mut acc) {
                                for m in msg.lines() { report(m.to_string()); }
                            }
                        }
                        _ => {
                            if !l.trim().is_empty() && acc.json_lines == 0 && stdout.len() < 400 {
                                report(format!("✍️ {}", short(&l, 140)));
                            }
                            stdout.push_str(&l);
                            stdout.push('\n');
                        }
                    }
                }
                _ => o_done = true,
            },
            l = err_lines.next_line(), if !e_done => match l {
                Ok(Some(l)) => {
                    let l = strip_ansi(&l);
                    if !l.trim().is_empty() {
                        report(format!("· {}", short(&l, 140)));
                    }
                    stderr.push_str(&l);
                    stderr.push('\n');
                }
                _ => e_done = true,
            },
        }
    }
    let status = child.wait().await?;
    let file_out = std::fs::read_to_string(&out_file).unwrap_or_default();
    let _ = std::fs::remove_file(&out_file);
    if !status.success() {
        let msg = if stderr.trim().is_empty() { acc.result.clone().unwrap_or(stdout.clone()) } else { stderr };
        return Err(anyhow!(
            "{} thoát với mã {}: {}{}",
            p.command,
            status.code().unwrap_or(-1),
            tail(msg.trim(), 1200),
            known_hint(&msg)
        ));
    }
    let streamed = acc.result.clone().filter(|r| !r.trim().is_empty()).unwrap_or_else(|| acc.text.clone());
    let text = if !file_out.trim().is_empty() {
        file_out
    } else if acc.json_lines > 0 && !streamed.trim().is_empty() {
        streamed
    } else {
        stdout
    };
    if text.trim().is_empty() {
        return Err(anyhow!("{} không trả về nội dung. {}", p.command, tail(stderr.trim(), 600)));
    }
    Ok(text)
}

/// Gợi ý cách khắc phục cho các lỗi hay gặp của CLI.
pub fn known_hint(msg: &str) -> String {
    let m = msg.to_lowercase();
    let h = if m.contains("ineligibletiererror") || m.contains("no longer supported for gemini code assist") {
        "Tài khoản Google này không còn dùng được Gemini CLI theo gói cá nhân. Cách khắc phục: tạo API key ở Google AI Studio rồi nhập vào kết nối \"Gemini API (Google)\" (hoặc đặt GEMINI_API_KEY cho Gemini CLI)."
    } else if m.contains("not running in a trusted directory") {
        "Gemini CLI chưa tin cậy thư mục làm việc — hãy cập nhật ứng dụng/Gemini CLI."
    } else if m.contains("invalid api key") || m.contains("not logged in") || m.contains("please run /login") || m.contains("authentication") {
        "Có vẻ CLI chưa đăng nhập — bấm \"Đăng nhập\" ở mục Kết nối AI."
    } else if m.contains("rate limit") || m.contains("quota") || m.contains("429") {
        "Đã chạm giới hạn sử dụng của tài khoản — chờ một lúc hoặc chuyển nhân viên sang AI khác."
    } else {
        ""
    };
    if h.is_empty() { String::new() } else { format!("\n\n👉 {h}") }
}

fn http(timeout: Duration) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("ai-office/1.0")
        .build()?)
}

fn need_key(p: &Provider) -> Result<()> {
    if p.api_key.trim().is_empty() {
        return Err(anyhow!("Chưa nhập API key cho \"{}\"", p.name));
    }
    Ok(())
}

async fn read_json(resp: reqwest::Response) -> Result<Value> {
    let status = resp.status();
    let body = resp.text().await?;
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::String(body.clone()));
    if !status.is_success() {
        let msg = v
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .map(String::from)
            .unwrap_or_else(|| tail(&body, 500));
        return Err(anyhow!("HTTP {}: {}", status.as_u16(), msg));
    }
    Ok(v)
}

async fn invoke_anthropic(p: &Provider, call: &Call<'_>, timeout: Duration) -> Result<String> {
    need_key(p)?;
    let model = effective_model(p, call.model);
    let base = if p.base_url.trim().is_empty() { "https://api.anthropic.com" } else { p.base_url.trim_end_matches('/') };
    let prompt = format!("{}{}", call.prompt, prompt_hints(p, call.effort, call.web));
    let mut body = json!({
        "model": model,
        "max_tokens": 16000,
        "messages": [{"role": "user", "content": prompt}],
        "fallbacks": "default",
    });
    if !call.system.trim().is_empty() {
        body["system"] = json!(call.system);
    }
    if let Some(ev) = effort_value(p, call.effort) {
        body["output_config"] = json!({"effort": ev});
    }
    if call.web {
        body["tools"] = json!([{"type": "web_search_20260209", "name": "web_search"}]);
    }
    let resp = http(timeout)?
        .post(format!("{base}/v1/messages"))
        .header("x-api-key", p.api_key.trim())
        .header("anthropic-version", "2026-10-21")
        .header("anthropic-beta", "server-side-fallback-2026-07-01")
        .json(&body)
        .send()
        .await?;
    let v = read_json(resp).await?;
    if v["stop_reason"] == "refusal" {
        let why = v.pointer("/stop_details/explanation").and_then(|x| x.as_str()).unwrap_or("");
        return Err(anyhow!("Claude từ chối yêu cầu này. {why}"));
    }
    let text: Vec<&str> = v["content"]
        .as_array()
        .map(|a| a.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect())
        .unwrap_or_default();
    if text.is_empty() {
        return Err(anyhow!("Phản hồi không có nội dung văn bản"));
    }
    Ok(text.join("\n"))
}

async fn invoke_openai(p: &Provider, call: &Call<'_>, timeout: Duration) -> Result<String> {
    let model = effective_model(p, call.model);
    if model.is_empty() {
        return Err(anyhow!("Chưa chọn model cho \"{}\"", p.name));
    }
    let base = p.base_url.trim_end_matches('/');
    let mut messages = vec![];
    if !call.system.trim().is_empty() {
        messages.push(json!({"role": "system", "content": call.system}));
    }
    messages.push(json!({"role": "user", "content": format!("{}{}", call.prompt, prompt_hints(p, call.effort, call.web))}));
    let mut body = json!({"model": model, "messages": messages});
    if let Some(ev) = effort_value(p, call.effort) {
        body["reasoning_effort"] = json!(ev);
    }
    let mut req = http(timeout)?.post(format!("{base}/chat/completions")).json(&body);
    if !p.api_key.trim().is_empty() {
        req = req.bearer_auth(p.api_key.trim());
    } else if !base.contains("localhost") && !base.contains("127.0.0.1") {
        need_key(p)?;
    }
    let v = read_json(req.send().await?).await?;
    v.pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow!("Phản hồi không có nội dung"))
}

async fn invoke_gemini(p: &Provider, call: &Call<'_>, timeout: Duration) -> Result<String> {
    need_key(p)?;
    let model = effective_model(p, call.model);
    let base = p.base_url.trim_end_matches('/');
    let prompt = format!("{}{}", call.prompt, prompt_hints(p, call.effort, call.web));
    let mut body = json!({"contents": [{"role": "user", "parts": [{"text": prompt}]}]});
    if !call.system.trim().is_empty() {
        body["systemInstruction"] = json!({"parts": [{"text": call.system}]});
    }
    if EFFORTS.contains(&call.effort) {
        // Gemini 2.5 dùng ngân sách token suy nghĩ; Gemini 3.x dùng mức (low/high)
        body["generationConfig"] = if model.starts_with("gemini-2.5") {
            let budget = match call.effort { "low" => 1024, "medium" => 8192, "high" => 16384, "xhigh" => 24576, _ => -1 };
            json!({"thinkingConfig": {"thinkingBudget": budget}})
        } else {
            json!({"thinkingConfig": {"thinkingLevel": if matches!(call.effort, "low" | "medium") { "low" } else { "high" }}})
        };
    }
    if call.web {
        body["tools"] = json!([{"google_search": {}}]);
    }
    let resp = http(timeout)?
        .post(format!("{base}/models/{model}:generateContent"))
        .header("x-goog-api-key", p.api_key.trim())
        .json(&body)
        .send()
        .await?;
    let v = read_json(resp).await?;
    let parts: Vec<&str> = v
        .pointer("/candidates/0/content/parts")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|x| x["text"].as_str()).collect())
        .unwrap_or_default();
    if parts.is_empty() {
        return Err(anyhow!("Gemini không trả về nội dung"));
    }
    Ok(parts.join(""))
}

async fn invoke_mock(call: &Call<'_>) -> Result<String> {
    let report = |s: &str| {
        if let Some(cb) = &call.on_progress {
            cb(s.to_string());
        }
    };
    let ms = 1500 + (rand::random::<u64>() % 2500);
    report("▶️ Bắt đầu (mô phỏng)");
    tokio::time::sleep(Duration::from_millis(ms / 3)).await;
    report("🔧 Đọc yêu cầu và kết quả của đồng nghiệp");
    tokio::time::sleep(Duration::from_millis(ms / 3)).await;
    report("✍️ Đang soạn kết quả…");
    tokio::time::sleep(Duration::from_millis(ms / 3)).await;
    // mô phỏng việc hỏi lại: yêu cầu có "(cần hỏi)" mà chưa có câu trả lời của quản lý
    if call.prompt.contains("(cần hỏi)") && !call.prompt.contains("[Quản lý trả lời") {
        return Ok(format!("{} Bạn muốn kết quả trình bày cho đối tượng nào và dài khoảng bao nhiêu trang?", QUESTION_MARK));
    }
    let first_line = call
        .prompt
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .unwrap_or("nhiệm vụ")
        .trim();
    Ok(format!(
        "## Báo cáo của {persona}\n\n\
         > *(Chế độ mô phỏng — hãy kết nối Claude/Codex/Gemini để có kết quả thật.)*\n\n\
         **Việc được giao:** {task}\n\n\
         **Mức suy luận:** {effort}{web}\n\n\
         ### Các bước đã thực hiện\n\
         1. Đọc và phân tích yêu cầu, xác định đầu ra cần có.\n\
         2. Thu thập thông tin liên quan và đối chiếu với kết quả của đồng nghiệp ở bước trước.\n\
         3. Soạn thảo kết quả, tự rà soát chất lượng.\n\n\
         ### Kết quả\n\
         - Đã hoàn thành phần việc theo đúng chuyên môn.\n\
         - Đề xuất bước tiếp theo: quản lý xem xét và phản hồi.\n",
        persona = if call.persona.is_empty() { "nhân viên" } else { call.persona },
        task = tail(first_line, 200),
        effort = effort_label(call.effort),
        web = if call.web { " · có tra cứu web" } else { "" },
    ))
}

/// Kiểm tra nhanh một kết nối: CLI thì chạy `--version`, API thì gửi câu hỏi ngắn.
pub async fn test_provider(p: &Provider, path_env: &str, workdir: &Path) -> Result<String> {
    match p.kind.as_str() {
        "cli" => {
            let exe = which(&p.command, path_env)
                .ok_or_else(|| anyhow!("Không tìm thấy lệnh \"{}\" trong PATH", p.command))?;
            let out = tokio::time::timeout(
                Duration::from_secs(20),
                tokio::process::Command::new(&exe)
                    .arg("--version")
                    .env("PATH", path_env)
                    .stdin(Stdio::null())
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .map_err(|_| anyhow!("Lệnh --version không phản hồi"))??;
            let v = strip_ansi(&String::from_utf8_lossy(&out.stdout));
            Ok(format!("Tìm thấy {} — {}", exe.display(), v.lines().next().unwrap_or("").trim()))
        }
        "mock" => Ok("Chế độ mô phỏng luôn sẵn sàng".into()),
        _ => {
            let (_tx, rx) = watch::channel(false);
            let call = Call {
                model: "",
                system: "",
                prompt: "Trả lời đúng một từ: OK",
                workdir,
                path_env,
                persona: "",
                extra_args: &[],
                effort: "",
                web: false,
                on_progress: None,
            };
            let r = invoke(p, &call, rx).await?;
            Ok(format!("Kết nối thành công — phản hồi: {}", tail(&r, 80)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &str, model_args: &str) -> Provider {
        let mut x = builtin_providers().into_iter().find(|p| p.id == "claude-cli").unwrap();
        x.args = args.into();
        x.model_args = model_args.into();
        x
    }

    #[test]
    fn args_with_model() {
        let p = cli("-p {prompt} --output-format text {model_args}", "--model {model}");
        let a = build_cli_args(&p, "opus", "xin chào \"bạn\"", "/tmp/o", "/w", &[]).unwrap();
        assert_eq!(a, vec!["-p", "xin chào \"bạn\"", "--output-format", "text", "--model", "opus"]);
    }

    #[test]
    fn args_without_model_drop_model_tokens() {
        let p = cli("exec --output-last-message {output_file} {model_args} {prompt}", "-m {model}");
        let a = build_cli_args(&p, "", "việc", "/tmp/o.txt", "/w", &[]).unwrap();
        assert_eq!(a, vec!["exec", "--output-last-message", "/tmp/o.txt", "việc"]);
    }

    #[test]
    fn stdin_mode_omits_prompt() {
        let mut p = cli("-p {model_args} {prompt}", "--model {model}");
        p.use_stdin = true;
        assert_eq!(build_cli_args(&p, "", "x", "", "", &[]).unwrap(), vec!["-p"]);
    }

    #[test]
    fn extra_args_go_before_prompt() {
        let p = builtin_providers().into_iter().find(|p| p.id == "claude-cli").unwrap();
        let extra = extra_cli_args(&p, true, &["/data/tep".into()]);
        assert_eq!(extra, vec!["--permission-mode", "acceptEdits", "--add-dir", "/data/tep"]);
        let a = build_cli_args(&p, "", "việc", "", "", &extra).unwrap();
        assert_eq!(a, vec!["-p", "--permission-mode", "acceptEdits", "--add-dir", "/data/tep", "việc", "--output-format", "stream-json", "--verbose"]);
        let api = builtin_providers().into_iter().find(|p| p.id == "anthropic-api").unwrap();
        assert!(extra_cli_args(&api, true, &[]).is_empty());
    }

    #[test]
    fn effort_and_web_args_per_vendor() {
        let all = builtin_providers();
        let get = |id: &str| all.iter().find(|p| p.id == id).unwrap().clone();
        let claude = get("claude-cli");
        assert_eq!(cli_extra(&claude, false, &[], "xhigh", true), vec!["--effort", "xhigh", "--allowedTools", "WebSearch,WebFetch"]);
        let codex = get("codex-cli");
        assert_eq!(cli_extra(&codex, false, &[], "max", false), vec!["-c", "model_reasoning_effort=high"]);
        let gemini = get("gemini-cli");
        assert!(cli_extra(&gemini, false, &[], "high", true).is_empty());
        assert!(prompt_hints(&gemini, "high", true).contains("CAO") && prompt_hints(&gemini, "high", true).contains("tìm kiếm web"));
        assert!(prompt_hints(&claude, "high", false).is_empty());
        assert_eq!(effort_value(&get("openai-api"), "max").as_deref(), Some("high"));
        assert_eq!(effort_value(&get("anthropic-api"), "max").as_deref(), Some("max"));
        assert!(effort_value(&claude, "").is_none());
    }

    #[tokio::test]
    async fn stream_json_progress_and_result() {
        // CLI giả in ra luồng sự kiện giống Claude Code
        let mut p = cli("-c {prompt}", "");
        p.command = "/bin/sh".into();
        let script = r#"echo '{"type":"system","subtype":"init","model":"opus"}'; echo '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/tmp/bao-cao.md"}}]}}'; echo '{"type":"assistant","message":{"content":[{"type":"text","text":"Đang viết phần mở đầu"}]}}'; echo '{"type":"result","subtype":"success","result":"KẾT QUẢ CUỐI","is_error":false}'"#;
        let lines = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let l2 = lines.clone();
        let dir = tempfile::tempdir().unwrap();
        let call = Call { model: "", system: "", prompt: script, workdir: dir.path(), path_env: "/bin:/usr/bin", persona: "", extra_args: &[], effort: "", web: false,
            on_progress: Some(std::sync::Arc::new(move |s| l2.lock().unwrap().push(s))) };
        let (_tx, rx) = watch::channel(false);
        assert_eq!(invoke(&p, &call, rx).await.unwrap(), "KẾT QUẢ CUỐI");
        let got = lines.lock().unwrap().join("\n");
        assert!(got.contains("Đọc tệp: /tmp/bao-cao.md") && got.contains("Đang viết phần mở đầu") && got.contains("Hoàn tất"), "{got}");
    }

    #[test]
    fn question_detection() {
        assert_eq!(extract_question("[[HỎI]] Báo cáo cho ai đọc?").as_deref(), Some("Báo cáo cho ai đọc?"));
        assert!(extract_question("Kết quả bình thường").is_none());
        let long = format!("{} và tôi có nhắc [[HỎI]] ở đây", "x".repeat(900));
        assert!(extract_question(&long).is_none());
    }

    #[test]
    fn ansi_is_stripped() {
        assert_eq!(strip_ansi("\u{1b}[31mĐỏ\u{1b}[0m"), "Đỏ");
    }

    #[test]
    fn which_finds_sh() {
        assert!(which("sh", "/bin:/usr/bin").is_some());
        assert!(which("khong-ton-tai-xyz", "/bin").is_none());
    }

    #[tokio::test]
    async fn custom_cli_roundtrip_and_cancel() {
        // Dùng /bin/echo như một "AI" CLI tuỳ biến.
        let mut p = cli("{prompt}", "");
        p.command = "/bin/echo".into();
        let dir = tempfile::tempdir().unwrap();
        let call = Call { model: "", system: "", prompt: "chào văn phòng", workdir: dir.path(), path_env: "/bin:/usr/bin", persona: "", extra_args: &[], effort: "", web: false, on_progress: None };
        let (_tx, rx) = watch::channel(false);
        assert_eq!(invoke(&p, &call, rx).await.unwrap(), "chào văn phòng");

        let mut slow = cli("{prompt}", "");
        slow.command = "/bin/sleep".into();
        let call = Call { prompt: "5", ..call };
        let (tx, rx) = watch::channel(false);
        let h = tokio::spawn(async move { tokio::time::sleep(Duration::from_millis(200)).await; tx.send(true).unwrap(); });
        let t = std::time::Instant::now();
        let err = invoke(&slow, &call, rx).await.unwrap_err().to_string();
        h.await.unwrap();
        assert!(err.contains("huỷ"));
        assert!(t.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn failing_cli_reports_error() {
        let mut p = cli("-c {prompt}", "");
        p.command = "/bin/sh".into();
        let dir = tempfile::tempdir().unwrap();
        let call = Call { model: "", system: "", prompt: "echo hỏng >&2; exit 3", workdir: dir.path(), path_env: "/bin:/usr/bin", persona: "", extra_args: &[], effort: "", web: false, on_progress: None };
        let (_tx, rx) = watch::channel(false);
        let e = invoke(&p, &call, rx).await.unwrap_err().to_string();
        assert!(e.contains("mã 3") && e.contains("hỏng"), "{e}");
    }
}
