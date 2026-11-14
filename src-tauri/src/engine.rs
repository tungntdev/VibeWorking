//! Bộ máy văn phòng: khởi tạo dữ liệu, tuyển dụng, lập kế hoạch, phân công và thực thi nhiệm vụ.

use crate::state::Core;
use crate::db::{self, Agent, Provider, Subtask};
use crate::providers::{self, Call, Cancel};
use crate::templates::{self, Candidate};
use crate::{optimizer, setup, workspace};
use std::collections::HashMap;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Instant;
use tokio::sync::watch;

const CLI_PREFERENCE: [&str; 3] = ["claude-cli", "codex-cli", "gemini-cli"];

// ───────────────────────────── Khởi tạo ─────────────────────────────

/// Tạo dữ liệu mặc định khi chạy lần đầu (kết nối AI, điều phối viên, thư ký).
pub fn seed(core: &Core) -> Result<()> {
    let path_env = core.path_env();
    let c = core.db();
    let existing: Vec<String> = db::list_providers(&c)?.into_iter().map(|p| p.id).collect();
    for p in providers::builtin_providers() {
        if !existing.contains(&p.id) {
            db::upsert_provider(&c, &p, false)?;
        } else if let Some(mut old) = db::get_provider(&c, &p.id)? {
            // bổ sung tham số mới cho kết nối dựng sẵn đã có từ phiên bản trước
            let mut changed = false;
            for (cur, def) in [
                (&mut old.edit_args, &p.edit_args),
                (&mut old.dir_args, &p.dir_args),
                (&mut old.effort_args, &p.effort_args),
                (&mut old.web_args, &p.web_args),
            ] {
                if cur.is_empty() && !def.is_empty() {
                    *cur = def.clone();
                    changed = true;
                }
            }
            if providers::LEGACY_ARGS.iter().any(|(id, a)| *id == old.id && old.args == *a) {
                old.args = p.args.clone();
                changed = true;
            }
            if changed {
                db::upsert_provider(&c, &old, true)?;
            }
        }
    }
    if db::get_setting(&c, "office_name")?.is_none() {
        db::set_setting(&c, "office_name", "Văn phòng AI")?;
        db::set_setting(&c, "office_description", "")?;
    }
    if db::list_pipelines(&c)?.is_empty() {
        for pl in builtin_pipelines() {
            db::save_pipeline(&c, &pl)?;
        }
    }
    if db::dispatcher(&c)?.is_none() {
        let provider = detect_default_provider(&c, &path_env)?;
        let t = templates::dispatcher_template();
        let a = agent_from_template(&t, "Điều phối viên Minh An", &provider, true);
        db::insert_agent(&c, &a)?;
        let s = templates::library().into_iter().find(|t| t.key == "thu_ky").unwrap();
        let a = agent_from_template(&s, "Trần Thu Hà", &provider, false);
        db::insert_agent(&c, &a)?;
        db::log(&c, "he_thong", "Hệ thống", &format!("Khởi tạo văn phòng, kết nối AI mặc định: {provider}"), None, None)?;
    }
    db::reset_all_agent_status(&c)?;
    db::reset_running_on_start(&c)?;
    Ok(())
}

/// Chọn CLI đầu tiên có trên máy (Claude → Codex → Gemini), nếu không có thì dùng mô phỏng.
pub fn detect_default_provider(c: &rusqlite::Connection, path_env: &str) -> Result<String> {
    for id in CLI_PREFERENCE {
        if let Some(p) = db::get_provider(c, id)? {
            if p.enabled && providers::which(&p.command, path_env).is_some() {
                return Ok(id.into());
            }
        }
    }
    Ok("mock".into())
}

pub fn agent_from_template(t: &templates::Template, name: &str, provider: &str, dispatcher: bool) -> Agent {
    Agent {
        id: 0,
        name: name.into(),
        title: t.title.clone(),
        department: t.department.clone(),
        level: t.level.clone(),
        skills: t.skills.clone(),
        system_prompt: t.system_prompt.clone(),
        provider_id: provider.into(),
        model: String::new(),
        avatar: t.avatar.clone(),
        color: t.color.clone(),
        is_dispatcher: dispatcher,
        desk: 0,
        status: "idle".into(),
        tasks_done: 0,
        created_at: String::new(),
        effort: String::new(),
    }
}

// ───────────────────────────── Tiện ích ─────────────────────────────

/// Lấy JSON từ câu trả lời AI (có thể nằm trong ```json ... ``` hoặc lẫn văn bản).
pub fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    if let Some(start) = t.find("```") {
        let rest = &t[start + 3..];
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        if let Some(end) = rest.find("```") {
            if let Ok(v) = serde_json::from_str::<Value>(rest[..end].trim()) {
                return Some(v);
            }
        }
    }
    for (o, cl) in [('{', '}'), ('[', ']')] {
        if let (Some(a), Some(b)) = (t.find(o), t.rfind(cl)) {
            if b > a {
                if let Ok(v) = serde_json::from_str::<Value>(&t[a..=b]) {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…(đã rút gọn)", s.chars().take(n).collect::<String>())
    }
}

fn level_label(l: &str) -> &'static str {
    templates::LEVELS.iter().find(|(k, _)| *k == l).map(|(_, v)| *v).unwrap_or("Trung cấp")
}

fn provider_of(core: &Core, agent: &Agent) -> Provider {
    let c = core.db();
    db::get_provider(&c, &agent.provider_id)
        .ok()
        .flatten()
        .or_else(|| db::get_provider(&c, "mock").ok().flatten())
        .unwrap_or_else(|| providers::builtin_providers().remove(0))
}

fn set_agent(core: &Core, agent_id: i64, status: &str, mission: Option<i64>, note: &str) {
    let _ = db::set_agent_status(&core.db(), agent_id, status);
    core.emit(json!({"type": "agent", "id": agent_id, "status": status, "mission_id": mission, "note": note}));
}

fn set_mission(core: &Core, id: i64, status: &str) {
    let _ = db::set_mission_status(&core.db(), id, status);
    core.emit(json!({"type": "mission", "id": id, "status": status}));
}

fn office_context(core: &Core) -> String {
    let c = core.db();
    let name = db::get_setting(&c, "office_name").ok().flatten().unwrap_or_default();
    let desc = db::get_setting(&c, "office_description").ok().flatten().unwrap_or_default();
    if desc.trim().is_empty() {
        format!("Văn phòng: {name}")
    } else {
        format!("Văn phòng: {name}\nMô tả: {desc}")
    }
}

async fn ask(core: &Core, agent: &Agent, prompt: &str, cancel: Cancel) -> Result<String> {
    ask_in(core, agent, prompt, cancel, None).await
}

/// Gọi AI của nhân viên; nếu có bối cảnh tệp thì chạy trong thư mục làm việc và kèm tài liệu.
async fn ask_in(core: &Core, agent: &Agent, prompt: &str, cancel: Cancel, ctx: Option<&workspace::Ctx>) -> Result<String> {
    ask_full(core, agent, prompt, cancel, ctx, None, None).await
}

/// Chuẩn hoá mức suy luận ('' = mặc định của AI).
pub fn norm_effort(e: &str) -> String {
    let e = e.trim().to_lowercase();
    if providers::EFFORTS.contains(&e.as_str()) { e } else { String::new() }
}

/// AI thực sự dùng cho một bước: AI riêng của bước nếu có, không thì AI của nhân viên.
pub fn resolve_ai(core: &Core, agent: &Agent, sub: Option<&Subtask>) -> (Provider, String, String, bool) {
    let base = provider_of(core, agent);
    let Some(s) = sub else { return (base, agent.model.clone(), agent.effort.clone(), false) };
    let over = if s.provider_id.is_empty() { None } else { db::get_provider(&core.db(), &s.provider_id).ok().flatten().filter(|p| p.enabled) };
    let (p, model) = match over {
        Some(p) => {
            let m = if !s.model.is_empty() { s.model.clone() } else if p.id == agent.provider_id { agent.model.clone() } else { String::new() };
            (p, m)
        }
        None => (base, if s.model.is_empty() { agent.model.clone() } else { s.model.clone() }),
    };
    let effort = if s.effort.is_empty() { agent.effort.clone() } else { s.effort.clone() };
    (p, model, effort, s.web)
}

async fn ask_full(core: &Core, agent: &Agent, prompt: &str, cancel: Cancel, ctx: Option<&workspace::Ctx>, sub: Option<&Subtask>, on_progress: Option<providers::Progress>) -> Result<String> {
    let (p, model, effort, web) = resolve_ai(core, agent, sub);
    let wd = ctx.and_then(|c| c.cwd.clone()).unwrap_or_else(|| core.workdir(agent.id));
    let path_env = core.path_env();
    let persona = format!("{} ({})", agent.name, agent.title);
    let (allow_edit, dirs) = ctx.map(|c| (c.allow_edit, c.dirs.clone())).unwrap_or_default();
    let extra = providers::cli_extra(&p, allow_edit, &dirs, &effort, web);
    let full = match ctx {
        Some(c) if !c.is_empty() => format!("{prompt}\n{}", c.for_kind(&p.kind)),
        _ => prompt.to_string(),
    };
    let call = Call {
        model: &model,
        system: &agent.system_prompt,
        prompt: &full,
        workdir: &wd,
        path_env: &path_env,
        persona: &persona,
        extra_args: &extra,
        effort: &effort,
        web,
        on_progress,
    };
    providers::invoke(&p, &call, cancel).await
}

// ───────────────────────────── Tuyển dụng ─────────────────────────────

pub struct RecruitResult {
    pub candidates: Vec<Candidate>,
    pub source: String,
    pub note: String,
}

/// Đề xuất ứng viên; nếu `optimize` thì gán AI + model phù hợp nhất trong số kết nối đang dùng được.
pub async fn recruit(core: &Core, job: &str, count: usize, provider_id: Option<String>, optimize: bool) -> Result<RecruitResult> {
    let mut r = recruit_raw(core, job, count, provider_id).await?;
    if optimize {
        let avail = setup::available(core).await?;
        assign_ai(&mut r.candidates, &avail);
        if avail.is_empty() {
            r.note = format!("{} Chưa có AI nào sẵn sàng nên nhân viên tạm dùng chế độ mô phỏng — vào Kết nối AI để cài/đăng nhập.", r.note).trim().to_string();
        }
    }
    Ok(r)
}

/// Gán AI cho danh sách ứng viên bằng bộ tối ưu (giữ nguyên nếu không có AI nào dùng được).
pub fn assign_ai(list: &mut [Candidate], avail: &[Provider]) {
    let mut load = HashMap::new();
    // vị trí cấp cao chọn trước để được ưu tiên AI mạnh
    let mut order: Vec<usize> = (0..list.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(level_rank(&list[i].level)));
    for i in order {
        let c = &mut list[i];
        match optimizer::choose(avail, &c.title, &c.skills, &c.department, &c.level, &mut load) {
            Some(ch) => {
                c.provider_id = ch.provider_id;
                c.model = ch.model;
                c.effort = ch.effort;
                c.ai_reason = ch.reason;
            }
            None => {
                c.provider_id = "mock".into();
                c.model.clear();
                c.ai_reason = "Chưa có AI thật nào sẵn sàng — dùng chế độ mô phỏng.".into();
            }
        }
    }
}

fn level_rank(l: &str) -> u8 {
    match l {
        "chuyen_gia" => 4,
        "chuyen_nghiep" => 3,
        "trung_cap" => 2,
        _ => 1,
    }
}

/// Đề xuất đổi AI/model cho nhân viên hiện có (kể cả điều phối viên).
pub async fn optimize_preview(core: &Core) -> Result<Vec<Value>> {
    let avail = setup::available(core).await?;
    let agents = db::list_agents(&core.db())?;
    let mut load = HashMap::new();
    let mut order: Vec<&Agent> = agents.iter().collect();
    order.sort_by_key(|a| (!a.is_dispatcher, std::cmp::Reverse(level_rank(&a.level))));
    let mut out = vec![];
    for a in order {
        let (title, skills) = if a.is_dispatcher {
            (format!("{} điều phối phân công kế hoạch", a.title), a.skills.clone())
        } else {
            (a.title.clone(), a.skills.clone())
        };
        let level = if a.is_dispatcher { "chuyen_gia" } else { a.level.as_str() };
        let Some(ch) = optimizer::choose(&avail, &title, &skills, &a.department, level, &mut load) else { continue };
        let changed = ch.provider_id != a.provider_id || ch.model != a.model || ch.effort != a.effort;
        out.push(json!({
            "agent_id": a.id, "name": a.name, "title": a.title, "level": a.level, "is_dispatcher": a.is_dispatcher,
            "current_provider": a.provider_id, "current_model": a.model,
            "provider_id": ch.provider_id, "model": ch.model, "effort": ch.effort, "current_effort": a.effort, "reason": ch.reason, "changed": changed,
        }));
    }
    Ok(out)
}

pub fn apply_ai(core: &Core, items: &[(i64, String, String, String)], actor: &str) -> Result<usize> {
    let c = core.db();
    let mut n = 0;
    for (id, pid, model, effort) in items {
        if db::get_provider(&c, pid)?.is_none() {
            continue;
        }
        if let Some(mut a) = db::get_agent(&c, *id)? {
            a.provider_id = pid.clone();
            a.model = model.clone();
            a.effort = norm_effort(effort);
            db::update_agent(&c, &a)?;
            n += 1;
        }
    }
    db::log(&c, "ket_noi", actor, &format!("Tối ưu AI cho {n} nhân viên"), None, None)?;
    drop(c);
    core.emit(json!({"type": "agents_changed"}));
    Ok(n)
}

async fn recruit_raw(core: &Core, job: &str, count: usize, provider_id: Option<String>) -> Result<RecruitResult> {
    let count = count.clamp(1, 12);
    let (disp, provider) = {
        let c = core.db();
        let d = db::dispatcher(&c)?.ok_or_else(|| anyhow!("Chưa có điều phối viên"))?;
        let pid = provider_id.filter(|s| !s.is_empty()).unwrap_or_else(|| d.provider_id.clone());
        let p = db::get_provider(&c, &pid)?.ok_or_else(|| anyhow!("Không tìm thấy kết nối AI {pid}"))?;
        (d, p)
    };
    if provider.kind == "mock" {
        return Ok(RecruitResult {
            candidates: finalize(templates::offline_candidates(job, count), &provider.id),
            source: "Bộ chọn từ khoá (mô phỏng)".into(),
            note: String::new(),
        });
    }
    let library: Vec<String> = templates::library()
        .iter()
        .map(|t| format!("{} [{}]", t.title, level_label(&t.level)))
        .collect();
    let prompt = format!(
        "Bạn là chuyên gia nhân sự của một văn phòng gồm các nhân viên AI. Mỗi nhân viên AI là một trợ lý có vai trò, \
         kỹ năng và lời nhắc hệ thống (system prompt) riêng.\n\n\
         MÔ TẢ CÔNG VIỆC CỦA VĂN PHÒNG:\n{job}\n\n\
         Hãy đề xuất đúng {count} ứng viên nhân viên AI phù hợp nhất, đa dạng cấp độ từ cơ bản đến chuyên gia khi hợp lý, \
         không trùng vai trò. Có thể tham khảo các vị trí mẫu: {lib}.\n\n\
         Chỉ trả về DUY NHẤT một JSON hợp lệ, không giải thích thêm, theo dạng:\n\
         {{\"candidates\":[{{\"name\":\"Họ tên tiếng Việt\",\"title\":\"Chức danh\",\"department\":\"Phòng ban\",\
         \"level\":\"co_ban|trung_cap|chuyen_nghiep|chuyen_gia\",\"skills\":[\"kỹ năng\"],\
         \"system_prompt\":\"Lời nhắc hệ thống chi tiết, tiếng Việt, mô tả vai trò, phạm vi, cách trình bày kết quả\",\
         \"avatar\":\"một trong: {avatars}\",\"reason\":\"Vì sao cần vị trí này\"}}]}}",
        lib = library.join("; "),
        avatars = templates::AVATARS.join(", "),
    );
    set_agent(core, disp.id, "thinking", None, "Đang sàng lọc ứng viên");
    let (_tx, rx) = watch::channel(false);
    let mut d2 = disp.clone();
    d2.provider_id = provider.id.clone();
    let res = ask(core, &d2, &prompt, rx).await;
    set_agent(core, disp.id, "idle", None, "");
    match res {
        Ok(text) => {
            let list = extract_json(&text).and_then(|v| {
                let arr = v.get("candidates").cloned().unwrap_or(v);
                serde_json::from_value::<Vec<Candidate>>(arr).ok()
            });
            match list {
                Some(mut l) if !l.is_empty() => {
                    l.truncate(count);
                    Ok(RecruitResult { candidates: finalize(l, &provider.id), source: provider.name.clone(), note: String::new() })
                }
                _ => Ok(RecruitResult {
                    candidates: finalize(templates::offline_candidates(job, count), &provider.id),
                    source: "Bộ chọn từ khoá".into(),
                    note: "AI trả về sai định dạng, đã dùng bộ chọn từ khoá thay thế.".into(),
                }),
            }
        }
        Err(e) => Ok(RecruitResult {
            candidates: finalize(templates::offline_candidates(job, count), &provider.id),
            source: "Bộ chọn từ khoá".into(),
            note: format!("Không gọi được {}: {e}. Đã dùng bộ chọn từ khoá thay thế.", provider.name),
        }),
    }
}

fn finalize(mut list: Vec<Candidate>, provider_id: &str) -> Vec<Candidate> {
    for (i, c) in list.iter_mut().enumerate() {
        c.title = c.title.trim().to_string();
        if c.title.is_empty() {
            c.title = "Nhân viên tổng hợp".into();
        }
        c.level = templates::normalize_level(&c.level);
        if !templates::valid_avatar(&c.avatar) {
            c.avatar = templates::AVATARS[(i * 7 + 1) % templates::AVATARS.len()].into();
        }
        if c.name.trim().is_empty() {
            c.name = templates::random_name(&c.avatar, i);
        }
        if !c.color.starts_with('#') || c.color.len() != 7 {
            c.color = templates::palette(i).into();
        }
        if c.system_prompt.trim().is_empty() {
            c.system_prompt = format!("Bạn là {} trong một văn phòng AI. Luôn trả lời bằng tiếng Việt, trình bày Markdown rõ ràng.", c.title);
        }
        if c.provider_id.is_empty() {
            c.provider_id = provider_id.into();
        }
        c.skills.retain(|s| !s.trim().is_empty());
    }
    list
}

pub fn hire(core: &Core, list: &[Candidate], actor: &str) -> Result<Vec<i64>> {
    let c = core.db();
    let mut ids = vec![];
    for (i, cand) in finalize(list.to_vec(), "mock").into_iter().enumerate() {
        let provider = if db::get_provider(&c, &cand.provider_id)?.is_some() { cand.provider_id.clone() } else { "mock".into() };
        let a = Agent {
            id: 0,
            name: cand.name.clone(),
            title: cand.title.clone(),
            department: cand.department.clone(),
            level: cand.level.clone(),
            skills: cand.skills.clone(),
            system_prompt: cand.system_prompt.clone(),
            provider_id: provider,
            model: cand.model.clone(),
            avatar: cand.avatar.clone(),
            color: if cand.color.is_empty() { templates::palette(i).into() } else { cand.color.clone() },
            is_dispatcher: false,
            desk: 0,
            status: "idle".into(),
            tasks_done: 0,
            created_at: String::new(),
            effort: norm_effort(&cand.effort),
        };
        let id = db::insert_agent(&c, &a)?;
        db::log(&c, "nhan_su", actor, &format!("Tuyển {} — {}", a.name, a.title), None, Some(id))?;
        ids.push(id);
    }
    drop(c);
    core.emit(json!({"type": "agents_changed"}));
    Ok(ids)
}

// ───────────────────────────── Quy trình nhiều AI ─────────────────────────────

fn stage(step: i64, title: &str, instruction: &str, role: &str, provider: &str, model: &str, effort: &str, web: bool) -> db::Stage {
    db::Stage { step, title: title.into(), instruction: instruction.into(), agent_id: None, role: role.into(), provider_id: provider.into(), model: model.into(), effort: effort.into(), web }
}

/// Quy trình mẫu: mỗi bước dùng AI của một hãng khác nhau.
pub fn builtin_pipelines() -> Vec<db::Pipeline> {
    vec![
        db::Pipeline {
            id: 0,
            name: "Nghiên cứu → Tổng hợp → Kiểm chứng".into(),
            description: "Gemini tra cứu thông tin, Claude viết tổng hợp, GPT (Codex) kiểm tra chéo lại thông tin.".into(),
            stages: vec![
                stage(1, "Tra cứu thông tin", "Tìm kiếm, thu thập thông tin và số liệu mới nhất liên quan; ghi rõ nguồn cho từng ý.", "nghiên cứu tra cứu phân tích", "gemini-cli", "gemini-2.5-pro", "high", true),
                stage(2, "Viết báo cáo tổng hợp", "Dựa trên kết quả tra cứu, viết báo cáo tổng hợp mạch lạc, có cấu trúc, giữ trích dẫn nguồn.", "soạn thảo văn bản báo cáo", "claude-cli", "opus", "high", false),
                stage(3, "Kiểm tra lại thông tin", "Kiểm tra chéo từng số liệu, mốc thời gian và nhận định trong báo cáo; liệt kê chỗ sai/thiếu nguồn và đề xuất sửa.", "kiểm thử kiểm tra chất lượng pháp chế", "codex-cli", "", "xhigh", false),
            ],
            builtin: true,
        },
        db::Pipeline {
            id: 0,
            name: "Lập trình → Review chéo → Kiểm thử".into(),
            description: "Claude viết mã, Codex review chéo, Gemini viết ca kiểm thử và tài liệu.".into(),
            stages: vec![
                stage(1, "Viết mã nguồn", "Thiết kế và viết mã nguồn đáp ứng yêu cầu, kèm hướng dẫn chạy.", "lập trình", "claude-cli", "opus", "high", false),
                stage(2, "Review mã nguồn", "Review chéo mã ở bước trước: lỗi logic, bảo mật, hiệu năng; đề xuất bản sửa cụ thể.", "kiến trúc bảo mật lập trình", "codex-cli", "", "xhigh", false),
                stage(3, "Kiểm thử & tài liệu", "Viết các ca kiểm thử và tài liệu sử dụng cho mã đã được review.", "kiểm thử tài liệu", "gemini-cli", "gemini-3.5-flash", "medium", false),
            ],
            builtin: true,
        },
        db::Pipeline {
            id: 0,
            name: "Dịch → Hiệu đính".into(),
            description: "Gemini dịch nhanh, Claude hiệu đính văn phong và thuật ngữ.".into(),
            stages: vec![
                stage(1, "Dịch tài liệu", "Dịch đầy đủ, giữ nguyên cấu trúc và thuật ngữ chuyên ngành.", "dịch thuật biên dịch", "gemini-cli", "gemini-3.5-flash", "medium", false),
                stage(2, "Hiệu đính", "Hiệu đính bản dịch: văn phong tự nhiên, thống nhất thuật ngữ, đánh dấu chỗ đã sửa.", "soạn thảo biên dịch", "claude-cli", "sonnet", "high", false),
            ],
            builtin: true,
        },
    ]
}

/// Chọn nhân viên hợp nhất cho một bước theo từ khoá vai trò (ưu tiên người chưa nhận bước nào).
fn best_agent<'a>(role: &str, agents: &'a [Agent], used: &[i64]) -> Option<&'a Agent> {
    let words: Vec<String> = templates::fold(role).split_whitespace().filter(|w| w.len() > 2).map(String::from).collect();
    let score = |a: &Agent| -> i32 {
        let text = templates::fold(&format!("{} {} {}", a.title, a.department, a.skills.join(" ")));
        let s = words.iter().filter(|w| text.contains(w.as_str())).count() as i32 * 3;
        s - if used.contains(&a.id) { 2 } else { 0 }
    };
    agents.iter().max_by_key(|a| (score(a), -(a.id as i32)))
}

/// Tạo các đầu việc của nhiệm vụ theo một quy trình nhiều AI.
pub async fn apply_pipeline(core: &Core, mission_id: i64, pipeline_id: i64) -> Result<String> {
    let (pl, agents) = {
        let c = core.db();
        let pl = db::get_pipeline(&c, pipeline_id)?.ok_or_else(|| anyhow!("Không tìm thấy quy trình"))?;
        let agents: Vec<Agent> = db::list_agents(&c)?.into_iter().filter(|a| !a.is_dispatcher).collect();
        (pl, agents)
    };
    if agents.is_empty() {
        return Err(anyhow!("Văn phòng chưa có nhân viên để chạy quy trình"));
    }
    let ready: Vec<String> = setup::available(core).await?.into_iter().map(|p| p.id).collect();
    let mut notes = vec![format!("Chạy theo quy trình **{}**: {}", pl.name, pl.description)];
    let mut used = vec![];
    let c = core.db();
    for st in pl.stages.iter() {
        let agent = st
            .agent_id
            .and_then(|id| agents.iter().find(|a| a.id == id))
            .or_else(|| best_agent(&format!("{} {}", st.role, st.title), &agents, &used))
            .ok_or_else(|| anyhow!("Không chọn được nhân viên cho bước {}", st.title))?;
        used.push(agent.id);
        let sid = db::insert_subtask(&c, mission_id, Some(agent.id), st.step.max(1), &st.title, &st.instruction)?;
        let pname = db::get_provider(&c, &st.provider_id)?.map(|p| p.name).unwrap_or_default();
        let (prov, model) = if st.provider_id.is_empty() || ready.contains(&st.provider_id) {
            (st.provider_id.clone(), st.model.clone())
        } else {
            notes.push(format!("- Bước {} “{}”: {} chưa sẵn sàng → dùng AI của {}.", st.step, st.title, if pname.is_empty() { st.provider_id.clone() } else { pname.clone() }, agent.name));
            (String::new(), String::new())
        };
        db::set_subtask_ai(&c, sid, &prov, &model, &norm_effort(&st.effort), st.web)?;
        notes.push(format!(
            "- Bước {} “{}” → {} · {}{} · suy luận {}{}",
            st.step, st.title, agent.name,
            if prov.is_empty() { "AI của nhân viên".to_string() } else { pname },
            if model.is_empty() { String::new() } else { format!(" ({model})") },
            providers::effort_label(&norm_effort(&st.effort)),
            if st.web { " · tra cứu web" } else { "" },
        ));
    }
    let text = notes.join("\n");
    db::set_mission_text(&c, mission_id, "analysis", &text)?;
    drop(c);
    core.emit(json!({"type": "mission", "id": mission_id}));
    Ok(text)
}

// ───────────────────────────── Lập kế hoạch ─────────────────────────────

/// Phân công ngoại tuyến: chọn nhân viên có kỹ năng khớp nhất với mô tả nhiệm vụ.
pub fn heuristic_plan(title: &str, description: &str, agents: &[Agent]) -> Vec<(i64, i64, String, String)> {
    let text = templates::fold(&format!("{title} {description}"));
    let lib = templates::library();
    let mut scored: Vec<(usize, &Agent)> = agents
        .iter()
        .map(|a| {
            let mut words: Vec<String> = a.skills.iter().map(|s| templates::fold(s)).collect();
            words.push(templates::fold(&a.title));
            words.push(templates::fold(&a.department));
            if let Some(t) = lib.iter().find(|t| t.title == a.title) {
                words.extend(t.keywords.iter().cloned());
            }
            let mut s = 0;
            for w in &words {
                if w.len() > 2 && text.contains(w.as_str()) {
                    s += 3;
                }
                for part in w.split_whitespace().filter(|p| p.len() > 3) {
                    if text.contains(part) {
                        s += 1;
                    }
                }
            }
            (s, a)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
    let mut chosen: Vec<&Agent> = scored.iter().filter(|(s, _)| *s > 0).take(3).map(|(_, a)| *a).collect();
    if chosen.is_empty() {
        if let Some((_, a)) = scored.first() {
            chosen.push(a);
        }
    }
    let mut out = vec![];
    for a in &chosen {
        out.push((
            a.id,
            1,
            format!("{} — phần việc của {}", title, a.title),
            format!("Thực hiện nhiệm vụ \"{title}\" dưới góc độ chuyên môn {}. Yêu cầu chi tiết: {description}", a.title),
        ));
    }
    if chosen.len() > 1 {
        // Người có điểm cao nhất rà soát và hoàn thiện ở bước 2.
        let lead = chosen[0];
        out.push((
            lead.id,
            2,
            format!("Rà soát và hoàn thiện: {title}"),
            "Đọc kết quả của các đồng nghiệp ở bước trước, hợp nhất thành sản phẩm hoàn chỉnh, sửa chỗ mâu thuẫn.".into(),
        ));
    }
    out
}

pub async fn plan(core: &Core, mission_id: i64, cancel: Cancel) -> Result<()> {
    let (mission, disp, agents) = {
        let c = core.db();
        let m = db::get_mission(&c, mission_id)?.ok_or_else(|| anyhow!("Không tìm thấy nhiệm vụ"))?;
        let d = db::dispatcher(&c)?.ok_or_else(|| anyhow!("Chưa có điều phối viên"))?;
        let a: Vec<Agent> = db::list_agents(&c)?.into_iter().filter(|a| !a.is_dispatcher).collect();
        (m, d, a)
    };
    if agents.is_empty() {
        return Err(anyhow!("Văn phòng chưa có nhân viên. Hãy tuyển dụng trước khi giao việc."));
    }
    set_mission(core, mission_id, "dang_lap_ke_hoach");
    set_agent(core, disp.id, "thinking", Some(mission_id), "Đang lập kế hoạch");
    core.log("ke_hoach", &disp.name, &format!("Bắt đầu phân tích nhiệm vụ \"{}\"", mission.title), Some(mission_id), Some(disp.id));

    let provider = provider_of(core, &disp);
    let mut analysis = String::new();
    let mut plan: Vec<(i64, i64, String, String)> = vec![];

    if provider.kind != "mock" {
        let roster: Vec<String> = agents
            .iter()
            .map(|a| {
                let ai = provider_of(core, a);
                format!(
                    "- agent_id={} | {} | {} | {} | Cấp độ: {} | AI: {}{} | Kỹ năng: {}",
                    a.id, a.name, a.title, a.department, level_label(&a.level), ai.name,
                    if a.model.is_empty() { String::new() } else { format!(" ({})", a.model) },
                    a.skills.join(", ")
                )
            })
            .collect();
        let prompt = format!(
            "{office}\n\nDANH SÁCH NHÂN VIÊN:\n{roster}\n\n\
             NHIỆM VỤ TỪ QUẢN LÝ:\nTiêu đề: {title}\nMô tả: {desc}\nMức ưu tiên: {prio}\nHạn: {deadline}\n\n\
             Hãy phân tích và phân rã nhiệm vụ thành 1–6 đầu việc, mỗi đầu việc giao cho MỘT nhân viên phù hợp nhất (dùng agent_id ở trên). \
             Tận dụng thế mạnh của từng AI: Gemini hợp tra cứu/tìm kiếm thông tin, Claude hợp viết và tổng hợp, \
             GPT/Codex hợp kiểm tra chéo và lập trình — nên để một AI khác hãng kiểm tra lại kết quả quan trọng. \
             Trường \"step\" thể hiện thứ tự: các đầu việc cùng step chạy song song, step sau được đọc kết quả của step trước. \
             Chỉ dẫn (instruction) phải cụ thể, nêu rõ đầu ra mong muốn.\n\n\
             Chỉ trả về DUY NHẤT một JSON hợp lệ dạng:\n\
             {{\"analysis\":\"Nhận định ngắn gọn về nhiệm vụ và lý do phân công\",\"subtasks\":[{{\"agent_id\":1,\"step\":1,\"title\":\"...\",\"instruction\":\"...\"}}]}}",
            office = office_context(core),
            roster = roster.join("\n"),
            title = mission.title,
            desc = mission.description,
            prio = mission.priority,
            deadline = if mission.deadline.is_empty() { "không có" } else { &mission.deadline },
        );
        // điều phối viên xem tài liệu ở chế độ chỉ đọc để phân công cho sát
        let mut pctx = workspace::prepare(core, &mission).unwrap_or_default();
        pctx.allow_edit = false;
        match ask_in(core, &disp, &prompt, cancel.clone(), Some(&pctx)).await {
            Ok(text) => {
                if let Some(v) = extract_json(&text) {
                    analysis = v["analysis"].as_str().unwrap_or("").to_string();
                    if let Some(arr) = v["subtasks"].as_array() {
                        for s in arr.iter().take(8) {
                            let aid = s["agent_id"].as_i64().or_else(|| s["agent_id"].as_str().and_then(|x| x.parse().ok()));
                            let Some(aid) = aid.filter(|id| agents.iter().any(|a| a.id == *id)) else { continue };
                            let step = s["step"].as_i64().unwrap_or(1).clamp(1, 10);
                            let title = s["title"].as_str().unwrap_or("Đầu việc").trim().to_string();
                            let ins = s["instruction"].as_str().unwrap_or("").trim().to_string();
                            plan.push((aid, step, title, ins));
                        }
                    }
                }
                if plan.is_empty() {
                    analysis = format!("{analysis}\n\n_(AI điều phối trả về kế hoạch không hợp lệ — dùng phân công tự động theo kỹ năng.)_");
                }
            }
            Err(e) => {
                if *cancel.borrow() {
                    set_agent(core, disp.id, "idle", Some(mission_id), "");
                    return Err(anyhow!("Đã huỷ"));
                }
                core.log("canh_bao", &disp.name, &format!("Không gọi được AI điều phối: {e}"), Some(mission_id), Some(disp.id));
                analysis = format!("_(Không gọi được {}: {} — dùng phân công tự động theo kỹ năng.)_", provider.name, clip(&e.to_string(), 300));
            }
        }
    }
    if plan.is_empty() {
        plan = heuristic_plan(&mission.title, &mission.description, &agents);
        if analysis.is_empty() {
            analysis = "Phân công tự động theo mức độ phù hợp kỹ năng (chế độ mô phỏng).".into();
        }
    }

    {
        let c = core.db();
        for s in db::list_subtasks(&c, mission_id)? {
            if s.status != "xong" {
                db::delete_subtask(&c, s.id)?;
            }
        }
        for (aid, step, title, ins) in &plan {
            db::insert_subtask(&c, mission_id, Some(*aid), *step, title, ins)?;
        }
        db::set_mission_text(&c, mission_id, "analysis", analysis.trim())?;
    }
    for (aid, _, title, _) in &plan {
        core.emit(json!({"type": "assign", "from": disp.id, "to": aid, "mission_id": mission_id, "title": title}));
    }
    let names: Vec<String> = {
        let c = core.db();
        plan.iter().filter_map(|(a, ..)| db::get_agent(&c, *a).ok().flatten().map(|x| x.name)).collect()
    };
    core.log("phan_cong", &disp.name, &format!("Đã phân công {} đầu việc cho: {}", plan.len(), names.join(", ")), Some(mission_id), Some(disp.id));
    set_agent(core, disp.id, "idle", Some(mission_id), "");
    set_mission(core, mission_id, "cho_duyet");
    Ok(())
}

// ───────────────────────────── Thực thi ─────────────────────────────

fn build_task_prompt(core: &Core, mission: &db::Mission, sub: &Subtask, previous: &[Subtask]) -> String {
    let mut p = format!(
        "{}\n\n## NHIỆM VỤ CHUNG CỦA VĂN PHÒNG\nTiêu đề: {}\nMô tả: {}\n",
        office_context(core),
        mission.title,
        mission.description
    );
    // trí nhớ của nhân viên (sở thích, bài học từ các lần chỉnh sửa trước)
    if let Some(aid) = sub.agent_id {
        let mem = crate::memory::prompt_section(&core.db(), aid).unwrap_or_default();
        p.push_str(&mem);
    }
    // mẫu kết quả: áp dụng cho bước cuối cùng của nhiệm vụ
    if let Some(tid) = mission.template_id {
        let (tpl, last_step) = {
            let c = core.db();
            (crate::memory::get_template(&c, tid).ok().flatten(), db::list_subtasks(&c, mission.id).map(|v| v.iter().map(|s| s.step).max().unwrap_or(1)).unwrap_or(1))
        };
        if let Some(t) = tpl.filter(|_| sub.step >= last_step) {
            p.push_str(&crate::memory::template_section(&t));
        }
    }
    if !mission.deadline.is_empty() {
        p.push_str(&format!("Hạn hoàn thành: {}\n", mission.deadline));
    }
    if !previous.is_empty() {
        p.push_str("\n## KẾT QUẢ CỦA ĐỒNG NGHIỆP Ở CÁC BƯỚC TRƯỚC\n");
        let per = (12000 / previous.len()).max(1500);
        for s in previous {
            p.push_str(&format!("\n### {} — {}\n{}\n", s.title, s.agent_name, clip(&s.output, per)));
        }
    }
    p.push_str(&format!(
        "\n## PHẦN VIỆC ĐƯỢC GIAO CHO BẠN\n{}\n{}\n\nHãy hoàn thành phần việc trên và trả về kết quả cuối cùng (tiếng Việt, Markdown).\n\
         Chỉ khi THIẾU thông tin bắt buộc mà không thể tự giả định hợp lý, hãy dừng lại và trả lời DUY NHẤT một dòng: \
         `{} <câu hỏi ngắn gọn cho Quản lý>` — Quản lý sẽ trả lời rồi bạn làm tiếp.\n\n\
         ## CÁCH XUẤT TỆP\n\
         Khi được yêu cầu tạo/xuất tệp (Word .docx, PDF .pdf, Excel .xlsx, .csv, .md, .html, .txt), hãy viết NỘI DUNG từng tệp \
         bằng Markdown (bảng dùng bảng Markdown) và đặt giữa hai dòng:\n\
         {} ten-tep.docx>>>\n…nội dung…\n{}\n\
         Ứng dụng sẽ tự tạo tệp đúng định dạng và lưu vào thư mục kết quả của nhiệm vụ. \
         Có thể tạo nhiều tệp; ngoài các khối tệp, hãy tóm tắt ngắn gọn đã làm gì.\n\n## GHI NHỚ\n{}",
        sub.title, sub.instruction, providers::QUESTION_MARK, crate::export::FILE_START, crate::export::FILE_END, crate::memory::MEMORY_RULE
    ));
    p
}

async fn run_subtask(core: Core, sub_id: i64, cancel: Cancel, ctx: std::sync::Arc<workspace::Ctx>) -> Result<()> {
    let (sub, mission, agent) = {
        let c = core.db();
        let s = db::get_subtask(&c, sub_id)?.ok_or_else(|| anyhow!("Không tìm thấy đầu việc"))?;
        let m = db::get_mission(&c, s.mission_id)?.ok_or_else(|| anyhow!("Không tìm thấy nhiệm vụ"))?;
        let a = match s.agent_id {
            Some(id) => db::get_agent(&c, id)?,
            None => None,
        };
        (s, m, a)
    };
    let Some(agent) = agent else {
        db::subtask_finish(&core.db(), sub_id, "loi", "", "Đầu việc chưa có người nhận (nhân viên đã bị xoá?)", 0)?;
        return Err(anyhow!("Đầu việc \"{}\" chưa có người nhận", sub.title));
    };
    // Mỗi nhân viên làm một việc tại một thời điểm; việc khác xếp hàng chờ.
    let lock = core.agent_lock(agent.id);
    let _guard = tokio::select! {
        g = lock.lock() => g,
        _ = providers::cancelled(cancel.clone()) => {
            db::subtask_finish(&core.db(), sub_id, "huy", "", "Đã huỷ", 0)?;
            return Err(anyhow!("Đã huỷ"));
        }
    };
    let previous: Vec<Subtask> = db::list_subtasks(&core.db(), mission.id)?
        .into_iter()
        .filter(|s| s.step < sub.step && s.status == "xong")
        .collect();
    let prompt = build_task_prompt(&core, &mission, &sub, &previous);
    // trao đổi: sang bàn đồng nghiệp đã làm bước trước để nhận kết quả
    let mut seen = vec![agent.id];
    for p in &previous {
        if let Some(pid) = p.agent_id {
            if !seen.contains(&pid) && seen.len() <= 2 {
                seen.push(pid);
                core.emit(json!({"type": "exchange", "from": agent.id, "to": pid, "mission_id": mission.id, "title": sub.title}));
            }
        }
    }
    db::subtask_start(&core.db(), sub_id)?;
    core.emit(json!({"type": "subtask", "id": sub_id, "mission_id": mission.id, "status": "dang_lam"}));
    set_agent(&core, agent.id, "working", Some(mission.id), &sub.title);
    core.log("thuc_hien", &agent.name, &format!("Bắt đầu: {}", sub.title), Some(mission.id), Some(agent.id));

    let scan_dir = ctx.cwd.clone().unwrap_or_else(|| core.workdir(agent.id));
    let before = crate::export::snapshot(&scan_dir);
    let t0 = Instant::now();
    let progress = {
        let core2 = core.clone();
        let (aid, mid) = (agent.id, mission.id);
        core.0.progress.lock().unwrap_or_else(|e| e.into_inner()).insert(sub_id, vec![]);
        std::sync::Arc::new(move |line: String| {
            let stamped = format!("{} {}", chrono::Local::now().format("%H:%M:%S"), line);
            {
                let mut m = core2.0.progress.lock().unwrap_or_else(|e| e.into_inner());
                let v = m.entry(sub_id).or_default();
                v.push(stamped.clone());
                if v.len() > 400 {
                    v.drain(..100);
                }
            }
            core2.emit(json!({"type": "progress", "subtask_id": sub_id, "agent_id": aid, "mission_id": mid, "line": stamped}));
        }) as providers::Progress
    };
    let res = ask_full(&core, &agent, &prompt, cancel.clone(), Some(&ctx), Some(&sub), Some(progress)).await;
    let ms = t0.elapsed().as_millis() as i64;
    let log = core.0.progress.lock().unwrap_or_else(|e| e.into_inner()).remove(&sub_id).unwrap_or_default().join("\n");
    let _ = db::set_subtask_log(&core.db(), sub_id, &log);
    if let Ok(out) = &res {
        if let Some(q) = providers::extract_question(out) {
            {
                let c = core.db();
                db::subtask_finish(&c, sub_id, "hoi", "", "", ms)?;
                db::set_subtask_question(&c, sub_id, &q)?;
            }
            core.emit(json!({"type": "subtask", "id": sub_id, "mission_id": mission.id, "status": "hoi"}));
            set_agent(&core, agent.id, "asking", Some(mission.id), &q);
            core.emit(json!({"type": "question", "agent_id": agent.id, "subtask_id": sub_id, "mission_id": mission.id, "question": q, "title": sub.title}));
            core.log("hoi", &agent.name, &format!("Hỏi lại: {q}"), Some(mission.id), Some(agent.id));
            return Err(anyhow!("{ASK} {}", sub.title));
        }
    }
    // tạo tệp thật từ khối tệp trong câu trả lời + liệt kê tệp AI đã tạo/sửa trong thư mục làm việc
    let res = res.map(|out| {
        let (blocks, cleaned) = crate::export::extract_file_blocks(&out);
        let mut files = crate::export::diff(&before, &crate::export::snapshot(&scan_dir));
        if !blocks.is_empty() {
            let out_dir = if !mission.workdir.trim().is_empty() && mission.allow_edit {
                std::path::PathBuf::from(&mission.workdir)
            } else {
                workspace::mission_dir(&core, mission.id).join("ket-qua")
            };
            let _ = std::fs::create_dir_all(&out_dir);
            let made = crate::export::materialize(&blocks, &out_dir, &mission.title);
            files.retain(|f| !made.iter().any(|m| m.path == f.path));
            files.splice(0..0, made);
        }
        if !files.is_empty() {
            let _ = db::set_subtask_files(&core.db(), sub_id, &files);
            core.emit(json!({"type": "files", "subtask_id": sub_id, "mission_id": mission.id, "files": files}));
            core.log("tep", &agent.name, &format!("Tạo/sửa {} tệp: {}", files.len(), files.iter().take(4).map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")), Some(mission.id), Some(agent.id));
        } else {
            let _ = db::set_subtask_files(&core.db(), sub_id, &[]);
        }
        let out = if blocks.is_empty() { out } else { cleaned };
        let (mems, out) = crate::memory::extract(&out);
        for (m, office) in &mems {
            let added = crate::memory::add(&core.db(), if *office { None } else { Some(agent.id) }, m, "ai", Some(mission.id)).ok().flatten();
            if added.is_some() {
                core.log("ghi_nho", &agent.name, &format!("Ghi nhớ{}: {m}", if *office { " chung" } else { "" }), Some(mission.id), Some(agent.id));
                core.emit(json!({"type": "memory", "agent_id": agent.id, "content": m, "office": office}));
            }
        }
        out
    });
    match res {
        Ok(out) => {
            {
                let c = core.db();
                db::subtask_finish(&c, sub_id, "xong", &out, "", ms)?;
                db::inc_agent_done(&c, agent.id)?;
            }
            core.emit(json!({"type": "subtask", "id": sub_id, "mission_id": mission.id, "status": "xong"}));
            set_agent(&core, agent.id, "done", Some(mission.id), &sub.title);
            set_agent(&core, agent.id, "idle", Some(mission.id), "");
            core.log("hoan_thanh", &agent.name, &format!("Hoàn thành: {} ({:.1} giây)", sub.title, ms as f64 / 1000.0), Some(mission.id), Some(agent.id));
            Ok(())
        }
        Err(e) => {
            let was_cancel = *cancel.borrow();
            let st = if was_cancel { "huy" } else { "loi" };
            db::subtask_finish(&core.db(), sub_id, st, "", &e.to_string(), ms)?;
            core.emit(json!({"type": "subtask", "id": sub_id, "mission_id": mission.id, "status": st}));
            set_agent(&core, agent.id, if was_cancel { "idle" } else { "error" }, Some(mission.id), &e.to_string());
            if !was_cancel {
                core.log("loi", &agent.name, &format!("Lỗi ở \"{}\": {}", sub.title, clip(&e.to_string(), 300)), Some(mission.id), Some(agent.id));
            }
            Err(e)
        }
    }
}

pub async fn execute(core: &Core, mission_id: i64, cancel: Cancel) -> Result<()> {
    let subs = db::list_subtasks(&core.db(), mission_id)?;
    if subs.is_empty() {
        return Err(anyhow!("Nhiệm vụ chưa có đầu việc nào"));
    }
    if subs.iter().any(|s| s.status == "hoi") {
        return Err(anyhow!("{ASK} còn câu hỏi chưa được trả lời"));
    }
    set_mission(core, mission_id, "dang_thuc_hien");
    let ctx = {
        let m = db::get_mission(&core.db(), mission_id)?.ok_or_else(|| anyhow!("Không tìm thấy nhiệm vụ"))?;
        std::sync::Arc::new(workspace::prepare(core, &m)?)
    };
    let mut by_step: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for s in subs.iter().filter(|s| s.status != "xong") {
        by_step.entry(s.step).or_default().push(s.id);
    }
    for (step, ids) in by_step {
        let mut set = tokio::task::JoinSet::new();
        for id in ids {
            set.spawn(run_subtask(core.clone(), id, cancel.clone(), ctx.clone()));
        }
        let (mut failed, mut asked) = (0, 0);
        while let Some(r) = set.join_next().await {
            match r {
                Ok(Ok(())) => {}
                Ok(Err(e)) if e.to_string().starts_with(ASK) => asked += 1,
                _ => failed += 1,
            }
        }
        if *cancel.borrow() {
            return Err(anyhow!("Đã huỷ"));
        }
        if asked > 0 && failed == 0 {
            return Err(anyhow!("{ASK} {asked} nhân viên đang chờ quản lý trả lời"));
        }
        if failed > 0 {
            return Err(anyhow!("{failed} đầu việc ở bước {step} bị lỗi. Hãy sửa phân công hoặc kết nối AI rồi chạy lại."));
        }
    }
    summarize(core, mission_id, cancel).await?;
    set_mission(core, mission_id, "hoan_thanh");
    announce_done(core, mission_id);
    Ok(())
}

/// Nhiệm vụ xong: tự xuất báo cáo theo định dạng của mẫu (nếu có) và báo cho Quản lý —
/// điều phối viên trình báo cáo khi nhiều người cùng làm, hoặc người làm bước cuối khi chỉ một người.
pub fn announce_done(core: &Core, mission_id: i64) {
    let Ok(Some(m)) = db::get_mission(&core.db(), mission_id) else { return };
    let subs = db::list_subtasks(&core.db(), mission_id).unwrap_or_default();
    let tpl = m.template_id.and_then(|t| crate::memory::get_template(&core.db(), t).ok().flatten());
    let mut exported = None;
    if let Some(t) = tpl.as_ref().filter(|t| crate::export::FORMATS.iter().any(|(e, _)| *e == t.format)) {
        if !m.summary.trim().is_empty() {
            let dir = workspace::mission_dir(core, mission_id).join("ket-qua");
            let path = crate::export::unique_path(&dir, &format!("{}.{}", crate::export::safe_name(&m.title), t.format));
            match crate::export::export(&m.summary, &m.title, &t.format, &path) {
                Ok(()) => {
                    core.log("tep", "Hệ thống", &format!("Tự xuất báo cáo theo mẫu “{}”: {}", t.name, path.display()), Some(mission_id), None);
                    exported = Some(path.to_string_lossy().to_string());
                }
                Err(e) => core.log("loi", "Hệ thống", &format!("Không tự xuất được theo mẫu: {e}"), Some(mission_id), None),
            }
        }
    }
    let agents: std::collections::BTreeSet<i64> = subs.iter().filter_map(|s| s.agent_id).collect();
    let last = subs.iter().filter(|s| s.status == "xong").max_by_key(|s| (s.step, s.id));
    let disp = db::dispatcher(&core.db()).ok().flatten();
    let presenter = if agents.len() > 1 { disp.map(|d| d.id) } else { last.and_then(|s| s.agent_id) }.or_else(|| last.and_then(|s| s.agent_id));
    let files: usize = subs.iter().map(|s| s.files.len()).sum::<usize>() + exported.iter().count();
    let excerpt: String = m.summary.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).take(14).collect::<Vec<_>>().join("\n");
    core.emit(json!({
        "type": "mission_done", "mission_id": mission_id, "title": m.title, "presenter": presenter,
        "excerpt": clip(&excerpt, 900), "files": files, "exported": exported,
        "template": tpl.map(|t| t.name), "steps": subs.len(),
    }));
}

fn concat_report(mission: &db::Mission, subs: &[Subtask]) -> String {
    if subs.len() == 1 {
        return subs[0].output.clone();
    }
    let mut r = format!("# Báo cáo tổng hợp: {}\n", mission.title);
    for (i, s) in subs.iter().enumerate() {
        r.push_str(&format!("\n## {}. {} — {}\n\n{}\n", i + 1, s.title, s.agent_name, s.output));
    }
    r
}

async fn summarize(core: &Core, mission_id: i64, cancel: Cancel) -> Result<()> {
    let (mission, subs, disp) = {
        let c = core.db();
        (
            db::get_mission(&c, mission_id)?.ok_or_else(|| anyhow!("Không tìm thấy nhiệm vụ"))?,
            db::list_subtasks(&c, mission_id)?,
            db::dispatcher(&c)?,
        )
    };
    // nhiệm vụ đã được nhắn tiếp: kết quả là phản hồi mới nhất
    if let Some(last) = subs.iter().filter(|s| s.title.starts_with(FOLLOW)).max_by_key(|s| (s.step, s.id)) {
        if last.status == "xong" {
            db::set_mission_text(&core.db(), mission_id, "summary", &last.output)?;
            core.log("bao_cao", "Điều phối viên", &format!("Cập nhật kết quả \"{}\"", mission.title), Some(mission_id), None);
            return Ok(());
        }
    }
    let fallback = concat_report(&mission, &subs);
    let mut summary = fallback.clone();
    if let Some(d) = disp.filter(|_| subs.len() > 1) {
        if provider_of(core, &d).kind != "mock" {
            set_mission(core, mission_id, "dang_tong_hop");
            set_agent(core, d.id, "thinking", Some(mission_id), "Đang tổng hợp báo cáo");
            let prompt = format!(
                "Quản lý giao nhiệm vụ \"{}\": {}\n\nDưới đây là kết quả của các nhân viên:\n{}\n\n\
                 Hãy viết BÁO CÁO TỔNG HỢP gửi Quản lý: tóm tắt điều hành, kết quả chính (giữ nguyên các nội dung quan trọng), \
                 vấn đề còn tồn tại và đề xuất bước tiếp theo. Tiếng Việt, Markdown.{}",
                mission.title,
                mission.description,
                clip(&fallback, 30000),
                mission
                    .template_id
                    .and_then(|t| crate::memory::get_template(&core.db(), t).ok().flatten())
                    .map(|t| crate::memory::template_section(&t))
                    .unwrap_or_default()
            );
            match ask(core, &d, &prompt, cancel).await {
                Ok(s) => summary = s,
                Err(e) => core.log("canh_bao", &d.name, &format!("Không tổng hợp được bằng AI: {e}"), Some(mission_id), Some(d.id)),
            }
            set_agent(core, d.id, "idle", Some(mission_id), "");
        }
    }
    db::set_mission_text(&core.db(), mission_id, "summary", &summary)?;
    core.log("bao_cao", "Điều phối viên", &format!("Hoàn tất nhiệm vụ \"{}\"", mission.title), Some(mission_id), None);
    Ok(())
}

// ───────────────────────────── Hỏi đáp, nhắn tiếp, xem việc đang làm ─────────────────────────────

/// Đánh dấu lỗi "đang chờ trả lời" (không phải lỗi thật).
pub const ASK: &str = "[cho-tra-loi]";
/// Tiền tố tiêu đề của đầu việc nhắn tiếp.
pub const FOLLOW: &str = "↪ ";

/// Quản lý trả lời câu hỏi của nhân viên → ghi vào chỉ dẫn và cho chạy tiếp.
pub fn answer(core: &Core, subtask_id: i64, answer: &str, actor: &str) -> Result<i64> {
    let (sub, agent_id) = {
        let c = core.db();
        let s = db::get_subtask(&c, subtask_id)?.ok_or_else(|| anyhow!("Không tìm thấy đầu việc"))?;
        if s.status != "hoi" {
            return Err(anyhow!("Đầu việc này không có câu hỏi đang chờ"));
        }
        db::append_instruction(&c, subtask_id, &format!("\n\n[Quản lý trả lời câu hỏi “{}”]: {}", s.question, answer.trim()))?;
        db::set_subtask_question(&c, subtask_id, "")?;
        db::log(&c, "tra_loi", actor, &format!("Trả lời {}: {}", s.agent_name, answer.trim()), Some(s.mission_id), s.agent_id)?;
        let aid = s.agent_id;
        (s, aid)
    };
    if let Some(a) = agent_id {
        set_agent(core, a, "idle", Some(sub.mission_id), "");
    }
    core.emit(json!({"type": "answered", "subtask_id": subtask_id, "agent_id": agent_id, "mission_id": sub.mission_id}));
    let still = db::list_subtasks(&core.db(), sub.mission_id)?.iter().any(|s| s.status == "hoi");
    if !still && !core.is_running(sub.mission_id) {
        start(core, sub.mission_id, false, true)?;
    }
    Ok(sub.mission_id)
}

/// Câu hỏi đang chờ trả lời.
pub fn questions(core: &Core) -> Result<Vec<Value>> {
    let c = core.db();
    let mut st = c.prepare("SELECT s.id, s.agent_id, s.mission_id, s.question, s.title, m.title FROM subtasks s JOIN missions m ON m.id=s.mission_id WHERE s.status='hoi' ORDER BY s.id")?;
    let v = st
        .query_map([], |r| {
            Ok(json!({"subtask_id": r.get::<_, i64>(0)?, "agent_id": r.get::<_, Option<i64>>(1)?, "mission_id": r.get::<_, i64>(2)?,
                "question": r.get::<_, String>(3)?, "title": r.get::<_, String>(4)?, "mission_title": r.get::<_, String>(5)?}))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Nhắn tiếp cho một nhiệm vụ: thêm đầu việc mới ở bước sau, đọc toàn bộ kết quả trước đó.
pub fn followup(core: &Core, mission_id: i64, message: &str, agent_id: Option<i64>, actor: &str) -> Result<i64> {
    if core.is_running(mission_id) {
        return Err(anyhow!("Nhiệm vụ đang chạy, hãy chờ xong rồi nhắn tiếp"));
    }
    let msg = message.trim();
    if msg.is_empty() {
        return Err(anyhow!("Nội dung trống"));
    }
    let sid = {
        let c = core.db();
        db::get_mission(&c, mission_id)?.ok_or_else(|| anyhow!("Không tìm thấy nhiệm vụ"))?;
        let subs = db::list_subtasks(&c, mission_id)?;
        if subs.iter().any(|s| s.status == "hoi") {
            return Err(anyhow!("Hãy trả lời câu hỏi đang chờ trước"));
        }
        let last = subs.iter().max_by_key(|s| (s.step, s.id));
        let who = agent_id.or_else(|| last.and_then(|s| s.agent_id)).ok_or_else(|| anyhow!("Chưa chọn người làm"))?;
        db::get_agent(&c, who)?.ok_or_else(|| anyhow!("Nhân viên không tồn tại"))?;
        let step = last.map(|s| s.step + 1).unwrap_or(1).min(99);
        let title: String = msg.chars().take(60).collect();
        let title = format!("{FOLLOW}{}{}", title, if msg.chars().count() > 60 { "…" } else { "" });
        let instruction = format!(
            "{msg}\n\n(Đây là yêu cầu tiếp theo của Quản lý cho nhiệm vụ đã làm. Dựa trên toàn bộ kết quả trước đó ở trên, \
             hãy thực hiện tiếp hoặc chỉnh sửa theo yêu cầu và trả về KẾT QUẢ HOÀN CHỈNH MỚI.)"
        );
        let sid = db::insert_subtask(&c, mission_id, Some(who), step, &title, &instruction)?;
        // giữ AI riêng của bước trước nếu cùng người làm
        if let Some(l) = last.filter(|l| l.agent_id == Some(who) && !l.provider_id.is_empty()) {
            db::set_subtask_ai(&c, sid, &l.provider_id, &l.model, &l.effort, l.web)?;
        }
        db::log(&c, "giao_viec", actor, &format!("Nhắn tiếp: {msg}"), Some(mission_id), Some(who))?;
        sid
    };
    // Lưu ý: đọc xong rồi mới dùng — nếu gọi core.db() ngay trong điều kiện `if let`, khoá CSDL
    // bị giữ suốt cả khối và lần gọi core.db() bên trong sẽ tự khoá chết (làm app đơ).
    let (who, disp) = {
        let c = core.db();
        (db::get_subtask(&c, sid)?.and_then(|s| s.agent_id), db::dispatcher(&c)?.map(|d| d.id))
    };
    if let Some(a) = who {
        core.emit(json!({"type": "assign", "from": disp, "to": a, "mission_id": mission_id, "title": "Yêu cầu tiếp"}));
    }
    start(core, mission_id, false, true)?;
    Ok(sid)
}

/// Chi tiết việc một nhân viên đang làm / đang chờ / đang hỏi.
pub fn agent_work(core: &Core, agent_id: i64) -> Result<Value> {
    let c = core.db();
    let mut st = c.prepare(
        "SELECT s.id FROM subtasks s JOIN missions m ON m.id=s.mission_id
         WHERE s.agent_id=?1 AND (s.status IN ('dang_lam','hoi') OR (s.status='cho' AND m.status IN ('dang_thuc_hien','cho_duyet','cho_tra_loi','dang_lap_ke_hoach')))
         ORDER BY CASE s.status WHEN 'dang_lam' THEN 0 WHEN 'hoi' THEN 1 ELSE 2 END, s.id",
    )?;
    let ids: Vec<i64> = st.query_map([agent_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let mut items = vec![];
    for id in ids.iter().take(8) {
        let Some(s) = db::get_subtask(&c, *id)? else { continue };
        let m = db::get_mission(&c, s.mission_id)?;
        let steps: Vec<Value> = db::list_subtasks(&c, s.mission_id)?
            .iter()
            .map(|x| json!({"id": x.id, "step": x.step, "title": x.title, "agent_name": x.agent_name, "status": x.status}))
            .collect();
        let live = core.0.progress.lock().map(|p| p.get(id).cloned().unwrap_or_default()).unwrap_or_default();
        items.push(json!({"subtask": s, "mission_title": m.as_ref().map(|m| m.title.clone()), "mission_status": m.map(|m| m.status), "steps": steps, "live": live}));
    }
    // việc đã xong gần đây
    let mut st = c.prepare("SELECT s.id, s.title, s.finished_at, s.mission_id FROM subtasks s WHERE s.agent_id=?1 AND s.status='xong' ORDER BY s.finished_at DESC LIMIT 5")?;
    let recent: Vec<Value> = st
        .query_map([agent_id], |r| Ok(json!({"id": r.get::<_, i64>(0)?, "title": r.get::<_, String>(1)?, "finished_at": r.get::<_, String>(2)?, "mission_id": r.get::<_, i64>(3)?})))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(json!({"items": items, "recent": recent}))
}

// ───────────────────────────── Điều khiển chạy nền ─────────────────────────────

/// Chạy nền: lập kế hoạch (nếu `do_plan`) rồi thực thi (nếu `do_exec`).
pub fn start(core: &Core, mission_id: i64, do_plan: bool, do_exec: bool) -> Result<()> {
    let (tx, rx) = watch::channel(false);
    {
        let mut r = core.0.running.lock().unwrap_or_else(|e| e.into_inner());
        if r.contains_key(&mission_id) {
            return Err(anyhow!("Nhiệm vụ đang được xử lý"));
        }
        r.insert(mission_id, tx);
    }
    let core = core.clone();
    tauri::async_runtime::spawn(async move {
        let res = async {
            if do_plan {
                plan(&core, mission_id, rx.clone()).await?;
            }
            if do_exec {
                execute(&core, mission_id, rx.clone()).await?;
            }
            Ok::<(), anyhow::Error>(())
        }
        .await;
        if let Err(e) = res {
            let cancelled = *rx.borrow();
            if e.to_string().starts_with(ASK) {
                set_mission(&core, mission_id, "cho_tra_loi");
                core.log("hoi", "Hệ thống", &format!("Nhiệm vụ #{mission_id} tạm dừng: chờ quản lý trả lời câu hỏi"), Some(mission_id), None);
            } else {
                set_mission(&core, mission_id, if cancelled { "da_huy" } else { "loi" });
            }
            if !e.to_string().starts_with(ASK) && !cancelled {
                let title = db::get_mission(&core.db(), mission_id).ok().flatten().map(|m| m.title).unwrap_or_default();
                core.emit(json!({"type": "mission_failed", "mission_id": mission_id, "title": title, "error": e.to_string()}));
            }
            if e.to_string().starts_with(ASK) {
            } else if !cancelled {
                core.log("loi", "Hệ thống", &format!("Nhiệm vụ #{mission_id}: {e}"), Some(mission_id), None);
            } else {
                core.log("huy", "Hệ thống", &format!("Đã huỷ nhiệm vụ #{mission_id}"), Some(mission_id), None);
            }
        }
        core.0.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&mission_id);
        core.emit(json!({"type": "mission", "id": mission_id, "done": true}));
    });
    Ok(())
}

pub fn cancel(core: &Core, mission_id: i64) -> bool {
    let r = core.0.running.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(tx) = r.get(&mission_id) {
        let _ = tx.send(true);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn test_core() -> (Core, Arc<Mutex<Vec<Value>>>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let core = Core::new(
            db::open_memory().unwrap(),
            dir.path().to_path_buf(),
            "/nonexistent".into(),
            Box::new(move |v| ev.lock().unwrap().push(v)),
        );
        seed(&core).unwrap();
        (core, events, dir)
    }

    #[test]
    fn json_extraction() {
        assert_eq!(extract_json("```json\n{\"a\":1}\n```").unwrap()["a"], 1);
        assert_eq!(extract_json("Đây là kết quả: {\"a\":2} xong").unwrap()["a"], 2);
        assert!(extract_json("[1,2]").unwrap().is_array());
        assert!(extract_json("không có json").is_none());
    }

    #[test]
    fn seed_creates_dispatcher_and_mock_default() {
        let (core, _, _d) = test_core();
        let c = core.db();
        let d = db::dispatcher(&c).unwrap().unwrap();
        assert_eq!(d.provider_id, "mock");
        assert_eq!(d.desk, -1);
        assert!(db::list_providers(&c).unwrap().len() >= 8);
        assert_eq!(db::list_agents(&c).unwrap().len(), 2);
    }

    #[test]
    fn heuristic_picks_matching_agent() {
        let (core, _, _d) = test_core();
        let ids = hire(&core, &templates::offline_candidates("lập trình phần mềm và phân tích dữ liệu", 3), "test").unwrap();
        assert_eq!(ids.len(), 3);
        let agents: Vec<Agent> = db::list_agents(&core.db()).unwrap().into_iter().filter(|a| !a.is_dispatcher).collect();
        let plan = heuristic_plan("Viết API", "Lập trình một API web bằng Rust", &agents);
        let first = agents.iter().find(|a| a.id == plan[0].0).unwrap();
        assert_eq!(first.title, "Lập trình viên Full-stack");
        // bàn làm việc không trùng nhau
        let mut desks: Vec<i64> = agents.iter().map(|a| a.desk).collect();
        desks.dedup();
        assert_eq!(desks.len(), agents.len());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn full_mission_with_mock_ai() {
        let (core, events, _d) = test_core();
        hire(&core, &templates::offline_candidates("soạn thảo báo cáo, phân tích dữ liệu", 2), "test").unwrap();
        let mid = db::insert_mission(&core.db(), "Báo cáo quý", "Soạn báo cáo và phân tích số liệu quý III", "cao", "auto", 1, None, "").unwrap();
        let (_tx, rx) = watch::channel(false);
        plan(&core, mid, rx.clone()).await.unwrap();
        let subs = db::list_subtasks(&core.db(), mid).unwrap();
        assert!(!subs.is_empty());
        assert_eq!(db::get_mission(&core.db(), mid).unwrap().unwrap().status, "cho_duyet");
        execute(&core, mid, rx).await.unwrap();
        let m = db::get_mission(&core.db(), mid).unwrap().unwrap();
        assert_eq!(m.status, "hoan_thanh");
        assert!(!m.summary.is_empty());
        assert!(db::list_subtasks(&core.db(), mid).unwrap().iter().all(|s| s.status == "xong" && !s.output.is_empty()));
        let ev = events.lock().unwrap();
        assert!(ev.iter().any(|e| e["type"] == "assign"));
        assert!(ev.iter().any(|e| e["type"] == "agent" && e["status"] == "working"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn failing_provider_marks_error_then_retry_succeeds() {
        let (core, _, _d) = test_core();
        // Kết nối CLI hỏng
        {
            let c = core.db();
            let mut p = db::get_provider(&c, "claude-cli").unwrap().unwrap();
            p.command = "/bin/false".into();
            p.args = "{prompt}".into();
            db::upsert_provider(&c, &p, true).unwrap();
        }
        let agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        let mut a2 = agent.clone();
        a2.provider_id = "claude-cli".into();
        db::update_agent(&core.db(), &a2).unwrap();
        let mid = db::insert_mission(&core.db(), "Thư mời", "Soạn thư mời họp", "trung_binh", "manual", 1, None, "").unwrap();
        db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Soạn thư", "Soạn thư mời").unwrap();
        let (_tx, rx) = watch::channel(false);
        assert!(execute(&core, mid, rx.clone()).await.is_err());
        let s = &db::list_subtasks(&core.db(), mid).unwrap()[0];
        assert_eq!(s.status, "loi");
        // Chuyển sang mô phỏng rồi chạy lại phần lỗi
        a2.provider_id = "mock".into();
        db::update_agent(&core.db(), &a2).unwrap();
        execute(&core, mid, rx).await.unwrap();
        assert_eq!(db::list_subtasks(&core.db(), mid).unwrap()[0].status, "xong");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cli_runs_inside_mission_workdir_with_edit_args() {
        let (core, events, _d) = test_core();
        let proj = tempfile::tempdir().unwrap();
        std::fs::write(proj.path().join("du-lieu.csv"), "a,b\n1,2").unwrap();
        {
            // CLI giả: in thư mục hiện tại + danh sách tệp + các tham số nhận được
            let c = core.db();
            let mut p = db::get_provider(&c, "claude-cli").unwrap().unwrap();
            p.command = "/bin/sh".into();
            p.args = "-c 'pwd; ls; echo \"$@\"' sh {prompt}".into();
            db::upsert_provider(&c, &p, true).unwrap();
        }
        let agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        let mut a2 = agent.clone();
        a2.provider_id = "claude-cli".into();
        db::update_agent(&core.db(), &a2).unwrap();
        let mid = db::insert_mission(&core.db(), "Sửa số liệu", "Đọc du-lieu.csv", "cao", "manual", 1, None, "").unwrap();
        db::set_mission_files(&core.db(), mid, &proj.path().to_string_lossy(), &[], true).unwrap();
        db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Xử lý", "Đọc tệp").unwrap();
        let (_tx, rx) = watch::channel(false);
        execute(&core, mid, rx).await.unwrap();
        let out = db::list_subtasks(&core.db(), mid).unwrap()[0].output.clone();
        let real = std::fs::canonicalize(proj.path()).unwrap();
        assert!(out.contains(&real.to_string_lossy().to_string()) || out.contains(&proj.path().to_string_lossy().to_string()), "{out}");
        assert!(out.contains("du-lieu.csv"));
        assert!(out.contains("acceptEdits"), "{out}");
        assert!(out.contains("ĐƯỢC PHÉP"));
        drop(events);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn later_step_emits_exchange_event() {
        let (core, events, _d) = test_core();
        hire(&core, &templates::offline_candidates("phân tích dữ liệu", 1), "test").unwrap();
        let staff: Vec<Agent> = db::list_agents(&core.db()).unwrap().into_iter().filter(|a| !a.is_dispatcher).collect();
        let mid = db::insert_mission(&core.db(), "Báo cáo", "x", "cao", "manual", 1, None, "").unwrap();
        db::insert_subtask(&core.db(), mid, Some(staff[0].id), 1, "Số liệu", "a").unwrap();
        db::insert_subtask(&core.db(), mid, Some(staff[1].id), 2, "Viết báo cáo", "b").unwrap();
        let (_tx, rx) = watch::channel(false);
        execute(&core, mid, rx).await.unwrap();
        let ev = events.lock().unwrap();
        assert!(ev.iter().any(|e| e["type"] == "exchange" && e["from"] == staff[1].id && e["to"] == staff[0].id));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn subtask_override_uses_other_vendor_and_effort() {
        let (core, _ev, _d) = test_core();
        {
            // hai "hãng" giả: mỗi CLI in ra tên hãng + tham số nhận được
            let c = core.db();
            for (id, tag) in [("claude-cli", "HANG_CLAUDE"), ("gemini-cli", "HANG_GEMINI")] {
                let mut p = db::get_provider(&c, id).unwrap().unwrap();
                p.command = "/bin/sh".into();
                p.args = format!("-c 'echo {tag} \"$@\"' sh {{model_args}} {{prompt}}");
                db::upsert_provider(&c, &p, true).unwrap();
            }
        }
        let mut agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        agent.provider_id = "claude-cli".into();
        agent.model = "sonnet".into();
        agent.effort = "low".into();
        db::update_agent(&core.db(), &agent).unwrap();
        let mid = db::insert_mission(&core.db(), "Chuỗi", "x", "cao", "manual", 1, None, "").unwrap();
        let s1 = db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Viết", "a").unwrap();
        let s2 = db::insert_subtask(&core.db(), mid, Some(agent.id), 2, "Tra cứu", "b").unwrap();
        db::set_subtask_ai(&core.db(), s2, "gemini-cli", "gemini-2.5-pro", "high", true).unwrap();
        let (_tx, rx) = watch::channel(false);
        execute(&core, mid, rx).await.unwrap();
        let subs = db::list_subtasks(&core.db(), mid).unwrap();
        let o1 = &subs.iter().find(|s| s.id == s1).unwrap().output;
        let o2 = &subs.iter().find(|s| s.id == s2).unwrap().output;
        assert!(o1.contains("HANG_CLAUDE") && o1.contains("--model sonnet") && o1.contains("--effort low"), "{o1}");
        assert!(o2.contains("HANG_GEMINI") && o2.contains("-m gemini-2.5-pro"), "{o2}");
        assert!(o2.contains("CAO") && o2.contains("tìm kiếm web"), "{o2}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pipeline_creates_multi_vendor_steps() {
        let (core, _ev, _d) = test_core();
        hire(&core, &templates::offline_candidates("nghiên cứu, soạn thảo báo cáo, kiểm thử chất lượng", 3), "test").unwrap();
        let pls = db::list_pipelines(&core.db()).unwrap();
        assert_eq!(pls.len(), 3);
        let mid = db::insert_mission(&core.db(), "Báo cáo thị trường", "x", "cao", "manual", 1, None, "").unwrap();
        let note = apply_pipeline(&core, mid, pls[0].id).await.unwrap();
        let subs = db::list_subtasks(&core.db(), mid).unwrap();
        assert_eq!(subs.len(), 3);
        assert_eq!(subs.iter().map(|s| s.step).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(subs[0].web && subs[0].effort == "high");
        assert!(note.contains("Nghiên cứu"));
        // nhiều người khác nhau đảm nhận các bước
        let mut who: Vec<_> = subs.iter().filter_map(|s| s.agent_id).collect();
        who.dedup();
        assert!(who.len() >= 2);
        // trong môi trường thử, CLI không có → bước dùng AI của nhân viên và có ghi chú
        assert!(subs.iter().all(|s| s.provider_id.is_empty()));
        assert!(note.contains("chưa sẵn sàng"));
        execute(&core, mid, watch::channel(false).1).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn agent_asks_then_continues_after_answer() {
        let (core, events, _d) = test_core();
        let agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        let mid = db::insert_mission(&core.db(), "Viết báo cáo", "Viết báo cáo (cần hỏi)", "cao", "manual", 1, None, "").unwrap();
        let sid = db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Soạn", "Soạn báo cáo (cần hỏi)").unwrap();
        let err = execute(&core, mid, watch::channel(false).1).await.unwrap_err();
        assert!(err.to_string().starts_with(ASK));
        let s = db::get_subtask(&core.db(), sid).unwrap().unwrap();
        assert_eq!(s.status, "hoi");
        assert!(s.question.contains("đối tượng nào"));
        assert!(!s.log.is_empty(), "nhật ký tiến trình phải được lưu");
        assert_eq!(questions(&core).unwrap().len(), 1);
        assert!(events.lock().unwrap().iter().any(|e| e["type"] == "question"));
        assert!(events.lock().unwrap().iter().any(|e| e["type"] == "progress"));
        // trả lời rồi chạy tiếp (gọi execute trực tiếp thay cho chạy nền)
        db::append_instruction(&core.db(), sid, "\n\n[Quản lý trả lời câu hỏi “x”]: Ban giám đốc, 2 trang").unwrap();
        db::set_subtask_question(&core.db(), sid, "").unwrap();
        execute(&core, mid, watch::channel(false).1).await.unwrap();
        assert_eq!(db::get_subtask(&core.db(), sid).unwrap().unwrap().status, "xong");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ai_file_blocks_become_real_files() {
        let (core, _ev, _d) = test_core();
        {
            // "AI" giả trả về một khối tệp Word
            let c = core.db();
            let mut p = db::get_provider(&c, "claude-cli").unwrap().unwrap();
            p.command = "/bin/sh".into();
            p.args = "-c 'printf \"Đã soạn xong.\\n<<<TỆP: Bao cao.docx>>>\\n# Báo cáo\\n\\n| A | B |\\n|---|---|\\n| 1 | 2 |\\n<<<HẾT TỆP>>>\\n\"' sh {prompt}".into();
            db::upsert_provider(&c, &p, true).unwrap();
        }
        let mut agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        agent.provider_id = "claude-cli".into();
        db::update_agent(&core.db(), &agent).unwrap();
        let mid = db::insert_mission(&core.db(), "Xuất báo cáo", "x", "cao", "manual", 1, None, "").unwrap();
        let sid = db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Xuất Word", "Xuất file word").unwrap();
        execute(&core, mid, watch::channel(false).1).await.unwrap();
        let s = db::get_subtask(&core.db(), sid).unwrap().unwrap();
        assert_eq!(s.files.len(), 1, "{:?}", s.files);
        assert_eq!(s.files[0].kind, "xuat");
        assert!(s.files[0].name.ends_with(".docx"));
        assert!(std::path::Path::new(&s.files[0].path).exists());
        assert!(s.files[0].path.contains("ket-qua"));
        assert!(s.output.contains("📄 Đã tạo tệp") && !s.output.contains("<<<"), "{}", s.output);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn memory_and_template_flow() {
        let (core, events, _d) = test_core();
        {
            // "AI" giả: in lại lời nhắc (để kiểm tra trí nhớ + mẫu đã được đưa vào) rồi ghi nhớ một bài học
            let c = core.db();
            let mut p = db::get_provider(&c, "claude-cli").unwrap().unwrap();
            p.command = "/bin/sh".into();
            p.args = "-c 'printf \"%s\\n[[GHI NHỚ]] Quản lý muốn báo cáo tối đa 1 trang\\n\" \"$1\"' sh {prompt}".into();
            db::upsert_provider(&c, &p, true).unwrap();
            crate::memory::add(&c, None, "Đơn vị tiền tệ là triệu đồng", "quan_ly", None).unwrap();
            let t = crate::memory::DocTemplate { name: "Báo cáo tuần".into(), keywords: "báo cáo tuần".into(), content: "# BÁO CÁO TUẦN\n## 1. Việc đã làm".into(), format: "docx".into(), auto: true, ..Default::default() };
            crate::memory::save_template(&c, &t).unwrap();
        }
        let mut agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        agent.provider_id = "claude-cli".into();
        db::update_agent(&core.db(), &agent).unwrap();
        let mid = db::insert_mission(&core.db(), "Viết báo cáo tuần", "Báo cáo tuần phòng kế hoạch", "cao", "manual", 1, None, "").unwrap();
        let tpl = crate::memory::match_template(&core.db(), "Viết báo cáo tuần phòng kế hoạch").unwrap().unwrap();
        db::set_mission_template(&core.db(), mid, Some(tpl.id)).unwrap();
        let sid = db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Soạn", "Soạn báo cáo").unwrap();
        execute(&core, mid, watch::channel(false).1).await.unwrap();
        let s = db::get_subtask(&core.db(), sid).unwrap().unwrap();
        // lời nhắc có trí nhớ chung + mẫu bắt buộc; dòng ghi nhớ bị tách khỏi kết quả
        assert!(s.output.contains("triệu đồng") && s.output.contains("MẪU KẾT QUẢ BẮT BUỘC") && s.output.contains("BÁO CÁO TUẦN"), "{}", &s.output[..400.min(s.output.len())]);
        assert!(!s.output.lines().any(|l| l.trim_start().starts_with("[[GHI NHỚ]]")), "dòng ghi nhớ phải được tách khỏi kết quả");
        let mems = crate::memory::list(&core.db(), Some(agent.id)).unwrap();
        assert!(mems.iter().any(|m| m.content.contains("tối đa 1 trang") && m.agent_id == Some(agent.id)));
        // lần sau: trí nhớ mới được đưa vào yêu cầu
        assert!(crate::memory::prompt_section(&core.db(), agent.id).unwrap().contains("tối đa 1 trang"));
        // xong → tự xuất .docx theo mẫu và phát sự kiện hoàn thành có người trình báo cáo
        let ev = events.lock().unwrap();
        let done = ev.iter().find(|e| e["type"] == "mission_done").expect("phải có sự kiện hoàn thành");
        assert_eq!(done["presenter"], agent.id);
        assert!(done["exported"].as_str().unwrap().ends_with(".docx"));
        assert!(std::path::Path::new(done["exported"].as_str().unwrap()).exists());
    }

    #[test]
    fn followup_command_path_does_not_deadlock() {
        // Gọi đúng hàm mà nút "Gửi yêu cầu tiếp" dùng; nếu khoá chết thì luồng sẽ không trả về.
        let (core, _ev, _d) = test_core();
        let agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        let mid = db::insert_mission(&core.db(), "Thư", "Soạn thư", "cao", "manual", 1, None, "").unwrap();
        db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Soạn", "Soạn").unwrap();
        tauri::async_runtime::block_on(execute(&core, mid, watch::channel(false).1)).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let c2 = core.clone();
        std::thread::spawn(move || {
            let _ = tx.send(followup(&c2, mid, "Rút gọn còn nửa trang", None, "test"));
        });
        let sid = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("followup bị treo (khoá chết CSDL)")
            .unwrap();
        // chờ chạy nền xong
        let t0 = std::time::Instant::now();
        loop {
            let st = db::get_mission(&core.db(), mid).unwrap().unwrap().status;
            if st == "hoan_thanh" && !core.is_running(mid) {
                break;
            }
            assert!(t0.elapsed() < std::time::Duration::from_secs(20), "nhắn tiếp không chạy xong: {st}");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let s = db::get_subtask(&core.db(), sid).unwrap().unwrap();
        assert_eq!(s.status, "xong");
        assert!(s.title.starts_with(FOLLOW));
        assert_eq!(db::get_mission(&core.db(), mid).unwrap().unwrap().summary, s.output);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn followup_adds_next_step_and_updates_summary() {
        let (core, _ev, _d) = test_core();
        let agent = db::list_agents(&core.db()).unwrap().into_iter().find(|a| !a.is_dispatcher).unwrap();
        let mid = db::insert_mission(&core.db(), "Thư mời", "Soạn thư mời", "cao", "manual", 1, None, "").unwrap();
        db::insert_subtask(&core.db(), mid, Some(agent.id), 1, "Soạn thư", "Soạn").unwrap();
        execute(&core, mid, watch::channel(false).1).await.unwrap();
        // nhắn tiếp (tạo bước 2) — chạy trực tiếp để kiểm tra
        let sid = {
            let c = core.db();
            db::insert_subtask(&c, mid, Some(agent.id), 2, &format!("{FOLLOW}Rút gọn còn nửa trang"), "Rút gọn").unwrap()
        };
        execute(&core, mid, watch::channel(false).1).await.unwrap();
        let s = db::get_subtask(&core.db(), sid).unwrap().unwrap();
        assert_eq!(s.status, "xong");
        let m = db::get_mission(&core.db(), mid).unwrap().unwrap();
        assert_eq!(m.summary, s.output);
        let w = agent_work(&core, agent.id).unwrap();
        assert!(w["recent"].as_array().unwrap().len() >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recruit_with_mock_uses_offline_selector() {
        let (core, _, _d) = test_core();
        let r = recruit(&core, "Cần người viết nội dung marketing và dịch tiếng Anh", 3, None, true).await.unwrap();
        assert_eq!(r.candidates.len(), 3);
        assert!(r.candidates.iter().all(|c| templates::valid_avatar(&c.avatar)));
    }
}
