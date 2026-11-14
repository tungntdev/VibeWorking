//! Chọn AI và model phù hợp nhất cho từng vị trí khi văn phòng có nhiều kết nối AI.
//!
//! Mỗi "họ" AI có hồ sơ thế mạnh theo nhóm công việc và một hạng năng lực. Vị trí được gắn nhãn
//! nhóm công việc từ chức danh/kỹ năng/phòng ban. Điểm = mức khớp thế mạnh + độ vừa với cấp độ
//! (vị trí cơ bản ưu tiên AI rẻ/nhanh, chuyên gia ưu tiên AI mạnh) − phạt nhẹ để chia đều tải.

use crate::db::Provider;
use crate::templates::fold;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub provider_id: String,
    pub model: String,
    pub effort: String,
    pub reason: String,
}

/// Mức suy luận gợi ý theo cấp độ vị trí.
pub fn effort_for(level: &str) -> &'static str {
    match level {
        "co_ban" => "low",
        "trung_cap" => "medium",
        "chuyen_nghiep" => "high",
        _ => "xhigh",
    }
}

const TAGS: &[(&str, &[&str])] = &[
    ("coding", &["lap trinh", "code", "phan mem", "developer", "full-stack", "fullstack", "rust", "python", "javascript", "typescript", "api", "web", "ung dung"]),
    ("testing", &["kiem thu", "test", "qa", "chat luong"]),
    ("devops", &["devops", "trien khai", "may chu", "server", "ha tang", "docker", "ci/cd", "sao luu"]),
    ("security", &["bao mat", "an toan thong tin", "an ninh", "security", "lo hong"]),
    ("data", &["du lieu", "phan tich", "thong ke", "sql", "data", "ke toan", "tai chinh", "du toan", "kpi", "so lieu"]),
    ("writing", &["soan thao", "van ban", "bao cao", "noi dung", "content", "email", "bien ban", "to trinh", "viet bai", "soan"]),
    ("translation", &["dich", "ngoai ngu", "translate", "tieng anh"]),
    ("research", &["nghien cuu", "tong quan", "khoa hoc", "tai lieu", "bai bao"]),
    ("marketing", &["marketing", "truyen thong", "seo", "mang xa hoi", "quang cao", "video"]),
    ("design", &["thiet ke", "ui", "ux", "giao dien", "wireframe"]),
    ("legal", &["phap che", "phap ly", "hop dong", "luat", "tuan thu"]),
    ("medical", &["y te", "benh vien", "kham", "dieu duong", "chuyen mon"]),
    ("admin", &["thu ky", "hanh chinh", "nhap lieu", "khach hang", "cskh", "lich", "faq", "tu van"]),
    ("reasoning", &["dieu phoi", "phan cong", "du an", "kien truc", "chien luoc", "giai phap", "ke hoach"]),
];

const TAG_VI: &[(&str, &str)] = &[
    ("coding", "lập trình"), ("testing", "kiểm thử"), ("devops", "hạ tầng"), ("security", "bảo mật"),
    ("data", "phân tích số liệu"), ("writing", "soạn thảo"), ("translation", "dịch thuật"), ("research", "nghiên cứu"),
    ("marketing", "truyền thông"), ("design", "thiết kế"), ("legal", "pháp lý"), ("medical", "y tế"),
    ("admin", "hành chính"), ("reasoning", "lập luận, điều phối"),
];

fn tag_vi(t: &str) -> &str {
    TAG_VI.iter().find(|(k, _)| *k == t).map(|(_, v)| *v).unwrap_or(t)
}

/// Gắn nhãn nhóm công việc cho một vị trí.
pub fn tags_for(title: &str, skills: &[String], department: &str) -> Vec<&'static str> {
    let raw = fold(&format!("{title} {} {department}", skills.join(" ")));
    // so khớp theo từ nguyên vẹn (tránh "Việt" khớp nhầm "viết", "ui" khớp trong từ khác)
    let text: String = raw.chars().map(|c| if c.is_alphanumeric() || c == '/' || c == '-' { c } else { ' ' }).collect();
    let text = format!(" {} ", text.split_whitespace().collect::<Vec<_>>().join(" "));
    let mut out: Vec<&'static str> = TAGS
        .iter()
        .filter(|(_, kws)| kws.iter().any(|k| text.contains(&format!(" {k} "))))
        .map(|(t, _)| *t)
        .collect();
    if out.is_empty() {
        out.push("writing");
    }
    out
}

/// Họ AI của một kết nối.
pub fn family(p: &Provider) -> &'static str {
    let local = p.base_url.contains("localhost") || p.base_url.contains("127.0.0.1");
    match (p.id.as_str(), p.kind.as_str()) {
        ("claude-cli", _) | (_, "anthropic") => "claude",
        ("codex-cli", _) => "openai",
        ("gemini-cli", _) | (_, "gemini") => "gemini",
        ("qwen-cli", _) => "qwen",
        (_, "openai") if local => "local",
        (_, "openai") if p.base_url.contains("api.openai.com") => "openai",
        (_, "openai") => "compat",
        (_, "mock") => "mock",
        _ => "custom",
    }
}

fn family_label(f: &str) -> &'static str {
    match f {
        "claude" => "Claude",
        "openai" => "OpenAI",
        "gemini" => "Gemini",
        "qwen" => "Qwen",
        "local" => "AI cục bộ",
        "mock" => "Mô phỏng",
        _ => "AI tuỳ chỉnh",
    }
}

/// Mức thế mạnh 0–3 của một họ AI với một nhóm công việc.
fn strength(fam: &str, tag: &str) -> i32 {
    let table: &[(&str, i32)] = match fam {
        "claude" => &[("coding", 3), ("writing", 3), ("data", 3), ("reasoning", 3), ("legal", 3), ("security", 3), ("research", 2), ("translation", 2), ("design", 2), ("admin", 2), ("devops", 2), ("testing", 2), ("medical", 2), ("marketing", 2)],
        "openai" => &[("coding", 3), ("devops", 3), ("testing", 3), ("data", 3), ("reasoning", 3), ("security", 2), ("writing", 2), ("research", 2), ("design", 2), ("admin", 2), ("legal", 2), ("marketing", 2), ("translation", 2), ("medical", 2)],
        "gemini" => &[("research", 3), ("translation", 3), ("marketing", 3), ("data", 2), ("writing", 2), ("coding", 2), ("admin", 2), ("design", 2), ("medical", 2), ("reasoning", 2), ("testing", 1), ("devops", 1), ("legal", 1), ("security", 1)],
        "qwen" => &[("coding", 2), ("translation", 2), ("testing", 2), ("data", 1), ("writing", 1), ("admin", 1)],
        "local" => &[("admin", 2), ("writing", 1), ("translation", 1), ("data", 1)],
        "compat" | "custom" => &[("coding", 1), ("writing", 1), ("data", 1), ("admin", 1), ("translation", 1), ("research", 1)],
        _ => &[],
    };
    table.iter().find(|(t, _)| *t == tag).map(|(_, s)| *s).unwrap_or(0)
}

fn tier(fam: &str) -> i32 {
    match fam {
        "claude" | "openai" | "gemini" => 3,
        "qwen" | "compat" | "custom" => 2,
        "local" => 1,
        _ => 0,
    }
}

fn need(level: &str) -> i32 {
    match level {
        "co_ban" => 1,
        "trung_cap" => 2,
        _ => 3,
    }
}

/// Model gợi ý theo họ AI và cấp độ (rỗng = dùng model mặc định của kết nối).
pub fn model_for(p: &Provider, level: &str) -> String {
    match (family(p), p.kind.as_str()) {
        // Opus chỉ dành cho cấp Chuyên gia; Chuyên nghiệp/Trung cấp dùng Sonnet; Cơ bản dùng Haiku
        ("claude", "cli") => match level { "chuyen_gia" => "opus", "co_ban" => "haiku", _ => "sonnet" }.into(),
        ("claude", "anthropic") => match level { "chuyen_gia" => "claude-opus-5-5", "co_ban" => "claude-haiku-5-5", _ => "claude-sonnet-5-5" }.into(),
        ("gemini", _) => match level { "co_ban" => "gemini-3.1-flash-lite", "trung_cap" => "gemini-3.5-flash", _ => "gemini-2.5-pro" }.into(),
        _ => String::new(),
    }
}

/// Chọn kết nối + model tốt nhất cho một vị trí. `load` đếm số vị trí đã giao cho mỗi kết nối
/// trong cùng lượt để chia đều khi điểm sát nhau.
pub fn choose(avail: &[Provider], title: &str, skills: &[String], department: &str, level: &str, load: &mut HashMap<String, i32>) -> Option<Choice> {
    let tags = tags_for(title, skills, department);
    let nd = need(level);
    let mut best: Option<(i32, &Provider)> = None;
    for p in avail {
        let fam = family(p);
        let fit: i32 = tags.iter().map(|t| strength(fam, t)).sum::<i32>() * 20 / tags.len() as i32;
        let tr = tier(fam);
        // dùng AI quá mạnh cho việc đơn giản bị trừ điểm (tốn chi phí); AI yếu cho việc khó bị trừ nặng
        let lvl = if tr >= nd { 30 - (tr - nd) * 12 } else { 30 - (nd - tr) * 25 };
        let score = fit + lvl - load.get(&p.id).copied().unwrap_or(0) * 6;
        if best.map_or(true, |(b, _)| score > b) {
            best = Some((score, p));
        }
    }
    let (_, p) = best?;
    *load.entry(p.id.clone()).or_default() += 1;
    let model = model_for(p, level);
    let fam = family(p);
    let strong: Vec<&str> = tags.iter().filter(|t| strength(fam, t) >= 3).map(|t| tag_vi(t)).collect();
    let why_fit = if strong.is_empty() {
        format!("phù hợp nhóm việc {}", tags.iter().map(|t| tag_vi(t)).collect::<Vec<_>>().join(", "))
    } else {
        format!("{} mạnh về {}", family_label(fam), strong.join(", "))
    };
    let why_lv = match (nd, tier(fam)) {
        (1, t) if t <= 1 => "chạy cục bộ, tiết kiệm cho việc cơ bản",
        (1, _) => "model nhẹ, nhanh và rẻ cho việc cơ bản",
        (2, _) => "cân đối chất lượng và chi phí",
        _ => "dùng model mạnh nhất cho việc chuyên sâu",
    };
    let effort = effort_for(level).to_string();
    Some(Choice {
        provider_id: p.id.clone(),
        model: model.clone(),
        reason: format!(
            "{}{} · suy luận {} — {}; {}.",
            p.name,
            if model.is_empty() { String::new() } else { format!(" · {model}") },
            crate::providers::effort_label(&effort),
            why_fit,
            why_lv
        ),
        effort,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::builtin_providers;

    fn pick(ids: &[&str]) -> Vec<Provider> {
        builtin_providers().into_iter().filter(|p| ids.contains(&p.id.as_str())).collect()
    }

    #[test]
    fn tags_detected() {
        let t = tags_for("Lập trình viên Full-stack", &["Rust".into()], "Công nghệ");
        assert!(t.contains(&"coding"));
        assert_eq!(tags_for("Vị trí lạ", &[], ""), vec!["writing"]);
    }

    #[test]
    fn expert_coder_gets_strong_model() {
        let avail = pick(&["claude-cli", "gemini-cli", "ollama"]);
        let c = choose(&avail, "Kiến trúc sư giải pháp", &["Kiến trúc hệ thống".into()], "Công nghệ", "chuyen_gia", &mut HashMap::new()).unwrap();
        assert_eq!(c.provider_id, "claude-cli");
        assert_eq!(c.model, "opus");
    }

    #[test]
    fn translator_prefers_gemini() {
        let avail = pick(&["claude-cli", "gemini-cli"]);
        let c = choose(&avail, "Biên dịch viên", &["Dịch Anh–Việt".into()], "Văn phòng", "trung_cap", &mut HashMap::new()).unwrap();
        assert_eq!(c.provider_id, "gemini-cli");
        assert_eq!(c.model, "gemini-3.5-flash");
        assert_eq!(c.effort, "medium");
    }

    #[test]
    fn basic_admin_prefers_local_when_available() {
        let avail = pick(&["claude-cli", "ollama"]);
        let c = choose(&avail, "Thư ký hành chính", &["Soạn email".into()], "Hành chính", "co_ban", &mut HashMap::new()).unwrap();
        assert_eq!(c.provider_id, "ollama");
    }

    #[test]
    fn only_one_ai_uses_level_models() {
        let avail = pick(&["claude-cli"]);
        let mut load = HashMap::new();
        assert_eq!(choose(&avail, "Thư ký", &[], "", "co_ban", &mut load).unwrap().model, "haiku");
        assert_eq!(choose(&avail, "Chuyên viên", &[], "", "trung_cap", &mut load).unwrap().model, "sonnet");
    }

    #[test]
    fn load_spreads_ties() {
        let avail = pick(&["claude-cli", "codex-cli"]);
        let mut load = HashMap::new();
        let a = choose(&avail, "Lập trình viên", &["Python".into()], "", "chuyen_nghiep", &mut load).unwrap();
        let b = choose(&avail, "Lập trình viên", &["Python".into()], "", "chuyen_nghiep", &mut load).unwrap();
        assert_ne!(a.provider_id, b.provider_id);
    }

    #[test]
    fn nothing_available() {
        assert!(choose(&[], "x", &[], "", "co_ban", &mut HashMap::new()).is_none());
    }
}
