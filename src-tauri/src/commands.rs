//! Các lệnh Tauri gọi từ giao diện. Mọi lệnh (trừ đăng nhập/khởi tạo) đều kiểm tra phiên và quyền.

use crate::auth::{self, ROLE_COORDINATOR, ROLE_MANAGER, ROLE_VIEWER};
use crate::state::Core;
use crate::db::{self, Agent, Provider, ProviderView, User};
use crate::engine;
use crate::providers;
use crate::templates::{self, Candidate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

#[derive(Serialize)]
pub struct LoginResult {
    token: String,
    user: User,
}

// ───────────────────────────── Phiên & tài khoản ─────────────────────────────

#[tauri::command]
pub fn app_info(core: State<'_, Core>) -> R<Value> {
    let c = core.db();
    Ok(json!({
        "needs_setup": db::count_users(&c).map_err(e)? == 0,
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir": core.0.data_dir.to_string_lossy(),
        "office_name": db::get_setting(&c, "office_name").map_err(e)?.unwrap_or_default(),
        "office_style": db::get_setting(&c, "office_style").map_err(e)?.unwrap_or_else(|| "hien_dai".into()),
    }))
}

const STYLES: &[&str] = &["hien_dai", "go_am", "cong_nghe", "sang_tao", "hanh_chinh"];

#[tauri::command]
pub fn set_office_style(core: State<'_, Core>, token: String, style: String) -> R<()> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if !STYLES.contains(&style.as_str()) {
        return Err("Phong cách không hợp lệ".into());
    }
    db::set_setting(&core.db(), "office_style", &style).map_err(e)?;
    core.log("he_thong", &me.display_name, &format!("Đổi phong cách văn phòng: {style}"), None, None);
    core.emit(json!({"type": "style", "style": style}));
    Ok(())
}

/// Mở tệp/thư mục (reveal = hiện tệp trong Finder thay vì mở bằng ứng dụng mặc định).
#[tauri::command]
pub fn open_path(core: State<'_, Core>, token: String, path: String, reveal: Option<bool>) -> R<()> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("Đường dẫn không tồn tại".into());
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        if p.is_file() && reveal.unwrap_or(true) {
            cmd.arg("-R");
        }
        cmd.arg(p).spawn().map_err(e)?;
    }
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(p).spawn().map_err(e)?;
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("xdg-open").arg(p).spawn().map_err(e)?;
    Ok(())
}

#[tauri::command]
pub fn mission_folder(core: State<'_, Core>, token: String, id: i64) -> R<String> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let d = crate::workspace::mission_dir(&core, id);
    std::fs::create_dir_all(&d).map_err(e)?;
    Ok(d.to_string_lossy().to_string())
}

#[tauri::command]
pub fn setup_manager(core: State<'_, Core>, username: String, display_name: String, password: String, office_style: Option<String>) -> R<LoginResult> {
    if let Some(st) = office_style.filter(|s| STYLES.contains(&s.as_str())) {
        db::set_setting(&core.db(), "office_style", &st).map_err(e)?;
    }
    let username = username.trim().to_string();
    auth::check_username(&username).map_err(e)?;
    auth::check_password_policy(&password).map_err(e)?;
    let c = core.db();
    if db::count_users(&c).map_err(e)? > 0 {
        return Err("Hệ thống đã được khởi tạo".into());
    }
    let name = if display_name.trim().is_empty() { username.clone() } else { display_name.trim().to_string() };
    let id = db::insert_user(&c, &username, &name, &auth::hash_password(&password).map_err(e)?, ROLE_MANAGER).map_err(e)?;
    db::log(&c, "he_thong", &name, "Tạo tài khoản quản lý đầu tiên", None, None).map_err(e)?;
    let user = db::get_user(&c, id).map_err(e)?.ok_or("lỗi")?;
    drop(c);
    let token = core.sessions().create(id);
    Ok(LoginResult { token, user })
}

#[tauri::command]
pub fn login(core: State<'_, Core>, username: String, password: String) -> R<LoginResult> {
    let username = username.trim().to_string();
    core.sessions().check_lock(&username).map_err(e)?;
    let row = db::user_auth_row(&core.db(), &username).map_err(e)?;
    match row {
        Some((user, hash)) if auth::verify_password(&password, &hash) => {
            if !user.active {
                return Err("Tài khoản đã bị khoá".into());
            }
            let mut s = core.sessions();
            s.clear_fail(&username);
            let token = s.create(user.id);
            drop(s);
            core.log("dang_nhap", &user.display_name, "Đăng nhập", None, None);
            Ok(LoginResult { token, user })
        }
        _ => {
            core.sessions().record_fail(&username);
            Err("Sai tên đăng nhập hoặc mật khẩu".into())
        }
    }
}

#[tauri::command]
pub fn logout(core: State<'_, Core>, token: String) -> R<()> {
    core.sessions().remove(&token);
    Ok(())
}

#[tauri::command]
pub fn me(core: State<'_, Core>, token: String) -> R<User> {
    core.auth(&token, ROLE_VIEWER).map_err(e)
}

#[tauri::command]
pub fn change_password(core: State<'_, Core>, token: String, old_password: String, new_password: String) -> R<()> {
    let u = core.auth(&token, ROLE_VIEWER).map_err(e)?;
    auth::check_password_policy(&new_password).map_err(e)?;
    let c = core.db();
    let h = db::password_hash_of(&c, u.id).map_err(e)?;
    if !auth::verify_password(&old_password, &h) {
        return Err("Mật khẩu hiện tại không đúng".into());
    }
    db::set_password(&c, u.id, &auth::hash_password(&new_password).map_err(e)?).map_err(e)
}

#[tauri::command]
pub fn list_users(core: State<'_, Core>, token: String) -> R<Vec<User>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    db::list_users(&core.db()).map_err(e)
}

#[derive(Deserialize)]
pub struct UserInput {
    id: Option<i64>,
    username: String,
    display_name: String,
    role: String,
    active: bool,
    #[serde(default)]
    password: String,
}

#[tauri::command]
pub fn save_user(core: State<'_, Core>, token: String, user: UserInput) -> R<i64> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if !auth::valid_role(&user.role) {
        return Err("Vai trò không hợp lệ".into());
    }
    let c = core.db();
    let display = if user.display_name.trim().is_empty() { user.username.trim().to_string() } else { user.display_name.trim().to_string() };
    match user.id {
        None => {
            auth::check_username(user.username.trim()).map_err(e)?;
            auth::check_password_policy(&user.password).map_err(e)?;
            let id = db::insert_user(&c, user.username.trim(), &display, &auth::hash_password(&user.password).map_err(e)?, &user.role).map_err(e)?;
            db::log(&c, "nguoi_dung", &me.display_name, &format!("Tạo tài khoản {} ({})", display, user.role), None, None).map_err(e)?;
            Ok(id)
        }
        Some(id) => {
            let old = db::get_user(&c, id).map_err(e)?.ok_or("Không tìm thấy tài khoản")?;
            let demoting = old.role == ROLE_MANAGER && (user.role != ROLE_MANAGER || !user.active);
            if demoting && db::count_active_managers(&c).map_err(e)? <= 1 {
                return Err("Phải còn ít nhất một quản lý đang hoạt động".into());
            }
            db::update_user(&c, id, &display, &user.role, user.active).map_err(e)?;
            if !user.password.is_empty() {
                auth::check_password_policy(&user.password).map_err(e)?;
                db::set_password(&c, id, &auth::hash_password(&user.password).map_err(e)?).map_err(e)?;
            }
            drop(c);
            if !user.active || !user.password.is_empty() {
                core.sessions().remove_user(id);
            }
            core.log("nguoi_dung", &me.display_name, &format!("Cập nhật tài khoản {display}"), None, None);
            Ok(id)
        }
    }
}

#[tauri::command]
pub fn delete_user(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if me.id == id {
        return Err("Không thể tự xoá tài khoản của mình".into());
    }
    let c = core.db();
    let u = db::get_user(&c, id).map_err(e)?.ok_or("Không tìm thấy tài khoản")?;
    if u.role == ROLE_MANAGER && db::count_active_managers(&c).map_err(e)? <= 1 {
        return Err("Phải còn ít nhất một quản lý".into());
    }
    db::delete_user(&c, id).map_err(e)?;
    drop(c);
    core.sessions().remove_user(id);
    core.log("nguoi_dung", &me.display_name, &format!("Xoá tài khoản {}", u.display_name), None, None);
    Ok(())
}

// ───────────────────────────── Cài đặt văn phòng ─────────────────────────────

#[tauri::command]
pub fn get_settings(core: State<'_, Core>, token: String) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let c = core.db();
    Ok(json!({
        "office_name": db::get_setting(&c, "office_name").map_err(e)?.unwrap_or_default(),
        "office_description": db::get_setting(&c, "office_description").map_err(e)?.unwrap_or_default(),
        "onboarded": db::get_setting(&c, "onboarded").map_err(e)?.unwrap_or_default() == "1",
    }))
}

#[tauri::command]
pub fn save_settings(core: State<'_, Core>, token: String, office_name: String, office_description: String) -> R<()> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    let c = core.db();
    db::set_setting(&c, "office_name", office_name.trim()).map_err(e)?;
    db::set_setting(&c, "office_description", office_description.trim()).map_err(e)
}

// ───────────────────────────── Kết nối AI ─────────────────────────────

#[tauri::command]
pub fn list_providers(core: State<'_, Core>, token: String) -> R<Vec<ProviderView>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    Ok(db::list_providers(&core.db())
        .map_err(e)?
        .into_iter()
        .map(|p| ProviderView { has_key: !p.api_key.is_empty(), p })
        .collect())
}

#[tauri::command]
pub fn save_provider(core: State<'_, Core>, token: String, provider: Provider) -> R<String> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if !["cli", "anthropic", "openai", "gemini", "mock"].contains(&provider.kind.as_str()) {
        return Err("Loại kết nối không hợp lệ".into());
    }
    if provider.name.trim().is_empty() {
        return Err("Tên kết nối không được để trống".into());
    }
    if provider.kind == "cli" && provider.command.trim().is_empty() {
        return Err("Cần nhập lệnh CLI".into());
    }
    if provider.kind == "cli" {
        shell_words::split(&provider.args).map_err(|x| format!("Mẫu tham số lỗi: {x}"))?;
    }
    let c = core.db();
    let mut p = provider;
    let existing = if p.id.is_empty() { None } else { db::get_provider(&c, &p.id).map_err(e)? };
    match &existing {
        Some(old) => p.builtin = old.builtin,
        None => {
            let slug: String = templates::fold(&p.name)
                .chars()
                .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
                .collect();
            p.id = format!("{}-{}", slug.trim_matches('-'), rand::random::<u16>());
            p.builtin = false;
        }
    }
    p.timeout_secs = p.timeout_secs.clamp(10, 7200);
    db::upsert_provider(&c, &p, true).map_err(e)?;
    db::log(&c, "ket_noi", &me.display_name, &format!("Lưu kết nối AI \"{}\"", p.name), None, None).map_err(e)?;
    Ok(p.id)
}

#[tauri::command]
pub fn delete_provider(core: State<'_, Core>, token: String, id: String) -> R<()> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    let c = core.db();
    let p = db::get_provider(&c, &id).map_err(e)?.ok_or("Không tìm thấy")?;
    if p.builtin {
        return Err("Không thể xoá kết nối dựng sẵn (có thể tắt đi)".into());
    }
    db::delete_provider(&c, &id).map_err(e)?;
    drop(c);
    core.emit(json!({"type": "agents_changed"}));
    Ok(())
}

#[tauri::command]
pub async fn test_provider(core: State<'_, Core>, token: String, id: String, live: Option<bool>) -> R<String> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    let p = db::get_provider(&core.db(), &id).map_err(e)?.ok_or("Không tìm thấy")?;
    let wd = core.0.data_dir.join("workspaces").join("kiem-tra");
    if live.unwrap_or(false) && p.kind == "cli" {
        // Gọi thật một câu hỏi rất ngắn để chắc chắn CLI đã đăng nhập.
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let path_env = core.path_env();
        let call = providers::Call { model: "", system: "", prompt: "Trả lời đúng một từ: OK", workdir: &wd, path_env: &path_env, persona: "", extra_args: &[], effort: "", web: false, on_progress: None };
        let mut quick = p.clone();
        quick.timeout_secs = 120;
        let r = providers::invoke(&quick, &call, rx).await.map_err(e)?;
        return Ok(format!("{} trả lời: {}", p.name, r.chars().take(80).collect::<String>()));
    }
    providers::test_provider(&p, &core.path_env(), &wd).await.map_err(e)
}

#[tauri::command]
pub fn detect_clis(core: State<'_, Core>, token: String) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let path = core.path_env();
    let list = db::list_providers(&core.db()).map_err(e)?;
    let mut out = serde_json::Map::new();
    for p in list.into_iter().filter(|p| p.kind == "cli") {
        out.insert(p.id.clone(), json!(providers::which(&p.command, &path).map(|x| x.to_string_lossy().to_string())));
    }
    Ok(Value::Object(out))
}

#[tauri::command]
pub fn refresh_path(core: State<'_, Core>, token: String) -> R<()> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    let p = providers::resolve_path_env();
    if let Ok(mut w) = core.0.path_env.write() {
        *w = p;
    }
    Ok(())
}

// ───────────────────────────── Nhân viên ─────────────────────────────

#[tauri::command]
pub fn list_agents(core: State<'_, Core>, token: String) -> R<Vec<Agent>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    db::list_agents(&core.db()).map_err(e)
}

#[tauri::command]
pub fn list_templates(core: State<'_, Core>, token: String) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    Ok(json!({
        "templates": templates::library(),
        "avatars": templates::AVATARS,
        "levels": templates::LEVELS.iter().map(|(k, v)| json!({"key": k, "label": v})).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
pub struct AgentInput {
    id: Option<i64>,
    name: String,
    title: String,
    #[serde(default)]
    department: String,
    level: String,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    system_prompt: String,
    provider_id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    effort: String,
    avatar: String,
    #[serde(default)]
    color: String,
}

#[tauri::command]
pub fn save_agent(core: State<'_, Core>, token: String, agent: AgentInput) -> R<i64> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if agent.name.trim().is_empty() || agent.title.trim().is_empty() {
        return Err("Cần nhập tên và chức danh".into());
    }
    if !templates::valid_avatar(&agent.avatar) {
        return Err("Hình đại diện không hợp lệ".into());
    }
    let c = core.db();
    if db::get_provider(&c, &agent.provider_id).map_err(e)?.is_none() {
        return Err("Kết nối AI không tồn tại".into());
    }
    let mut a = Agent {
        id: agent.id.unwrap_or(0),
        name: agent.name.trim().into(),
        title: agent.title.trim().into(),
        department: agent.department.trim().into(),
        level: templates::normalize_level(&agent.level),
        skills: agent.skills.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        system_prompt: agent.system_prompt,
        provider_id: agent.provider_id,
        model: agent.model.trim().into(),
        avatar: agent.avatar,
        color: if agent.color.starts_with('#') { agent.color } else { "#6366f1".into() },
        is_dispatcher: false,
        desk: 0,
        status: "idle".into(),
        tasks_done: 0,
        created_at: String::new(),
        effort: engine::norm_effort(&agent.effort),
    };
    let id = match agent.id {
        Some(id) => {
            let old = db::get_agent(&c, id).map_err(e)?.ok_or("Không tìm thấy nhân viên")?;
            a.is_dispatcher = old.is_dispatcher;
            db::update_agent(&c, &a).map_err(e)?;
            db::log(&c, "nhan_su", &me.display_name, &format!("Cập nhật hồ sơ {}", a.name), None, Some(id)).map_err(e)?;
            id
        }
        None => {
            let id = db::insert_agent(&c, &a).map_err(e)?;
            db::log(&c, "nhan_su", &me.display_name, &format!("Thêm nhân viên {} — {}", a.name, a.title), None, Some(id)).map_err(e)?;
            id
        }
    };
    drop(c);
    core.emit(json!({"type": "agents_changed"}));
    Ok(id)
}

#[tauri::command]
pub fn delete_agent(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    let c = core.db();
    let a = db::get_agent(&c, id).map_err(e)?.ok_or("Không tìm thấy nhân viên")?;
    if a.is_dispatcher {
        return Err("Không thể xoá Điều phối viên (có thể sửa hồ sơ)".into());
    }
    if a.status == "working" {
        return Err("Nhân viên đang làm việc, hãy huỷ nhiệm vụ trước".into());
    }
    db::delete_agent(&c, id).map_err(e)?;
    db::log(&c, "nhan_su", &me.display_name, &format!("Cho thôi việc {} — {}", a.name, a.title), None, None).map_err(e)?;
    drop(c);
    core.emit(json!({"type": "agents_changed"}));
    Ok(())
}

#[derive(Serialize)]
pub struct RecruitOut {
    candidates: Vec<Candidate>,
    source: String,
    note: String,
}

#[tauri::command]
pub async fn recruit(core: State<'_, Core>, token: String, job_description: String, count: usize, provider_id: Option<String>, optimize: Option<bool>) -> R<RecruitOut> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if job_description.trim().chars().count() < 10 {
        return Err("Hãy mô tả công việc chi tiết hơn (ít nhất 10 ký tự)".into());
    }
    {
        let c = core.db();
        if db::get_setting(&c, "office_description").map_err(e)?.unwrap_or_default().is_empty() {
            db::set_setting(&c, "office_description", job_description.trim()).map_err(e)?;
        }
    }
    core.log("tuyen_dung", &me.display_name, "Yêu cầu AI đề xuất ứng viên", None, None);
    let r = engine::recruit(&core, &job_description, count, provider_id, optimize.unwrap_or(true)).await.map_err(e)?;
    Ok(RecruitOut { candidates: r.candidates, source: r.source, note: r.note })
}

#[tauri::command]
pub fn hire(core: State<'_, Core>, token: String, candidates: Vec<Candidate>) -> R<Vec<i64>> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if candidates.is_empty() {
        return Err("Chưa chọn ứng viên nào".into());
    }
    engine::hire(&core, &candidates, &me.display_name).map_err(e)
}

// ───────────────────────────── Nhiệm vụ ─────────────────────────────

#[tauri::command]
pub fn list_missions(core: State<'_, Core>, token: String) -> R<Vec<Value>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let c = core.db();
    let ms = db::list_missions(&c).map_err(e)?;
    let mut out = vec![];
    for m in ms {
        let subs = db::list_subtasks(&c, m.id).map_err(e)?;
        let done = subs.iter().filter(|s| s.status == "xong").count();
        let agents: Vec<i64> = subs.iter().filter_map(|s| s.agent_id).collect();
        let mut v = serde_json::to_value(&m).map_err(e)?;
        v["total"] = json!(subs.len());
        v["done"] = json!(done);
        v["agent_ids"] = json!(agents);
        v["running"] = json!(core.is_running(m.id));
        v["summary"] = json!(""); // danh sách không cần nội dung dài
        out.push(v);
    }
    Ok(out)
}

#[tauri::command]
pub fn get_mission(core: State<'_, Core>, token: String, id: i64) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let c = core.db();
    let m = db::get_mission(&c, id).map_err(e)?.ok_or("Không tìm thấy nhiệm vụ")?;
    let subs = db::list_subtasks(&c, id).map_err(e)?;
    Ok(json!({"mission": m, "subtasks": subs, "running": core.is_running(id)}))
}

#[derive(Deserialize)]
pub struct MissionInput {
    title: String,
    description: String,
    #[serde(default = "prio")]
    priority: String,
    #[serde(default = "mode")]
    mode: String,
    coordinator_id: Option<i64>,
    #[serde(default)]
    deadline: String,
    /// Thư mục làm việc (kéo thả thư mục vào hoặc chọn bằng hộp thoại)
    #[serde(default)]
    workdir: String,
    #[serde(default)]
    attachments: Vec<String>,
    #[serde(default)]
    allow_edit: bool,
    /// Chạy theo quy trình nhiều AI (mode = "pipeline")
    #[serde(default)]
    pipeline_id: Option<i64>,
    /// Chạy ngay sau khi tạo các bước (không thì chờ duyệt)
    #[serde(default)]
    pipeline_run: bool,
    /// Mẫu kết quả: "auto" (tự chọn theo nội dung, mặc định) | "none" | "pick" (dùng template_id)
    #[serde(default)]
    template_mode: String,
    #[serde(default)]
    template_id: Option<i64>,
}

/// Chọn mẫu kết quả cho nhiệm vụ theo yêu cầu của người giao việc.
fn pick_template(c: &rusqlite::Connection, mode: &str, id: Option<i64>, text: &str) -> Option<i64> {
    match mode {
        "none" => None,
        "pick" => id.filter(|t| crate::memory::get_template(c, *t).ok().flatten().is_some()),
        _ => crate::memory::match_template(c, text).ok().flatten().map(|t| t.id),
    }
}
fn prio() -> String {
    "trung_binh".into()
}
fn mode() -> String {
    "auto".into()
}

fn check_priority(p: &str) -> R<()> {
    if ["thap", "trung_binh", "cao", "khan"].contains(&p) {
        Ok(())
    } else {
        Err("Mức ưu tiên không hợp lệ".into())
    }
}

#[tauri::command]
pub async fn create_mission(core: State<'_, Core>, token: String, mission: MissionInput) -> R<i64> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    if mission.title.trim().is_empty() || mission.description.trim().is_empty() {
        return Err("Cần nhập tiêu đề và mô tả nhiệm vụ".into());
    }
    check_priority(&mission.priority)?;
    if !["auto", "review", "manual", "pipeline"].contains(&mission.mode.as_str()) {
        return Err("Chế độ không hợp lệ".into());
    }
    let id = {
        let c = core.db();
        if let Some(cid) = mission.coordinator_id {
            let u = db::get_user(&c, cid).map_err(e)?.ok_or("Người phân công không tồn tại")?;
            if auth::role_rank(&u.role) < auth::role_rank(ROLE_COORDINATOR) {
                return Err("Người phân công phải có vai trò Điều phối hoặc Quản lý".into());
            }
        }
        crate::workspace::validate(&mission.workdir, &mission.attachments).map_err(e)?;
        let id = db::insert_mission(&c, mission.title.trim(), mission.description.trim(), &mission.priority, &mission.mode, me.id, mission.coordinator_id, mission.deadline.trim())
            .map_err(e)?;
        db::set_mission_files(&c, id, mission.workdir.trim(), &mission.attachments, mission.allow_edit).map_err(e)?;
        let tpl = pick_template(&c, &mission.template_mode, mission.template_id, &format!("{} {}", mission.title, mission.description));
        db::set_mission_template(&c, id, tpl).map_err(e)?;
        id
    };
    core.log("giao_viec", &me.display_name, &format!("Giao nhiệm vụ \"{}\"", mission.title.trim()), Some(id), None);
    match mission.mode.as_str() {
        "auto" => engine::start(&core, id, true, true).map_err(e)?,
        "review" => engine::start(&core, id, true, false).map_err(e)?,
        "pipeline" => {
            let pid = mission.pipeline_id.ok_or("Chưa chọn quy trình")?;
            engine::apply_pipeline(&core, id, pid).await.map_err(e)?;
            if mission.pipeline_run {
                engine::start(&core, id, false, true).map_err(e)?;
            } else {
                db::set_mission_status(&core.db(), id, "cho_duyet").map_err(e)?;
            }
        }
        _ => {}
    }
    core.emit(json!({"type": "mission", "id": id, "status": "moi"}));
    Ok(id)
}

#[tauri::command]
pub fn update_mission(core: State<'_, Core>, token: String, id: i64, mission: MissionInput) -> R<()> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    check_priority(&mission.priority)?;
    if core.is_running(id) {
        return Err("Nhiệm vụ đang chạy".into());
    }
    crate::workspace::validate(&mission.workdir, &mission.attachments).map_err(e)?;
    let c = core.db();
    db::update_mission_meta(&c, id, mission.title.trim(), mission.description.trim(), &mission.priority, mission.coordinator_id, mission.deadline.trim())
        .map_err(e)?;
    db::set_mission_files(&c, id, mission.workdir.trim(), &mission.attachments, mission.allow_edit).map_err(e)?;
    drop(c);
    core.emit(json!({"type": "mission", "id": id}));
    Ok(())
}

#[tauri::command]
pub fn delete_mission(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    if core.is_running(id) {
        return Err("Nhiệm vụ đang chạy, hãy huỷ trước".into());
    }
    db::delete_mission(&core.db(), id).map_err(e)?;
    core.log("giao_viec", &me.display_name, &format!("Xoá nhiệm vụ #{id}"), None, None);
    core.emit(json!({"type": "mission", "id": id, "deleted": true}));
    Ok(())
}

#[tauri::command]
pub fn plan_mission(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    engine::start(&core, id, true, false).map_err(e)
}

#[tauri::command]
pub fn run_mission(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    let has_subs = !db::list_subtasks(&core.db(), id).map_err(e)?.is_empty();
    core.log("giao_viec", &me.display_name, &format!("Duyệt và cho chạy nhiệm vụ #{id}"), Some(id), None);
    engine::start(&core, id, !has_subs, true).map_err(e)
}

#[tauri::command]
pub fn cancel_mission(core: State<'_, Core>, token: String, id: i64) -> R<bool> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    Ok(engine::cancel(&core, id))
}

#[derive(Deserialize)]
pub struct SubtaskInput {
    id: Option<i64>,
    mission_id: i64,
    agent_id: Option<i64>,
    step: i64,
    title: String,
    #[serde(default)]
    instruction: String,
    /// AI riêng cho bước (rỗng = theo nhân viên)
    #[serde(default)]
    provider_id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    effort: String,
    #[serde(default)]
    web: bool,
}

#[tauri::command]
pub fn save_subtask(core: State<'_, Core>, token: String, subtask: SubtaskInput) -> R<i64> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    if core.is_running(subtask.mission_id) {
        return Err("Nhiệm vụ đang chạy, không sửa được phân công".into());
    }
    if subtask.title.trim().is_empty() {
        return Err("Cần nhập tên đầu việc".into());
    }
    let c = core.db();
    db::get_mission(&c, subtask.mission_id).map_err(e)?.ok_or("Không tìm thấy nhiệm vụ")?;
    if let Some(a) = subtask.agent_id {
        db::get_agent(&c, a).map_err(e)?.ok_or("Nhân viên không tồn tại")?;
    }
    let step = subtask.step.clamp(1, 10);
    let id = match subtask.id {
        Some(id) => {
            db::update_subtask_plan(&c, id, subtask.agent_id, step, subtask.title.trim(), subtask.instruction.trim()).map_err(e)?;
            id
        }
        None => db::insert_subtask(&c, subtask.mission_id, subtask.agent_id, step, subtask.title.trim(), subtask.instruction.trim()).map_err(e)?,
    };
    if !subtask.provider_id.is_empty() && db::get_provider(&c, &subtask.provider_id).map_err(e)?.is_none() {
        return Err("Kết nối AI của bước không tồn tại".into());
    }
    db::set_subtask_ai(&c, id, &subtask.provider_id, subtask.model.trim(), &engine::norm_effort(&subtask.effort), subtask.web).map_err(e)?;
    let st = db::get_mission(&c, subtask.mission_id).map_err(e)?.map(|m| m.status).unwrap_or_default();
    if st == "hoan_thanh" || st == "moi" {
        db::set_mission_status(&c, subtask.mission_id, "cho_duyet").map_err(e)?;
    }
    let agent_name = subtask.agent_id.and_then(|a| db::get_agent(&c, a).ok().flatten()).map(|a| a.name).unwrap_or_default();
    db::log(&c, "phan_cong", &me.display_name, &format!("Phân công \"{}\" cho {}", subtask.title.trim(), agent_name), Some(subtask.mission_id), subtask.agent_id)
        .map_err(e)?;
    drop(c);
    if let Some(a) = subtask.agent_id {
        let disp = db::dispatcher(&core.db()).ok().flatten().map(|d| d.id);
        core.emit(json!({"type": "assign", "from": disp, "to": a, "mission_id": subtask.mission_id, "title": subtask.title}));
    }
    core.emit(json!({"type": "mission", "id": subtask.mission_id}));
    Ok(id)
}

#[tauri::command]
pub fn delete_subtask(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    let c = core.db();
    let s = db::get_subtask(&c, id).map_err(e)?.ok_or("Không tìm thấy đầu việc")?;
    if core.is_running(s.mission_id) {
        return Err("Nhiệm vụ đang chạy".into());
    }
    db::delete_subtask(&c, id).map_err(e)?;
    drop(c);
    core.emit(json!({"type": "mission", "id": s.mission_id}));
    Ok(())
}

/// Giao việc nhanh trực tiếp cho một nhân viên (hỏi đáp/nhờ việc).
#[tauri::command]
pub fn ask_agent(core: State<'_, Core>, token: String, agent_id: i64, message: String, workdir: Option<String>, attachments: Option<Vec<String>>, allow_edit: Option<bool>) -> R<i64> {
    let workdir = workdir.unwrap_or_default();
    let attachments = attachments.unwrap_or_default();
    crate::workspace::validate(&workdir, &attachments).map_err(e)?;
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    let msg = message.trim();
    if msg.is_empty() {
        return Err("Nội dung trống".into());
    }
    let id = {
        let c = core.db();
        let a = db::get_agent(&c, agent_id).map_err(e)?.ok_or("Không tìm thấy nhân viên")?;
        let title: String = msg.chars().take(70).collect();
        let title = if msg.chars().count() > 70 { format!("{title}…") } else { title };
        let mid = db::insert_mission(&c, &title, msg, "trung_binh", "manual", me.id, None, "").map_err(e)?;
        db::insert_subtask(&c, mid, Some(agent_id), 1, &format!("Việc giao trực tiếp cho {}", a.name), msg).map_err(e)?;
        db::set_mission_files(&c, mid, workdir.trim(), &attachments, allow_edit.unwrap_or(false)).map_err(e)?;
        let tpl = pick_template(&c, "auto", None, msg);
        db::set_mission_template(&c, mid, tpl).map_err(e)?;
        mid
    };
    core.log("giao_viec", &me.display_name, "Giao việc trực tiếp", Some(id), Some(agent_id));
    engine::start(&core, id, false, true).map_err(e)?;
    Ok(id)
}

// ───────────────────────────── Nhật ký & tổng quan ─────────────────────────────

#[tauri::command]
pub fn list_activity(core: State<'_, Core>, token: String, limit: Option<i64>) -> R<Vec<db::Activity>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    db::list_activity(&core.db(), limit.unwrap_or(100).clamp(1, 1000)).map_err(e)
}

#[tauri::command]
pub fn dashboard(core: State<'_, Core>, token: String) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let c = core.db();
    let q = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    Ok(json!({
        "agents": q("SELECT COUNT(*) FROM agents"),
        "working": q("SELECT COUNT(*) FROM agents WHERE status IN ('working','thinking')"),
        "missions": q("SELECT COUNT(*) FROM missions"),
        "missions_done": q("SELECT COUNT(*) FROM missions WHERE status='hoan_thanh'"),
        "missions_active": q("SELECT COUNT(*) FROM missions WHERE status IN ('dang_lap_ke_hoach','dang_thuc_hien','dang_tong_hop')"),
        "missions_waiting": q("SELECT COUNT(*) FROM missions WHERE status IN ('moi','cho_duyet','cho_tra_loi')"),
        "subtasks_done": q("SELECT COUNT(*) FROM subtasks WHERE status='xong'"),
    }))
}

// ───────────────────────────── Thiết lập máy & tối ưu AI ─────────────────────────────

#[tauri::command]
pub async fn scan_machine(core: State<'_, Core>, token: String) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    crate::setup::scan(&core).await.map_err(e)
}

#[tauri::command]
pub fn install_cli(core: State<'_, Core>, token: String, id: String) -> R<()> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    core.log("ket_noi", &me.display_name, &format!("Bắt đầu cài đặt {id}"), None, None);
    crate::setup::start_install(&core, &id).map_err(e)
}

#[tauri::command]
pub fn open_cli_login(core: State<'_, Core>, token: String, id: String) -> R<()> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    crate::setup::open_login(&core, &id).map_err(e)
}

#[tauri::command]
pub async fn optimize_preview(core: State<'_, Core>, token: String) -> R<Vec<Value>> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    engine::optimize_preview(&core).await.map_err(e)
}

#[derive(Deserialize)]
pub struct AiAssign {
    agent_id: i64,
    provider_id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    effort: String,
}

#[tauri::command]
pub fn apply_optimization(core: State<'_, Core>, token: String, items: Vec<AiAssign>) -> R<usize> {
    let me = core.auth(&token, ROLE_MANAGER).map_err(e)?;
    let list: Vec<(i64, String, String, String)> = items.into_iter().map(|i| (i.agent_id, i.provider_id, i.model, i.effort)).collect();
    engine::apply_ai(&core, &list, &me.display_name).map_err(e)
}

#[tauri::command]
pub fn set_onboarded(core: State<'_, Core>, token: String, done: bool) -> R<()> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    db::set_setting(&core.db(), "onboarded", if done { "1" } else { "0" }).map_err(e)
}

#[tauri::command]
pub fn cancel_install(core: State<'_, Core>, token: String, id: String) -> R<bool> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    Ok(crate::setup::cancel_install(&core, &id))
}

/// Phân loại các đường dẫn được kéo thả vào (tệp hay thư mục, kích thước).
#[tauri::command]
pub fn path_info(core: State<'_, Core>, token: String, paths: Vec<String>) -> R<Vec<Value>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    Ok(paths
        .into_iter()
        .take(100)
        .filter_map(|p| {
            let m = std::fs::metadata(&p).ok()?;
            let name = std::path::Path::new(&p).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
            Some(json!({"path": p, "name": name, "is_dir": m.is_dir(), "size": m.len()}))
        })
        .collect())
}

// ───────────────────────────── Quy trình nhiều AI & danh sách model ─────────────────────────────

#[tauri::command]
pub fn list_pipelines(core: State<'_, Core>, token: String) -> R<Vec<db::Pipeline>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    db::list_pipelines(&core.db()).map_err(e)
}

#[tauri::command]
pub fn save_pipeline(core: State<'_, Core>, token: String, pipeline: db::Pipeline) -> R<i64> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    if pipeline.name.trim().is_empty() {
        return Err("Cần đặt tên quy trình".into());
    }
    if pipeline.stages.is_empty() || pipeline.stages.len() > 12 {
        return Err("Quy trình cần từ 1 đến 12 bước".into());
    }
    let mut p = pipeline;
    for (i, st) in p.stages.iter_mut().enumerate() {
        if st.title.trim().is_empty() {
            return Err(format!("Bước {} chưa có tên", i + 1));
        }
        st.step = st.step.clamp(1, 12);
        st.effort = engine::norm_effort(&st.effort);
    }
    let c = core.db();
    if p.id > 0 {
        if let Some(old) = db::get_pipeline(&c, p.id).map_err(e)? {
            p.builtin = old.builtin;
        }
    }
    let id = db::save_pipeline(&c, &p).map_err(e)?;
    db::log(&c, "quy_trinh", &me.display_name, &format!("Lưu quy trình \"{}\"", p.name), None, None).map_err(e)?;
    Ok(id)
}

#[tauri::command]
pub fn delete_pipeline(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    core.auth(&token, ROLE_MANAGER).map_err(e)?;
    db::delete_pipeline(&core.db(), id).map_err(e)
}

#[tauri::command]
pub async fn list_models(core: State<'_, Core>, token: String, provider_id: String) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let p = db::get_provider(&core.db(), &provider_id).map_err(e)?.ok_or("Không tìm thấy kết nối")?;
    let (models, live) = crate::setup::list_models(&p, &core.path_env()).await;
    Ok(json!({"models": models, "live": live, "default_model": p.default_model}))
}

// ───────────────────────────── Hỏi đáp, nhắn tiếp, việc đang làm ─────────────────────────────

#[tauri::command]
pub fn answer_question(core: State<'_, Core>, token: String, subtask_id: i64, answer: String) -> R<i64> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    if answer.trim().is_empty() {
        return Err("Hãy nhập câu trả lời".into());
    }
    engine::answer(&core, subtask_id, &answer, &me.display_name).map_err(e)
}

#[tauri::command]
pub fn list_questions(core: State<'_, Core>, token: String) -> R<Vec<Value>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    engine::questions(&core).map_err(e)
}

#[tauri::command]
pub fn followup_mission(core: State<'_, Core>, token: String, id: i64, message: String, agent_id: Option<i64>) -> R<i64> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    engine::followup(&core, id, &message, agent_id, &me.display_name).map_err(e)
}

#[tauri::command]
pub fn agent_work(core: State<'_, Core>, token: String, agent_id: i64) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    engine::agent_work(&core, agent_id).map_err(e)
}

// ───────────────────────────── Quên mật khẩu: xác thực bằng máy tính ─────────────────────────────

#[tauri::command]
pub fn os_auth_info() -> R<Value> {
    let (ok, method) = crate::osauth::available();
    Ok(json!({"available": ok, "method": method}))
}

/// Xác thực chủ máy (Touch ID / mật khẩu máy tính) cho một tài khoản đang hoạt động.
async fn os_verify(core: &Core, username: &str, reason: String) -> R<User> {
    let username = username.trim().to_string();
    let (user, _) = db::user_auth_row(&core.db(), &username).map_err(e)?.ok_or("Không tìm thấy tài khoản này")?;
    if !user.active {
        return Err("Tài khoản đã bị khoá — hãy nhờ Quản lý mở khoá".into());
    }
    tauri::async_runtime::spawn_blocking(move || crate::osauth::authenticate(&reason))
        .await
        .map_err(e)?
        .map_err(e)?;
    Ok(user)
}

#[tauri::command]
pub async fn os_reset_password(core: State<'_, Core>, username: String, new_password: String) -> R<LoginResult> {
    auth::check_password_policy(&new_password).map_err(e)?;
    let user = os_verify(&core, &username, format!("đặt lại mật khẩu tài khoản \"{}\" trong AI Office 3D", username.trim())).await?;
    db::set_password(&core.db(), user.id, &auth::hash_password(&new_password).map_err(e)?).map_err(e)?;
    let token = {
        let mut s = core.sessions();
        s.clear_fail(&user.username);
        s.remove_user(user.id);
        s.create(user.id)
    };
    core.log("nguoi_dung", &user.display_name, "Đặt lại mật khẩu bằng xác thực máy tính", None, None);
    Ok(LoginResult { token, user })
}

#[tauri::command]
pub async fn os_login(core: State<'_, Core>, username: String) -> R<LoginResult> {
    let user = os_verify(&core, &username, format!("đăng nhập tài khoản \"{}\" trong AI Office 3D", username.trim())).await?;
    let token = {
        let mut s = core.sessions();
        s.clear_fail(&user.username);
        s.create(user.id)
    };
    core.log("dang_nhap", &user.display_name, "Đăng nhập bằng xác thực máy tính", None, None);
    Ok(LoginResult { token, user })
}

// ───────────────────────────── Xuất tệp & thư mục làm việc ─────────────────────────────

#[tauri::command]
pub fn export_formats() -> R<Value> {
    Ok(json!(crate::export::FORMATS.iter().map(|(k, v)| json!({"ext": k, "label": v})).collect::<Vec<_>>()))
}

/// Xuất báo cáo nhiệm vụ (hoặc kết quả một đầu việc) ra tệp. Không chọn đường dẫn → lưu vào thư mục kết quả.
#[tauri::command]
pub fn export_content(core: State<'_, Core>, token: String, mission_id: i64, subtask_id: Option<i64>, format: String, path: Option<String>) -> R<String> {
    let me = core.auth(&token, ROLE_VIEWER).map_err(e)?;
    if !crate::export::FORMATS.iter().any(|(k, _)| *k == format) {
        return Err("Định dạng không hỗ trợ".into());
    }
    let (title, md) = {
        let c = core.db();
        let m = db::get_mission(&c, mission_id).map_err(e)?.ok_or("Không tìm thấy nhiệm vụ")?;
        match subtask_id {
            Some(sid) => {
                let s = db::get_subtask(&c, sid).map_err(e)?.ok_or("Không tìm thấy đầu việc")?;
                (format!("{} — {}", m.title, s.title.trim_start_matches(engine::FOLLOW)), s.output)
            }
            None => {
                let md = if m.summary.trim().is_empty() {
                    db::list_subtasks(&c, mission_id).map_err(e)?.iter().filter(|s| !s.output.is_empty()).map(|s| format!("## {}\n\n{}", s.title, s.output)).collect::<Vec<_>>().join("\n\n")
                } else {
                    m.summary.clone()
                };
                (m.title.clone(), md)
            }
        }
    };
    if md.trim().is_empty() {
        return Err("Chưa có nội dung để xuất".into());
    }
    let target = match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => {
            let mut pb = std::path::PathBuf::from(p);
            if crate::export::ext_of(&pb.to_string_lossy()) != format {
                pb.set_extension(&format);
            }
            pb
        }
        None => {
            let dir = crate::workspace::mission_dir(&core, mission_id).join("ket-qua");
            crate::export::unique_path(&dir, &format!("{}.{}", crate::export::safe_name(&title), format))
        }
    };
    crate::export::export(&md, &title, &format, &target).map_err(e)?;
    core.log("tep", &me.display_name, &format!("Xuất tệp {}", target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), Some(mission_id), None);
    core.emit(json!({"type": "mission", "id": mission_id}));
    Ok(target.to_string_lossy().to_string())
}

/// Tất cả tệp của nhiệm vụ: tệp AI tạo/sửa ở từng bước + tệp trong thư mục kết quả.
#[tauri::command]
pub fn mission_files(core: State<'_, Core>, token: String, id: i64) -> R<Value> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    let subs = db::list_subtasks(&core.db(), id).map_err(e)?;
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for s in subs.iter().rev() {
        for f in &s.files {
            if std::path::Path::new(&f.path).exists() && seen.insert(f.path.clone()) {
                out.push(json!({"path": f.path, "name": f.name, "size": f.size, "kind": f.kind, "by": s.agent_name, "step": s.step}));
            }
        }
    }
    let dir = crate::workspace::mission_dir(&core, id).join("ket-qua");
    let _ = std::fs::create_dir_all(&dir); // để nút "Mở thư mục kết quả" luôn mở được
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut extra: Vec<_> = rd.flatten().filter(|x| x.path().is_file()).collect();
        extra.sort_by_key(|x| std::cmp::Reverse(x.metadata().and_then(|m| m.modified()).ok()));
        for x in extra {
            let p = x.path().to_string_lossy().to_string();
            if !x.file_name().to_string_lossy().starts_with('.') && seen.insert(p.clone()) {
                out.push(json!({"path": p, "name": x.file_name().to_string_lossy(), "size": x.metadata().map(|m| m.len()).unwrap_or(0), "kind": "xuat", "by": "", "step": null}));
            }
        }
    }
    Ok(json!({"files": out, "folder": dir.to_string_lossy()}))
}

/// Tạo thư mục làm việc mới (mặc định trong ~/Documents/AI Office).
#[tauri::command]
pub fn create_workdir(core: State<'_, Core>, token: String, name: String, parent: Option<String>) -> R<String> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    let name = crate::export::safe_name(&name);
    let parent = match parent.filter(|p| !p.trim().is_empty()) {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join("Documents").join("AI Office"),
    };
    let dir = crate::export::unique_path(&parent, &name);
    std::fs::create_dir_all(&dir).map_err(|x| format!("Không tạo được thư mục: {x}"))?;
    Ok(dir.to_string_lossy().to_string())
}

// ───────────────────────────── Trí nhớ & mẫu tài liệu ─────────────────────────────

#[tauri::command]
pub fn list_memories(core: State<'_, Core>, token: String, agent_id: Option<i64>) -> R<Vec<crate::memory::Memory>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    crate::memory::list(&core.db(), agent_id).map_err(e)
}

#[tauri::command]
pub fn save_memory(core: State<'_, Core>, token: String, memory: crate::memory::Memory) -> R<()> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    let c = core.db();
    if memory.id > 0 {
        crate::memory::update(&c, memory.id, &memory.content, memory.active, memory.agent_id).map_err(e)?;
    } else {
        crate::memory::add(&c, memory.agent_id, &memory.content, "quan_ly", None).map_err(e)?.ok_or("Ghi nhớ này đã có hoặc quá ngắn")?;
        db::log(&c, "ghi_nho", &me.display_name, &format!("Thêm ghi nhớ: {}", memory.content.trim()), None, memory.agent_id).map_err(e)?;
    }
    Ok(())
}

#[tauri::command]
pub fn delete_memory(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    crate::memory::delete(&core.db(), id).map_err(e)
}

#[tauri::command]
pub fn list_doc_templates(core: State<'_, Core>, token: String) -> R<Vec<crate::memory::DocTemplate>> {
    core.auth(&token, ROLE_VIEWER).map_err(e)?;
    crate::memory::list_templates(&core.db()).map_err(e)
}

#[tauri::command]
pub fn save_doc_template(core: State<'_, Core>, token: String, template: crate::memory::DocTemplate) -> R<i64> {
    let me = core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    if !template.format.is_empty() && !crate::export::FORMATS.iter().any(|(k, _)| *k == template.format) {
        return Err("Định dạng xuất không hợp lệ".into());
    }
    let c = core.db();
    let id = crate::memory::save_template(&c, &template).map_err(e)?;
    db::log(&c, "mau", &me.display_name, &format!("Lưu mẫu tài liệu “{}”", template.name.trim()), None, None).map_err(e)?;
    Ok(id)
}

#[tauri::command]
pub fn delete_doc_template(core: State<'_, Core>, token: String, id: i64) -> R<()> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    crate::memory::delete_template(&core.db(), id).map_err(e)
}

#[tauri::command]
pub fn import_template_file(core: State<'_, Core>, token: String, path: String) -> R<Value> {
    core.auth(&token, ROLE_COORDINATOR).map_err(e)?;
    let (name, content) = crate::memory::import_file(std::path::Path::new(&path)).map_err(e)?;
    Ok(json!({"name": name, "content": content}))
}
