//! Lớp lưu trữ SQLite: lược đồ, kiểu dữ liệu và các truy vấn.

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

pub fn open_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            display_name TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            command TEXT NOT NULL DEFAULT '',
            args TEXT NOT NULL DEFAULT '',
            model_args TEXT NOT NULL DEFAULT '',
            use_stdin INTEGER NOT NULL DEFAULT 0,
            base_url TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL DEFAULT '',
            default_model TEXT NOT NULL DEFAULT '',
            timeout_secs INTEGER NOT NULL DEFAULT 900,
            enabled INTEGER NOT NULL DEFAULT 1,
            builtin INTEGER NOT NULL DEFAULT 0,
            notes TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS agents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            title TEXT NOT NULL,
            department TEXT NOT NULL DEFAULT '',
            level TEXT NOT NULL DEFAULT 'trung_cap',
            skills TEXT NOT NULL DEFAULT '[]',
            system_prompt TEXT NOT NULL DEFAULT '',
            provider_id TEXT NOT NULL DEFAULT 'mock',
            model TEXT NOT NULL DEFAULT '',
            avatar TEXT NOT NULL DEFAULT 'character-male-a',
            color TEXT NOT NULL DEFAULT '#6366f1',
            is_dispatcher INTEGER NOT NULL DEFAULT 0,
            desk INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'idle',
            tasks_done INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS missions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            priority TEXT NOT NULL DEFAULT 'trung_binh',
            mode TEXT NOT NULL DEFAULT 'auto',
            status TEXT NOT NULL DEFAULT 'moi',
            created_by INTEGER,
            coordinator_id INTEGER,
            deadline TEXT NOT NULL DEFAULT '',
            analysis TEXT NOT NULL DEFAULT '',
            summary TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS subtasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            mission_id INTEGER NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
            agent_id INTEGER REFERENCES agents(id) ON DELETE SET NULL,
            agent_name TEXT NOT NULL DEFAULT '',
            step INTEGER NOT NULL DEFAULT 1,
            title TEXT NOT NULL,
            instruction TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'cho',
            output TEXT NOT NULL DEFAULT '',
            error TEXT NOT NULL DEFAULT '',
            started_at TEXT NOT NULL DEFAULT '',
            finished_at TEXT NOT NULL DEFAULT '',
            duration_ms INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS activity (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL,
            kind TEXT NOT NULL,
            actor TEXT NOT NULL DEFAULT '',
            message TEXT NOT NULL,
            mission_id INTEGER,
            agent_id INTEGER
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_subtasks_mission ON subtasks(mission_id);
        CREATE INDEX IF NOT EXISTS idx_activity_ts ON activity(id DESC);
        "#,
    )?;
    migrate(conn)?;
    Ok(())
}

/// Nâng cấp CSDL cũ: thêm cột mới nếu chưa có.
fn migrate(conn: &Connection) -> Result<()> {
    let add = |table: &str, col: &str, def: &str| -> Result<()> {
        let mut st = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let cols: Vec<String> = st.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<_>>()?;
        if !cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {def}"), [])?;
        }
        Ok(())
    };
    add("missions", "workdir", "TEXT NOT NULL DEFAULT ''")?;
    add("missions", "attachments", "TEXT NOT NULL DEFAULT '[]'")?;
    add("missions", "allow_edit", "INTEGER NOT NULL DEFAULT 0")?;
    add("providers", "edit_args", "TEXT NOT NULL DEFAULT ''")?;
    add("providers", "dir_args", "TEXT NOT NULL DEFAULT ''")?;
    add("providers", "effort_args", "TEXT NOT NULL DEFAULT ''")?;
    add("providers", "web_args", "TEXT NOT NULL DEFAULT ''")?;
    add("agents", "effort", "TEXT NOT NULL DEFAULT ''")?;
    add("subtasks", "provider_id", "TEXT NOT NULL DEFAULT ''")?;
    add("subtasks", "model", "TEXT NOT NULL DEFAULT ''")?;
    add("subtasks", "effort", "TEXT NOT NULL DEFAULT ''")?;
    add("subtasks", "web", "INTEGER NOT NULL DEFAULT 0")?;
    add("subtasks", "question", "TEXT NOT NULL DEFAULT ''")?;
    add("subtasks", "log", "TEXT NOT NULL DEFAULT ''")?;
    add("subtasks", "files", "TEXT NOT NULL DEFAULT '[]'")?;
    add("missions", "template_id", "INTEGER")?;
    crate::memory::init(conn)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pipelines (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            stages TEXT NOT NULL DEFAULT '[]',
            builtin INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

// ───────────────────────────── Kiểu dữ liệu ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub command: String,
    pub args: String,
    pub model_args: String,
    /// Tham số cho phép AI sửa/tạo tệp (khi nhiệm vụ bật "cho phép sửa file").
    pub edit_args: String,
    /// Tham số cấp quyền đọc thêm một thư mục, dùng {dir}.
    pub dir_args: String,
    /// Tham số mức suy luận, dùng {effort}.
    pub effort_args: String,
    /// Tham số cho phép tra cứu web.
    pub web_args: String,
    pub use_stdin: bool,
    pub base_url: String,
    #[serde(default, skip_serializing)]
    pub api_key: String,
    pub default_model: String,
    pub timeout_secs: i64,
    pub enabled: bool,
    pub builtin: bool,
    pub notes: String,
}

/// Bản gửi ra giao diện: không bao giờ lộ API key.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub p: Provider,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: i64,
    pub name: String,
    pub title: String,
    pub department: String,
    pub level: String,
    pub skills: Vec<String>,
    pub system_prompt: String,
    pub provider_id: String,
    pub model: String,
    /// Mức suy luận: '' (mặc định) | low | medium | high | xhigh | max
    #[serde(default)]
    pub effort: String,
    pub avatar: String,
    pub color: String,
    pub is_dispatcher: bool,
    pub desk: i64,
    pub status: String,
    pub tasks_done: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub mode: String,
    pub status: String,
    pub created_by: Option<i64>,
    pub coordinator_id: Option<i64>,
    pub deadline: String,
    pub analysis: String,
    pub summary: String,
    pub created_at: String,
    pub updated_at: String,
    /// Thư mục làm việc (AI chạy ngay trong thư mục này, như một project).
    pub workdir: String,
    /// Tệp đính kèm (đường dẫn tuyệt đối).
    pub attachments: Vec<String>,
    /// Cho phép AI sửa/tạo tệp trong thư mục làm việc.
    pub allow_edit: bool,
    /// Mẫu kết quả áp dụng (thư viện mẫu tài liệu)
    pub template_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: i64,
    pub mission_id: i64,
    pub agent_id: Option<i64>,
    pub agent_name: String,
    pub step: i64,
    pub title: String,
    pub instruction: String,
    pub status: String,
    pub output: String,
    pub error: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
    /// AI riêng cho bước này (rỗng = dùng AI của nhân viên)
    pub provider_id: String,
    pub model: String,
    pub effort: String,
    /// Cho phép tra cứu web ở bước này
    pub web: bool,
    /// Câu hỏi nhân viên đang chờ quản lý trả lời
    pub question: String,
    /// Nhật ký tiến trình lần chạy gần nhất
    pub log: String,
    /// Tệp đã tạo/sửa/xuất ở lần chạy gần nhất
    pub files: Vec<crate::export::FileRec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Activity {
    pub id: i64,
    pub ts: String,
    pub kind: String,
    pub actor: String,
    pub message: String,
    pub mission_id: Option<i64>,
    pub agent_id: Option<i64>,
}

// ───────────────────────────── Ánh xạ hàng ─────────────────────────────

fn user_from(r: &Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get("id")?,
        username: r.get("username")?,
        display_name: r.get("display_name")?,
        role: r.get("role")?,
        active: r.get::<_, i64>("active")? != 0,
        created_at: r.get("created_at")?,
    })
}

fn provider_from(r: &Row) -> rusqlite::Result<Provider> {
    Ok(Provider {
        id: r.get("id")?,
        name: r.get("name")?,
        kind: r.get("kind")?,
        command: r.get("command")?,
        args: r.get("args")?,
        model_args: r.get("model_args")?,
        edit_args: r.get("edit_args")?,
        dir_args: r.get("dir_args")?,
        effort_args: r.get("effort_args")?,
        web_args: r.get("web_args")?,
        use_stdin: r.get::<_, i64>("use_stdin")? != 0,
        base_url: r.get("base_url")?,
        api_key: r.get("api_key")?,
        default_model: r.get("default_model")?,
        timeout_secs: r.get("timeout_secs")?,
        enabled: r.get::<_, i64>("enabled")? != 0,
        builtin: r.get::<_, i64>("builtin")? != 0,
        notes: r.get("notes")?,
    })
}

fn agent_from(r: &Row) -> rusqlite::Result<Agent> {
    let skills: String = r.get("skills")?;
    Ok(Agent {
        id: r.get("id")?,
        name: r.get("name")?,
        title: r.get("title")?,
        department: r.get("department")?,
        level: r.get("level")?,
        skills: serde_json::from_str(&skills).unwrap_or_default(),
        system_prompt: r.get("system_prompt")?,
        provider_id: r.get("provider_id")?,
        model: r.get("model")?,
        effort: r.get("effort")?,
        avatar: r.get("avatar")?,
        color: r.get("color")?,
        is_dispatcher: r.get::<_, i64>("is_dispatcher")? != 0,
        desk: r.get("desk")?,
        status: r.get("status")?,
        tasks_done: r.get("tasks_done")?,
        created_at: r.get("created_at")?,
    })
}

fn mission_from(r: &Row) -> rusqlite::Result<Mission> {
    Ok(Mission {
        id: r.get("id")?,
        title: r.get("title")?,
        description: r.get("description")?,
        priority: r.get("priority")?,
        mode: r.get("mode")?,
        status: r.get("status")?,
        created_by: r.get("created_by")?,
        coordinator_id: r.get("coordinator_id")?,
        deadline: r.get("deadline")?,
        analysis: r.get("analysis")?,
        summary: r.get("summary")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        workdir: r.get("workdir")?,
        attachments: serde_json::from_str(&r.get::<_, String>("attachments")?).unwrap_or_default(),
        allow_edit: r.get::<_, i64>("allow_edit")? != 0,
        template_id: r.get("template_id")?,
    })
}

pub fn set_mission_template(c: &Connection, id: i64, template_id: Option<i64>) -> Result<()> {
    c.execute("UPDATE missions SET template_id=?2 WHERE id=?1", params![id, template_id])?;
    Ok(())
}

pub fn set_mission_files(c: &Connection, id: i64, workdir: &str, attachments: &[String], allow_edit: bool) -> Result<()> {
    c.execute(
        "UPDATE missions SET workdir=?2, attachments=?3, allow_edit=?4, updated_at=?5 WHERE id=?1",
        params![id, workdir, serde_json::to_string(attachments)?, allow_edit as i64, now()],
    )?;
    Ok(())
}

fn subtask_from(r: &Row) -> rusqlite::Result<Subtask> {
    Ok(Subtask {
        id: r.get("id")?,
        mission_id: r.get("mission_id")?,
        agent_id: r.get("agent_id")?,
        agent_name: r.get("agent_name")?,
        step: r.get("step")?,
        title: r.get("title")?,
        instruction: r.get("instruction")?,
        status: r.get("status")?,
        output: r.get("output")?,
        error: r.get("error")?,
        started_at: r.get("started_at")?,
        finished_at: r.get("finished_at")?,
        duration_ms: r.get("duration_ms")?,
        provider_id: r.get("provider_id")?,
        model: r.get("model")?,
        effort: r.get("effort")?,
        web: r.get::<_, i64>("web")? != 0,
        question: r.get("question")?,
        log: r.get("log")?,
        files: serde_json::from_str(&r.get::<_, String>("files")?).unwrap_or_default(),
    })
}

pub fn set_subtask_files(c: &Connection, id: i64, files: &[crate::export::FileRec]) -> Result<()> {
    c.execute("UPDATE subtasks SET files=?2 WHERE id=?1", params![id, serde_json::to_string(files)?])?;
    Ok(())
}

pub fn set_subtask_question(c: &Connection, id: i64, q: &str) -> Result<()> {
    c.execute("UPDATE subtasks SET question=?2, status=CASE WHEN ?2='' THEN 'cho' ELSE 'hoi' END WHERE id=?1", params![id, q])?;
    Ok(())
}

pub fn set_subtask_log(c: &Connection, id: i64, log: &str) -> Result<()> {
    c.execute("UPDATE subtasks SET log=?2 WHERE id=?1", params![id, log])?;
    Ok(())
}

pub fn append_instruction(c: &Connection, id: i64, extra: &str) -> Result<()> {
    c.execute("UPDATE subtasks SET instruction=instruction || ?2 WHERE id=?1", params![id, extra])?;
    Ok(())
}

pub fn set_subtask_ai(c: &Connection, id: i64, provider_id: &str, model: &str, effort: &str, web: bool) -> Result<()> {
    c.execute(
        "UPDATE subtasks SET provider_id=?2, model=?3, effort=?4, web=?5 WHERE id=?1",
        params![id, provider_id, model, effort, web as i64],
    )?;
    Ok(())
}

// ───────────────────────────── Quy trình nhiều AI ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Stage {
    pub step: i64,
    pub title: String,
    pub instruction: String,
    /// Nhân viên cố định (nếu có); không thì chọn theo từ khoá `role`
    pub agent_id: Option<i64>,
    pub role: String,
    pub provider_id: String,
    pub model: String,
    pub effort: String,
    pub web: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Pipeline {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub stages: Vec<Stage>,
    pub builtin: bool,
}

fn pipeline_from(r: &Row) -> rusqlite::Result<Pipeline> {
    Ok(Pipeline {
        id: r.get("id")?,
        name: r.get("name")?,
        description: r.get("description")?,
        stages: serde_json::from_str(&r.get::<_, String>("stages")?).unwrap_or_default(),
        builtin: r.get::<_, i64>("builtin")? != 0,
    })
}

pub fn list_pipelines(c: &Connection) -> Result<Vec<Pipeline>> {
    let mut s = c.prepare("SELECT * FROM pipelines ORDER BY builtin DESC, id")?;
    let v = s.query_map([], pipeline_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn get_pipeline(c: &Connection, id: i64) -> Result<Option<Pipeline>> {
    Ok(c.query_row("SELECT * FROM pipelines WHERE id=?1", [id], pipeline_from).optional()?)
}

pub fn save_pipeline(c: &Connection, p: &Pipeline) -> Result<i64> {
    let stages = serde_json::to_string(&p.stages)?;
    if p.id > 0 {
        c.execute("UPDATE pipelines SET name=?2, description=?3, stages=?4 WHERE id=?1", params![p.id, p.name, p.description, stages])?;
        Ok(p.id)
    } else {
        c.execute(
            "INSERT INTO pipelines(name,description,stages,builtin,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![p.name, p.description, stages, p.builtin as i64, now()],
        )?;
        Ok(c.last_insert_rowid())
    }
}

pub fn delete_pipeline(c: &Connection, id: i64) -> Result<()> {
    c.execute("DELETE FROM pipelines WHERE id=?1", [id])?;
    Ok(())
}

// ───────────────────────────── Cài đặt ─────────────────────────────

pub fn get_setting(c: &Connection, key: &str) -> Result<Option<String>> {
    Ok(c
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn set_setting(c: &Connection, key: &str, value: &str) -> Result<()> {
    c.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ───────────────────────────── Người dùng ─────────────────────────────

pub fn count_users(c: &Connection) -> Result<i64> {
    Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)
}

pub fn insert_user(c: &Connection, username: &str, display: &str, hash: &str, role: &str) -> Result<i64> {
    c.execute(
        "INSERT INTO users(username,display_name,password_hash,role,active,created_at) VALUES(?1,?2,?3,?4,1,?5)",
        params![username, display, hash, role, now()],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            anyhow!("Tên đăng nhập đã tồn tại")
        }
        e => e.into(),
    })?;
    Ok(c.last_insert_rowid())
}

pub fn user_auth_row(c: &Connection, username: &str) -> Result<Option<(User, String)>> {
    Ok(c
        .query_row("SELECT * FROM users WHERE username=?1", [username], |r| {
            Ok((user_from(r)?, r.get::<_, String>("password_hash")?))
        })
        .optional()?)
}

pub fn get_user(c: &Connection, id: i64) -> Result<Option<User>> {
    Ok(c.query_row("SELECT * FROM users WHERE id=?1", [id], user_from).optional()?)
}

pub fn password_hash_of(c: &Connection, id: i64) -> Result<String> {
    Ok(c.query_row("SELECT password_hash FROM users WHERE id=?1", [id], |r| r.get(0))?)
}

pub fn list_users(c: &Connection) -> Result<Vec<User>> {
    let mut s = c.prepare("SELECT * FROM users ORDER BY id")?;
    let v = s.query_map([], user_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn update_user(c: &Connection, id: i64, display: &str, role: &str, active: bool) -> Result<()> {
    c.execute(
        "UPDATE users SET display_name=?2, role=?3, active=?4 WHERE id=?1",
        params![id, display, role, active as i64],
    )?;
    Ok(())
}

pub fn set_password(c: &Connection, id: i64, hash: &str) -> Result<()> {
    c.execute("UPDATE users SET password_hash=?2 WHERE id=?1", params![id, hash])?;
    Ok(())
}

pub fn delete_user(c: &Connection, id: i64) -> Result<()> {
    c.execute("UPDATE missions SET coordinator_id=NULL WHERE coordinator_id=?1", [id])?;
    c.execute("DELETE FROM users WHERE id=?1", [id])?;
    Ok(())
}

pub fn count_active_managers(c: &Connection) -> Result<i64> {
    Ok(c.query_row(
        "SELECT COUNT(*) FROM users WHERE role='manager' AND active=1",
        [],
        |r| r.get(0),
    )?)
}

// ───────────────────────────── Nhà cung cấp AI ─────────────────────────────

pub fn list_providers(c: &Connection) -> Result<Vec<Provider>> {
    let mut s = c.prepare("SELECT * FROM providers ORDER BY builtin DESC, name")?;
    let v = s.query_map([], provider_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn get_provider(c: &Connection, id: &str) -> Result<Option<Provider>> {
    Ok(c.query_row("SELECT * FROM providers WHERE id=?1", [id], provider_from).optional()?)
}

/// Lưu nhà cung cấp. Nếu `keep_key` thì giữ nguyên API key cũ khi key mới rỗng.
pub fn upsert_provider(c: &Connection, p: &Provider, keep_key: bool) -> Result<()> {
    let key = if keep_key && p.api_key.is_empty() {
        get_provider(c, &p.id)?.map(|o| o.api_key).unwrap_or_default()
    } else {
        p.api_key.clone()
    };
    c.execute(
        r#"INSERT INTO providers(id,name,kind,command,args,model_args,use_stdin,base_url,api_key,default_model,timeout_secs,enabled,builtin,notes,edit_args,dir_args,effort_args,web_args)
           VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
           ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, command=excluded.command, args=excluded.args,
             model_args=excluded.model_args, use_stdin=excluded.use_stdin, base_url=excluded.base_url, api_key=excluded.api_key,
             default_model=excluded.default_model, timeout_secs=excluded.timeout_secs, enabled=excluded.enabled, notes=excluded.notes,
             edit_args=excluded.edit_args, dir_args=excluded.dir_args, effort_args=excluded.effort_args, web_args=excluded.web_args"#,
        params![
            p.id, p.name, p.kind, p.command, p.args, p.model_args, p.use_stdin as i64, p.base_url, key,
            p.default_model, p.timeout_secs, p.enabled as i64, p.builtin as i64, p.notes, p.edit_args, p.dir_args, p.effort_args, p.web_args
        ],
    )?;
    Ok(())
}

pub fn delete_provider(c: &Connection, id: &str) -> Result<()> {
    c.execute("UPDATE agents SET provider_id='mock' WHERE provider_id=?1", [id])?;
    c.execute("DELETE FROM providers WHERE id=?1 AND builtin=0", [id])?;
    Ok(())
}

// ───────────────────────────── Nhân viên AI ─────────────────────────────

pub fn list_agents(c: &Connection) -> Result<Vec<Agent>> {
    let mut s = c.prepare("SELECT * FROM agents ORDER BY is_dispatcher DESC, desk, id")?;
    let v = s.query_map([], agent_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn get_agent(c: &Connection, id: i64) -> Result<Option<Agent>> {
    Ok(c.query_row("SELECT * FROM agents WHERE id=?1", [id], agent_from).optional()?)
}

pub fn dispatcher(c: &Connection) -> Result<Option<Agent>> {
    Ok(c
        .query_row("SELECT * FROM agents WHERE is_dispatcher=1 ORDER BY id LIMIT 1", [], agent_from)
        .optional()?)
}

/// Bàn trống nhỏ nhất cho nhân viên mới.
pub fn free_desk(c: &Connection) -> Result<i64> {
    let mut s = c.prepare("SELECT desk FROM agents WHERE is_dispatcher=0 ORDER BY desk")?;
    let used: Vec<i64> = s.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let mut d = 0;
    while used.contains(&d) {
        d += 1;
    }
    Ok(d)
}

pub fn insert_agent(c: &Connection, a: &Agent) -> Result<i64> {
    let desk = if a.is_dispatcher { -1 } else { free_desk(c)? };
    c.execute(
        r#"INSERT INTO agents(name,title,department,level,skills,system_prompt,provider_id,model,avatar,color,is_dispatcher,desk,status,tasks_done,created_at,effort)
           VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'idle',0,?13,?14)"#,
        params![
            a.name, a.title, a.department, a.level, serde_json::to_string(&a.skills)?, a.system_prompt,
            a.provider_id, a.model, a.avatar, a.color, a.is_dispatcher as i64, desk, now(), a.effort
        ],
    )?;
    Ok(c.last_insert_rowid())
}

pub fn update_agent(c: &Connection, a: &Agent) -> Result<()> {
    c.execute(
        r#"UPDATE agents SET name=?2,title=?3,department=?4,level=?5,skills=?6,system_prompt=?7,provider_id=?8,model=?9,avatar=?10,color=?11,effort=?12
           WHERE id=?1"#,
        params![
            a.id, a.name, a.title, a.department, a.level, serde_json::to_string(&a.skills)?, a.system_prompt,
            a.provider_id, a.model, a.avatar, a.color, a.effort
        ],
    )?;
    Ok(())
}

pub fn delete_agent(c: &Connection, id: i64) -> Result<()> {
    c.execute("DELETE FROM agents WHERE id=?1 AND is_dispatcher=0", [id])?;
    Ok(())
}

pub fn set_agent_status(c: &Connection, id: i64, status: &str) -> Result<()> {
    c.execute("UPDATE agents SET status=?2 WHERE id=?1", params![id, status])?;
    Ok(())
}

pub fn inc_agent_done(c: &Connection, id: i64) -> Result<()> {
    c.execute("UPDATE agents SET tasks_done=tasks_done+1 WHERE id=?1", [id])?;
    Ok(())
}

pub fn reset_all_agent_status(c: &Connection) -> Result<()> {
    c.execute("UPDATE agents SET status='idle'", [])?;
    Ok(())
}

// ───────────────────────────── Nhiệm vụ ─────────────────────────────

pub fn insert_mission(
    c: &Connection,
    title: &str,
    description: &str,
    priority: &str,
    mode: &str,
    created_by: i64,
    coordinator_id: Option<i64>,
    deadline: &str,
) -> Result<i64> {
    let t = now();
    c.execute(
        r#"INSERT INTO missions(title,description,priority,mode,status,created_by,coordinator_id,deadline,created_at,updated_at)
           VALUES(?1,?2,?3,?4,'moi',?5,?6,?7,?8,?8)"#,
        params![title, description, priority, mode, created_by, coordinator_id, deadline, t],
    )?;
    Ok(c.last_insert_rowid())
}

pub fn list_missions(c: &Connection) -> Result<Vec<Mission>> {
    let mut s = c.prepare("SELECT * FROM missions ORDER BY id DESC")?;
    let v = s.query_map([], mission_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn get_mission(c: &Connection, id: i64) -> Result<Option<Mission>> {
    Ok(c.query_row("SELECT * FROM missions WHERE id=?1", [id], mission_from).optional()?)
}

pub fn set_mission_status(c: &Connection, id: i64, status: &str) -> Result<()> {
    c.execute(
        "UPDATE missions SET status=?2, updated_at=?3 WHERE id=?1",
        params![id, status, now()],
    )?;
    Ok(())
}

pub fn set_mission_text(c: &Connection, id: i64, field: &str, value: &str) -> Result<()> {
    let sql = match field {
        "analysis" => "UPDATE missions SET analysis=?2, updated_at=?3 WHERE id=?1",
        "summary" => "UPDATE missions SET summary=?2, updated_at=?3 WHERE id=?1",
        _ => return Err(anyhow!("trường không hợp lệ")),
    };
    c.execute(sql, params![id, value, now()])?;
    Ok(())
}

pub fn update_mission_meta(
    c: &Connection,
    id: i64,
    title: &str,
    description: &str,
    priority: &str,
    coordinator_id: Option<i64>,
    deadline: &str,
) -> Result<()> {
    c.execute(
        "UPDATE missions SET title=?2, description=?3, priority=?4, coordinator_id=?5, deadline=?6, updated_at=?7 WHERE id=?1",
        params![id, title, description, priority, coordinator_id, deadline, now()],
    )?;
    Ok(())
}

pub fn delete_mission(c: &Connection, id: i64) -> Result<()> {
    c.execute("DELETE FROM subtasks WHERE mission_id=?1", [id])?;
    c.execute("DELETE FROM missions WHERE id=?1", [id])?;
    Ok(())
}

pub fn list_subtasks(c: &Connection, mission_id: i64) -> Result<Vec<Subtask>> {
    let mut s = c.prepare("SELECT * FROM subtasks WHERE mission_id=?1 ORDER BY step, id")?;
    let v = s.query_map([mission_id], subtask_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn get_subtask(c: &Connection, id: i64) -> Result<Option<Subtask>> {
    Ok(c.query_row("SELECT * FROM subtasks WHERE id=?1", [id], subtask_from).optional()?)
}

pub fn insert_subtask(
    c: &Connection,
    mission_id: i64,
    agent_id: Option<i64>,
    step: i64,
    title: &str,
    instruction: &str,
) -> Result<i64> {
    let name = match agent_id {
        Some(a) => get_agent(c, a)?.map(|x| x.name).unwrap_or_default(),
        None => String::new(),
    };
    c.execute(
        "INSERT INTO subtasks(mission_id,agent_id,agent_name,step,title,instruction,status) VALUES(?1,?2,?3,?4,?5,?6,'cho')",
        params![mission_id, agent_id, name, step, title, instruction],
    )?;
    Ok(c.last_insert_rowid())
}

pub fn update_subtask_plan(
    c: &Connection,
    id: i64,
    agent_id: Option<i64>,
    step: i64,
    title: &str,
    instruction: &str,
) -> Result<()> {
    let name = match agent_id {
        Some(a) => get_agent(c, a)?.map(|x| x.name).unwrap_or_default(),
        None => String::new(),
    };
    c.execute(
        "UPDATE subtasks SET agent_id=?2, agent_name=?3, step=?4, title=?5, instruction=?6, status='cho', output='', error='' WHERE id=?1",
        params![id, agent_id, name, step, title, instruction],
    )?;
    Ok(())
}

pub fn delete_subtask(c: &Connection, id: i64) -> Result<()> {
    c.execute("DELETE FROM subtasks WHERE id=?1", [id])?;
    Ok(())
}

pub fn subtask_start(c: &Connection, id: i64) -> Result<()> {
    c.execute(
        "UPDATE subtasks SET status='dang_lam', started_at=?2, error='' WHERE id=?1",
        params![id, now()],
    )?;
    Ok(())
}

pub fn subtask_finish(c: &Connection, id: i64, status: &str, output: &str, error: &str, ms: i64) -> Result<()> {
    c.execute(
        "UPDATE subtasks SET status=?2, output=?3, error=?4, finished_at=?5, duration_ms=?6 WHERE id=?1",
        params![id, status, output, error, now(), ms],
    )?;
    Ok(())
}

pub fn reset_running_on_start(c: &Connection) -> Result<()> {
    // Sau khi ứng dụng bị tắt giữa chừng: đánh dấu việc đang làm là lỗi để có thể chạy lại.
    c.execute(
        "UPDATE subtasks SET status='loi', error='Ứng dụng đã đóng khi đang thực hiện' WHERE status='dang_lam'",
        [],
    )?;
    c.execute(
        "UPDATE missions SET status=CASE WHEN EXISTS(SELECT 1 FROM subtasks s WHERE s.mission_id=missions.id AND s.status='hoi') THEN 'cho_tra_loi' ELSE 'loi' END WHERE status IN ('dang_lap_ke_hoach','dang_thuc_hien','dang_tong_hop')",
        [],
    )?;
    Ok(())
}

// ───────────────────────────── Nhật ký ─────────────────────────────

pub fn log(
    c: &Connection,
    kind: &str,
    actor: &str,
    message: &str,
    mission_id: Option<i64>,
    agent_id: Option<i64>,
) -> Result<Activity> {
    let ts = now();
    c.execute(
        "INSERT INTO activity(ts,kind,actor,message,mission_id,agent_id) VALUES(?1,?2,?3,?4,?5,?6)",
        params![ts, kind, actor, message, mission_id, agent_id],
    )?;
    Ok(Activity {
        id: c.last_insert_rowid(),
        ts,
        kind: kind.into(),
        actor: actor.into(),
        message: message.into(),
        mission_id,
        agent_id,
    })
}

pub fn list_activity(c: &Connection, limit: i64) -> Result<Vec<Activity>> {
    let mut s = c.prepare("SELECT * FROM activity ORDER BY id DESC LIMIT ?1")?;
    let v = s
        .query_map([limit], |r| {
            Ok(Activity {
                id: r.get("id")?,
                ts: r.get("ts")?,
                kind: r.get("kind")?,
                actor: r.get("actor")?,
                message: r.get("message")?,
                mission_id: r.get("mission_id")?,
                agent_id: r.get("agent_id")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}
