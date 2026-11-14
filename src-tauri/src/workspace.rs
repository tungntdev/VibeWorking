//! Thư mục làm việc và tệp đính kèm của nhiệm vụ (giống "project"/cowork):
//! - Có thư mục làm việc → AI dạng CLI chạy ngay trong thư mục đó (đọc, và nếu được phép thì sửa/tạo tệp).
//! - Tệp đính kèm được chép vào thư mục riêng của nhiệm vụ; nội dung văn bản được trích vào yêu cầu
//!   để cả AI dạng API (không truy cập được ổ đĩa) cũng đọc được.

use crate::db::Mission;
use crate::state::Core;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

const MAX_TEXT_FILE: u64 = 150 * 1024;
const MAX_TEXT_TOTAL: usize = 400 * 1024;
const TREE_LIMIT: usize = 220;
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", "__pycache__", ".venv", "venv", ".next", ".idea", ".vscode", "DerivedData"];

#[derive(Debug, Clone, Default)]
pub struct Ctx {
    /// Thư mục chạy CLI (None = thư mục riêng của nhân viên).
    pub cwd: Option<PathBuf>,
    /// Thư mục cấp thêm quyền đọc (bản sao tệp đính kèm khi đã có thư mục làm việc).
    pub dirs: Vec<String>,
    pub allow_edit: bool,
    /// Phần mô tả tài liệu chung cho mọi AI.
    pub note: String,
    /// Thêm cho AI dạng CLI (được truy cập ổ đĩa).
    pub note_cli: String,
    /// Thêm cho AI dạng API (không truy cập ổ đĩa).
    pub note_api: String,
}

impl Ctx {
    pub fn is_empty(&self) -> bool {
        self.note.is_empty()
    }
    pub fn for_kind(&self, kind: &str) -> String {
        if self.note.is_empty() {
            return String::new();
        }
        format!("{}\n{}", self.note, if kind == "cli" { &self.note_cli } else { &self.note_api })
    }
}

pub fn mission_dir(core: &Core, id: i64) -> PathBuf {
    core.0.data_dir.join("missions").join(format!("nhiem-vu-{id}"))
}

fn human(n: u64) -> String {
    if n >= 1 << 20 { format!("{:.1} MB", n as f64 / 1048576.0) } else if n >= 1024 { format!("{:.0} KB", n as f64 / 1024.0) } else { format!("{n} B") }
}

/// Đọc tệp nếu là văn bản UTF-8 và không quá lớn.
pub fn read_text(p: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(p).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    let bytes = std::fs::read(p).ok()?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Cây thư mục rút gọn (bỏ thư mục ẩn, node_modules, target…).
pub fn tree(root: &Path, depth: usize) -> (String, usize) {
    fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>, count: &mut usize) {
        if depth == 0 || *count >= TREE_LIMIT {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut items: Vec<_> = rd.flatten().collect();
        items.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));
        for e in items {
            if *count >= TREE_LIMIT {
                out.push(format!("{prefix}…"));
                return;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            *count += 1;
            let p = e.path();
            if p.is_dir() {
                out.push(format!("{prefix}{name}/"));
                walk(&p, &format!("{prefix}  "), depth - 1, out, count);
            } else {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push(format!("{prefix}{name} ({})", human(size)));
            }
        }
    }
    let mut out = vec![];
    let mut count = 0;
    walk(root, "", depth, &mut out, &mut count);
    (out.join("\n"), count)
}

/// Kiểm tra đường dẫn người dùng gửi lên.
pub fn validate(workdir: &str, attachments: &[String]) -> Result<()> {
    if !workdir.trim().is_empty() && !Path::new(workdir).is_dir() {
        return Err(anyhow!("Thư mục làm việc không tồn tại: {workdir}"));
    }
    for a in attachments {
        if !Path::new(a).exists() {
            return Err(anyhow!("Không tìm thấy tệp: {a}"));
        }
    }
    if attachments.len() > 50 {
        return Err(anyhow!("Tối đa 50 tệp đính kèm"));
    }
    Ok(())
}

fn copy_into(src: &Path, dst_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dst_dir)?;
    let name = src.file_name().ok_or_else(|| anyhow!("tên tệp không hợp lệ"))?;
    let dst = dst_dir.join(name);
    if src.is_dir() {
        // thư mục đính kèm: chép nông (chỉ các tệp ở cấp đầu) để tránh chép nhầm cả ổ đĩa
        std::fs::create_dir_all(&dst)?;
        for e in std::fs::read_dir(src)?.flatten() {
            if e.path().is_file() {
                std::fs::copy(e.path(), dst.join(e.file_name()))?;
            }
        }
    } else {
        std::fs::copy(src, &dst)?;
    }
    Ok(dst)
}

/// Chuẩn bị bối cảnh tệp cho một nhiệm vụ (chép tệp đính kèm, đọc cây thư mục, trích nội dung).
pub fn prepare(core: &Core, m: &Mission) -> Result<Ctx> {
    let mut ctx = Ctx { allow_edit: m.allow_edit, ..Default::default() };
    if m.workdir.trim().is_empty() && m.attachments.is_empty() {
        return Ok(ctx);
    }
    let mdir = mission_dir(core, m.id);
    let attach_dir = mdir.join("tep-dinh-kem");
    let mut note = String::from("\n## TÀI LIỆU CỦA NHIỆM VỤ\n");
    let mut cli = String::new();
    let mut api = String::new();

    if !m.workdir.trim().is_empty() {
        let wd = PathBuf::from(&m.workdir);
        let (t, n) = tree(&wd, 3);
        note.push_str(&format!("Thư mục làm việc: `{}` ({} mục)\n```\n{}\n```\n", wd.display(), n, t));
        ctx.cwd = Some(wd.clone());
        cli.push_str(&format!(
            "Bạn đang chạy NGAY TRONG thư mục `{}`: hãy tự mở và đọc các tệp cần thiết. {}\n",
            wd.display(),
            if m.allow_edit {
                "Bạn ĐƯỢC PHÉP tạo/sửa tệp trong thư mục này; cuối câu trả lời liệt kê rõ các tệp đã tạo hoặc thay đổi."
            } else {
                "Chế độ CHỈ ĐỌC: KHÔNG sửa hay tạo tệp; nếu cần thay đổi, hãy trình bày nội dung đề xuất trong câu trả lời."
            }
        ));
        // AI dạng API không đọc được ổ đĩa: trích vài tệp văn bản nhỏ quan trọng
        let mut total = 0;
        let mut picked = 0;
        let mut cands: Vec<PathBuf> = std::fs::read_dir(&wd).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default();
        cands.sort_by_key(|p| {
            let n = p.file_name().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            (!n.starts_with("readme"), !n.ends_with(".md"), n)
        });
        for p in cands {
            if picked >= 8 || total > MAX_TEXT_TOTAL / 2 {
                break;
            }
            if let Some(t) = read_text(&p, 40 * 1024) {
                total += t.len();
                picked += 1;
                api.push_str(&format!("\n### {}\n```\n{}\n```\n", p.file_name().unwrap().to_string_lossy(), t));
            }
        }
        if picked > 0 {
            api = format!("(Bạn không truy cập trực tiếp được thư mục; dưới đây là nội dung một số tệp chính.)\n{api}");
        }
    } else if !m.attachments.is_empty() {
        ctx.cwd = Some(mdir.clone());
        cli.push_str(&format!(
            "Bạn đang chạy trong thư mục nhiệm vụ `{}`; tệp đính kèm nằm ở `tep-dinh-kem/`. {}\n",
            mdir.display(),
            if m.allow_edit { "Có thể tạo tệp kết quả trong thư mục này." } else { "Chỉ đọc." }
        ));
    }

    if !m.attachments.is_empty() {
        note.push_str("Tệp đính kèm:\n");
        let mut total = 0usize;
        for a in &m.attachments {
            let src = PathBuf::from(a);
            let copied = copy_into(&src, &attach_dir).unwrap_or_else(|_| src.clone());
            let size = std::fs::metadata(&src).map(|x| x.len()).unwrap_or(0);
            let name = src.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| a.clone());
            match read_text(&src, MAX_TEXT_FILE).filter(|t| total + t.len() <= MAX_TEXT_TOTAL) {
                Some(t) => {
                    total += t.len();
                    note.push_str(&format!("\n### {name} ({})\n```\n{t}\n```\n", human(size)));
                }
                None => note.push_str(&format!(
                    "- {name} ({}{}) — bản sao tại `{}`\n",
                    human(size),
                    if src.is_dir() { ", thư mục" } else { ", không trích được nội dung văn bản" },
                    copied.display()
                )),
            }
        }
        if !m.workdir.trim().is_empty() {
            ctx.dirs.push(attach_dir.to_string_lossy().to_string());
        }
    }
    ctx.note = note;
    ctx.note_cli = cli;
    ctx.note_api = api;
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn mission(workdir: &str, att: Vec<String>, edit: bool) -> Mission {
        Mission {
            id: 7, title: "t".into(), description: "d".into(), priority: "cao".into(), mode: "auto".into(), status: "moi".into(),
            created_by: None, coordinator_id: None, deadline: String::new(), analysis: String::new(), summary: String::new(),
            created_at: String::new(), updated_at: String::new(), workdir: workdir.into(), attachments: att, allow_edit: edit, template_id: None,
        }
    }

    #[test]
    fn workdir_and_attachments() {
        let data = tempfile::tempdir().unwrap();
        let proj = tempfile::tempdir().unwrap();
        std::fs::write(proj.path().join("README.md"), "# Dự án mẫu").unwrap();
        std::fs::create_dir_all(proj.path().join("src")).unwrap();
        std::fs::write(proj.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(proj.path().join("node_modules/x")).unwrap();
        let att = proj.path().join("ghi-chu.txt");
        std::fs::write(&att, "nội dung đính kèm").unwrap();
        let bin = proj.path().join("anh.bin");
        std::fs::write(&bin, [0u8, 1, 2, 3]).unwrap();

        let core = Core::new(db::open_memory().unwrap(), data.path().into(), "/bin".into(), Box::new(|_| {}));
        let m = mission(&proj.path().to_string_lossy(), vec![att.to_string_lossy().into(), bin.to_string_lossy().into()], true);
        let ctx = prepare(&core, &m).unwrap();
        assert_eq!(ctx.cwd.as_deref(), Some(proj.path()));
        assert!(ctx.note.contains("src/") && ctx.note.contains("main.rs") && !ctx.note.contains("node_modules"));
        assert!(ctx.note.contains("nội dung đính kèm"));
        assert!(ctx.note.contains("anh.bin") && ctx.note.contains("không trích được"));
        assert!(ctx.for_kind("cli").contains("ĐƯỢC PHÉP"));
        assert!(ctx.for_kind("anthropic").contains("# Dự án mẫu"));
        assert_eq!(ctx.dirs.len(), 1);
        assert!(mission_dir(&core, 7).join("tep-dinh-kem/ghi-chu.txt").exists());
    }

    #[test]
    fn empty_mission_has_no_context() {
        let data = tempfile::tempdir().unwrap();
        let core = Core::new(db::open_memory().unwrap(), data.path().into(), "/bin".into(), Box::new(|_| {}));
        let ctx = prepare(&core, &mission("", vec![], false)).unwrap();
        assert!(ctx.is_empty() && ctx.cwd.is_none());
        assert!(validate("/khong/ton/tai", &[]).is_err());
    }
}
