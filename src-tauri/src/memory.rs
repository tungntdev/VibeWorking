//! Trí nhớ của nhân viên AI và thư viện mẫu tài liệu/báo cáo.
//!
//! - Trí nhớ: các bài học/sở thích của Quản lý rút ra từ những lần chỉnh sửa (AI tự ghi bằng dòng
//!   `[[GHI NHỚ]] …`, hoặc Quản lý thêm tay). Được đưa vào yêu cầu ở các việc sau.
//! - Mẫu tài liệu: cấu trúc báo cáo có sẵn (nhập từ .docx/.md/.txt). Nhiệm vụ dùng mẫu (chọn tay hoặc
//!   tự khớp theo từ khoá) → kết quả cuối trình bày đúng mẫu và tự xuất ra định dạng của mẫu.

use crate::db::now;
use crate::templates::fold;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MEMORY_MARK: &str = "[[GHI NHỚ]]";
pub const MEMORY_OFFICE_MARK: &str = "[[GHI NHỚ CHUNG]]";
const PROMPT_MEMORIES: usize = 25;

pub fn init(c: &Connection) -> Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS memories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id INTEGER,
            content TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT '',
            mission_id INTEGER,
            active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS doc_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            keywords TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            format TEXT NOT NULL DEFAULT 'docx',
            auto INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

// ───────────────────────────── Trí nhớ ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Memory {
    pub id: i64,
    /// None = trí nhớ chung của cả văn phòng
    pub agent_id: Option<i64>,
    pub content: String,
    /// ai | quan_ly
    pub source: String,
    pub mission_id: Option<i64>,
    pub active: bool,
    pub created_at: String,
}

fn mem_from(r: &Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get("id")?,
        agent_id: r.get("agent_id")?,
        content: r.get("content")?,
        source: r.get("source")?,
        mission_id: r.get("mission_id")?,
        active: r.get::<_, i64>("active")? != 0,
        created_at: r.get("created_at")?,
    })
}

/// Trí nhớ của một nhân viên (kèm trí nhớ chung), hoặc toàn bộ nếu agent = None.
pub fn list(c: &Connection, agent: Option<i64>) -> Result<Vec<Memory>> {
    let v = match agent {
        Some(a) => {
            let mut s = c.prepare("SELECT * FROM memories WHERE agent_id=?1 OR agent_id IS NULL ORDER BY id DESC")?;
            let v = s.query_map([a], mem_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
            v
        }
        None => {
            let mut s = c.prepare("SELECT * FROM memories ORDER BY id DESC")?;
            let v = s.query_map([], mem_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
            v
        }
    };
    Ok(v)
}

/// Thêm trí nhớ (bỏ qua nếu trùng nội dung với trí nhớ đang có).
pub fn add(c: &Connection, agent: Option<i64>, content: &str, source: &str, mission: Option<i64>) -> Result<Option<i64>> {
    let content = content.trim().trim_start_matches(['-', '•', ':']).trim();
    if content.chars().count() < 4 {
        return Ok(None);
    }
    let content: String = content.chars().take(400).collect();
    let key = fold(&content);
    if list(c, agent)?.iter().any(|m| fold(&m.content) == key && m.agent_id == agent) {
        return Ok(None);
    }
    c.execute(
        "INSERT INTO memories(agent_id,content,source,mission_id,active,created_at) VALUES(?1,?2,?3,?4,1,?5)",
        params![agent, content, source, mission, now()],
    )?;
    Ok(Some(c.last_insert_rowid()))
}

pub fn update(c: &Connection, id: i64, content: &str, active: bool, agent: Option<i64>) -> Result<()> {
    c.execute("UPDATE memories SET content=?2, active=?3, agent_id=?4 WHERE id=?1", params![id, content.trim(), active as i64, agent])?;
    Ok(())
}

pub fn delete(c: &Connection, id: i64) -> Result<()> {
    c.execute("DELETE FROM memories WHERE id=?1", [id])?;
    Ok(())
}

/// Đoạn "ghi nhớ" đưa vào yêu cầu cho nhân viên.
pub fn prompt_section(c: &Connection, agent: i64) -> Result<String> {
    let mems: Vec<Memory> = list(c, Some(agent))?.into_iter().filter(|m| m.active).take(PROMPT_MEMORIES).collect();
    if mems.is_empty() {
        return Ok(String::new());
    }
    let mut s = String::from("\n## GHI NHỚ TỪ CÁC LẦN LÀM VIỆC TRƯỚC (luôn tuân theo)\n");
    for m in mems.iter().rev() {
        s.push_str(&format!("- {}{}\n", m.content, if m.agent_id.is_none() { " (quy định chung của văn phòng)" } else { "" }));
    }
    Ok(s)
}

/// Tách các dòng `[[GHI NHỚ]] …` khỏi câu trả lời. Trả về ((nội dung, chung?), câu trả lời đã làm sạch).
pub fn extract(out: &str) -> (Vec<(String, bool)>, String) {
    let mut found = vec![];
    let mut kept = vec![];
    for line in out.lines() {
        let t = line.trim().trim_start_matches(['-', '*', '>', ' ']).trim();
        let hit = [(MEMORY_OFFICE_MARK, true), ("[[GHI NHO CHUNG]]", true), (MEMORY_MARK, false), ("[[GHI NHO]]", false)]
            .iter()
            .find_map(|(m, office)| t.strip_prefix(m).map(|rest| (rest.trim().trim_start_matches(':').trim().to_string(), *office)));
        match hit {
            Some((c, office)) if !c.is_empty() => found.push((c, office)),
            Some(_) => {}
            None => kept.push(line),
        }
    }
    (found, kept.join("\n").trim_end().to_string())
}

pub const MEMORY_RULE: &str = "Nếu Quản lý thể hiện một SỞ THÍCH/YÊU CẦU LÂU DÀI (văn phong, độ dài, định dạng, cách trình bày, \
quy định nghiệp vụ…) mà nên áp dụng cho các việc sau, hãy thêm ở CUỐI câu trả lời một dòng `[[GHI NHỚ]] <bài học ngắn gọn>` \
(dùng `[[GHI NHỚ CHUNG]]` nếu áp dụng cho cả văn phòng). Không ghi nhớ chi tiết chỉ dùng một lần.";

// ───────────────────────────── Mẫu tài liệu ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DocTemplate {
    pub id: i64,
    pub name: String,
    /// Dùng cho loại việc nào
    pub description: String,
    /// Từ khoá (phân cách bằng dấu phẩy) để tự chọn mẫu
    pub keywords: String,
    /// Nội dung mẫu (Markdown)
    pub content: String,
    /// Định dạng tự xuất khi xong: docx | pdf | xlsx | md | … | '' (không tự xuất)
    pub format: String,
    /// Cho phép tự khớp theo từ khoá
    pub auto: bool,
    pub created_at: String,
}

fn tpl_from(r: &Row) -> rusqlite::Result<DocTemplate> {
    Ok(DocTemplate {
        id: r.get("id")?,
        name: r.get("name")?,
        description: r.get("description")?,
        keywords: r.get("keywords")?,
        content: r.get("content")?,
        format: r.get("format")?,
        auto: r.get::<_, i64>("auto")? != 0,
        created_at: r.get("created_at")?,
    })
}

pub fn list_templates(c: &Connection) -> Result<Vec<DocTemplate>> {
    let mut s = c.prepare("SELECT * FROM doc_templates ORDER BY name")?;
    let v = s.query_map([], tpl_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn get_template(c: &Connection, id: i64) -> Result<Option<DocTemplate>> {
    Ok(c.query_row("SELECT * FROM doc_templates WHERE id=?1", [id], tpl_from).optional()?)
}

pub fn save_template(c: &Connection, t: &DocTemplate) -> Result<i64> {
    if t.name.trim().is_empty() || t.content.trim().is_empty() {
        return Err(anyhow!("Mẫu cần có tên và nội dung"));
    }
    if t.id > 0 {
        c.execute(
            "UPDATE doc_templates SET name=?2, description=?3, keywords=?4, content=?5, format=?6, auto=?7 WHERE id=?1",
            params![t.id, t.name.trim(), t.description.trim(), t.keywords.trim(), t.content, t.format, t.auto as i64],
        )?;
        Ok(t.id)
    } else {
        c.execute(
            "INSERT INTO doc_templates(name,description,keywords,content,format,auto,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![t.name.trim(), t.description.trim(), t.keywords.trim(), t.content, t.format, t.auto as i64, now()],
        )?;
        Ok(c.last_insert_rowid())
    }
}

pub fn delete_template(c: &Connection, id: i64) -> Result<()> {
    c.execute("UPDATE missions SET template_id=NULL WHERE template_id=?1", [id])?;
    c.execute("DELETE FROM doc_templates WHERE id=?1", [id])?;
    Ok(())
}

/// Tự chọn mẫu khớp nhất với nội dung nhiệm vụ (theo từ khoá và tên mẫu).
pub fn match_template(c: &Connection, text: &str) -> Result<Option<DocTemplate>> {
    let t = fold(text);
    let mut best: Option<(usize, DocTemplate)> = None;
    for tpl in list_templates(c)?.into_iter().filter(|x| x.auto) {
        let mut score = 0;
        for k in tpl.keywords.split([',', ';', '\n']).map(|k| fold(k.trim())).filter(|k| k.len() > 1) {
            if t.contains(&k) {
                score += 3 + k.split_whitespace().count();
            }
        }
        if t.contains(&fold(tpl.name.trim())) {
            score += 5;
        }
        if score > 0 && best.as_ref().map_or(true, |(b, _)| score > *b) {
            best = Some((score, tpl));
        }
    }
    Ok(best.map(|(_, t)| t))
}

pub fn template_section(t: &DocTemplate) -> String {
    format!(
        "\n## MẪU KẾT QUẢ BẮT BUỘC: “{}”\n{}Hãy trình bày kết quả CUỐI CÙNG theo đúng cấu trúc, đề mục, thứ tự và văn phong của mẫu dưới đây \
         (giữ nguyên các tiêu đề, bảng; thay phần nội dung bằng thông tin thực tế; mục nào không có thông tin thì ghi rõ “chưa có số liệu”).\n\
         ---- MẪU ----\n{}\n---- HẾT MẪU ----\n",
        t.name,
        if t.description.trim().is_empty() { String::new() } else { format!("Dùng cho: {}\n", t.description.trim()) },
        t.content.trim()
    )
}

// ───────────────────────────── Nhập mẫu từ tệp ─────────────────────────────

fn docx_to_markdown(buf: &[u8]) -> Result<String> {
    use docx_rs::*;
    let doc = read_docx(buf).map_err(|e| anyhow!("Không đọc được tệp Word: {e:?}"))?;
    fn para_text(p: &Paragraph) -> (String, bool) {
        let mut s = String::new();
        let mut all_bold = true;
        let mut any = false;
        for ch in &p.children {
            if let ParagraphChild::Run(r) = ch {
                let mut t = String::new();
                for rc in &r.children {
                    match rc {
                        RunChild::Text(x) => t.push_str(&x.text),
                        RunChild::Tab(_) => t.push('\t'),
                        RunChild::Break(_) => t.push(' '),
                        _ => {}
                    }
                }
                if !t.trim().is_empty() {
                    any = true;
                    if r.run_property.bold.is_none() {
                        all_bold = false;
                    }
                }
                s.push_str(&t);
            }
        }
        (s, any && all_bold)
    }
    let mut out = vec![];
    for child in &doc.document.children {
        match child {
            DocumentChild::Paragraph(p) => {
                let (text, bold) = para_text(p);
                let text = text.trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let style = p.property.style.as_ref().map(|s| s.val.to_lowercase()).unwrap_or_default();
                let level = if style.contains("title") {
                    Some(1)
                } else if let Some(d) = style.strip_prefix("heading").and_then(|d| d.trim().chars().next()).and_then(|c| c.to_digit(10)) {
                    Some(d.min(4) as usize)
                } else {
                    None
                };
                match level {
                    Some(l) => out.push(format!("{} {}", "#".repeat(l), text)),
                    None if p.has_numbering => out.push(format!("- {text}")),
                    None if bold && text.chars().count() < 90 => out.push(format!("### {text}")),
                    None => out.push(text),
                }
            }
            DocumentChild::Table(t) => {
                let mut rows: Vec<Vec<String>> = vec![];
                for TableChild::TableRow(r) in &t.rows {
                    let mut cells = vec![];
                    for TableRowChild::TableCell(c) in &r.cells {
                        let txt: Vec<String> = c
                            .children
                            .iter()
                            .filter_map(|x| if let TableCellContent::Paragraph(p) = x { Some(para_text(p).0.trim().to_string()) } else { None })
                            .filter(|s| !s.is_empty())
                            .collect();
                        cells.push(txt.join(" ").replace('|', "/"));
                    }
                    rows.push(cells);
                }
                if let Some(head) = rows.first() {
                    let n = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                    let pad = |r: &Vec<String>| (0..n).map(|i| r.get(i).cloned().unwrap_or_default()).collect::<Vec<_>>().join(" | ");
                    out.push(format!("| {} |\n|{}|", pad(head), vec!["---"; n].join("|")));
                    for r in rows.iter().skip(1) {
                        out.push(format!("| {} |", pad(r)));
                    }
                }
            }
            _ => {}
        }
    }
    // gộp các dòng bảng liên tiếp (đã có \n), cách đoạn giữa các khối khác
    let mut md = String::new();
    for (i, b) in out.iter().enumerate() {
        let tablish = b.starts_with('|');
        let prev_tablish = i > 0 && out[i - 1].starts_with('|');
        let listy = b.starts_with("- ") && i > 0 && out[i - 1].starts_with("- ");
        if i > 0 {
            md.push_str(if (tablish && prev_tablish) || listy { "\n" } else { "\n\n" });
        }
        md.push_str(b);
    }
    Ok(md)
}

/// Đọc tệp mẫu → (tên gợi ý, nội dung Markdown).
pub fn import_file(path: &Path) -> Result<(String, String)> {
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Mẫu mới".into());
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let content = match ext.as_str() {
        "md" | "markdown" | "txt" => std::fs::read_to_string(path)?,
        "docx" => docx_to_markdown(&std::fs::read(path)?)?,
        "doc" | "rtf" | "odt" | "html" | "htm" | "pages" if cfg!(target_os = "macos") => {
            // macOS: textutil chuyển sang .docx rồi đọc như Word
            let tmp = std::env::temp_dir().join(format!("ai-office-mau-{}.docx", rand::random::<u32>()));
            let ok = std::process::Command::new("textutil").args(["-convert", "docx"]).arg(path).arg("-output").arg(&tmp).status()?.success();
            if !ok {
                return Err(anyhow!("Không chuyển được tệp .{ext}"));
            }
            let r = docx_to_markdown(&std::fs::read(&tmp)?);
            let _ = std::fs::remove_file(&tmp);
            r?
        }
        _ => return Err(anyhow!("Hãy dùng tệp .docx, .md hoặc .txt làm mẫu")),
    };
    if content.trim().is_empty() {
        return Err(anyhow!("Tệp mẫu không có nội dung"));
    }
    Ok((name, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        crate::db::open_memory().unwrap()
    }

    #[test]
    fn memories_add_dedupe_and_prompt() {
        let c = conn();
        assert!(add(&c, Some(3), "Báo cáo luôn tối đa 1 trang", "ai", Some(1)).unwrap().is_some());
        assert!(add(&c, Some(3), "báo cáo LUÔN tối đa 1 trang", "ai", Some(2)).unwrap().is_none(), "trùng thì bỏ qua");
        add(&c, None, "Dùng giọng văn hành chính trang trọng", "quan_ly", None).unwrap();
        add(&c, Some(9), "Việc của người khác", "ai", None).unwrap();
        let p = prompt_section(&c, 3).unwrap();
        assert!(p.contains("tối đa 1 trang") && p.contains("trang trọng (quy định chung") && !p.contains("người khác"));
        let id = list(&c, Some(3)).unwrap()[0].id;
        update(&c, id, "x đã sửa", false, Some(3)).unwrap();
        assert!(!prompt_section(&c, 3).unwrap().contains("x đã sửa"));
    }

    #[test]
    fn memory_lines_extracted() {
        let out = "Kết quả đây.\n\n[[GHI NHỚ]] Quản lý thích bảng số liệu ở đầu báo cáo\n- [[GHI NHỚ CHUNG]]: Đơn vị tiền là triệu đồng\n[[GHI NHỚ]]";
        let (m, cleaned) = extract(out);
        assert_eq!(m, vec![("Quản lý thích bảng số liệu ở đầu báo cáo".to_string(), false), ("Đơn vị tiền là triệu đồng".to_string(), true)]);
        assert_eq!(cleaned, "Kết quả đây.");
    }

    #[test]
    fn templates_match_by_keywords() {
        let c = conn();
        let mut t = DocTemplate { name: "Báo cáo tuần".into(), keywords: "báo cáo tuần, tuần qua".into(), content: "# Báo cáo tuần\n## 1. Kết quả".into(), format: "docx".into(), auto: true, ..Default::default() };
        save_template(&c, &t).unwrap();
        t.name = "Tờ trình".into();
        t.keywords = "tờ trình, đề xuất mua sắm".into();
        t.content = "# TỜ TRÌNH".into();
        save_template(&c, &t).unwrap();
        assert_eq!(match_template(&c, "Lập tờ trình đề xuất mua sắm thiết bị").unwrap().unwrap().name, "Tờ trình");
        assert_eq!(match_template(&c, "Viết BÁO CÁO TUẦN phòng kế hoạch").unwrap().unwrap().name, "Báo cáo tuần");
        assert!(match_template(&c, "Dịch tài liệu").unwrap().is_none());
    }

    #[test]
    fn import_docx_template_roundtrip() {
        // xuất một tệp Word rồi nhập lại làm mẫu: giữ tiêu đề, bảng, danh sách
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Mẫu báo cáo quý.docx");
        crate::export::export("# Báo cáo quý\n\n## 1. Số liệu\n\n| Chỉ tiêu | Kết quả |\n|---|---|\n| Doanh thu | … |\n\n## 2. Đề xuất\n\nNội dung đề xuất.", "", "docx", &p).unwrap();
        let (name, md) = import_file(&p).unwrap();
        assert_eq!(name, "Mẫu báo cáo quý");
        assert!(md.contains("Báo cáo quý") && md.contains("| Chỉ tiêu | Kết quả |") && md.contains("Đề xuất"), "{md}");
    }
}
