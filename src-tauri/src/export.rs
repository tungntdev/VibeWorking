//! Xuất kết quả ra tệp: Markdown → Word (.docx), PDF, Excel (.xlsx), CSV, HTML, Markdown, văn bản.
//! Đồng thời hỗ trợ AI "đóng gói" tệp trong câu trả lời bằng khối đánh dấu để ứng dụng tạo tệp thật.

use anyhow::{anyhow, Context, Result};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const FILE_START: &str = "<<<TỆP:";
pub const FILE_END: &str = "<<<HẾT TỆP>>>";
pub const FORMATS: &[(&str, &str)] = &[
    ("docx", "Word (.docx)"),
    ("pdf", "PDF (.pdf)"),
    ("xlsx", "Excel (.xlsx)"),
    ("csv", "Bảng CSV (.csv)"),
    ("html", "Trang web (.html)"),
    ("md", "Markdown (.md)"),
    ("txt", "Văn bản (.txt)"),
];

// ───────────────────────────── Mô hình nội dung ─────────────────────────────

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Span>),
    Para(Vec<Span>),
    List { ordered: bool, items: Vec<Vec<Span>> },
    Code(String),
    Quote(Vec<Span>),
    Table { head: Vec<String>, rows: Vec<Vec<String>> },
    Rule,
}

fn plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Đọc Markdown thành các khối đơn giản (tiêu đề, đoạn, danh sách, bảng, mã, trích dẫn).
pub fn parse(md: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut blocks = vec![];
    let mut spans: Vec<Span> = vec![];
    let (mut bold, mut italic) = (0, 0);
    let mut heading: Option<u8> = None;
    let mut list_stack: Vec<(bool, Vec<Vec<Span>>)> = vec![];
    let mut in_item = 0;
    let mut quote = 0;
    let mut code: Option<String> = None;
    let mut table: Option<(Vec<String>, Vec<Vec<String>>, Vec<String>, bool)> = None; // head, rows, current row, in_head
    let mut cell = String::new();

    let push_text = |spans: &mut Vec<Span>, t: &str, bold: i32, italic: i32, code: bool| {
        spans.push(Span { text: t.to_string(), bold: bold > 0, italic: italic > 0, code });
    };

    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    _ => 4,
                });
                spans.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                blocks.push(Block::Heading(heading.take().unwrap_or(2), std::mem::take(&mut spans)));
            }
            Event::Start(Tag::Paragraph) => {
                if in_item == 0 && quote == 0 {
                    spans.clear();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if in_item > 0 {
                    spans.push(Span { text: " ".into(), ..Default::default() });
                } else if quote == 0 && !spans.is_empty() {
                    blocks.push(Block::Para(std::mem::take(&mut spans)));
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                quote += 1;
                spans.clear();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                quote -= 1;
                if quote == 0 && !spans.is_empty() {
                    blocks.push(Block::Quote(std::mem::take(&mut spans)));
                }
            }
            Event::Start(Tag::List(start)) => {
                // danh sách lồng nhau: đóng mục cha đang viết dở để giữ thứ tự
                if in_item > 0 {
                    if let Some((_, items)) = list_stack.last_mut() {
                        items.push(std::mem::take(&mut spans));
                    }
                }
                list_stack.push((start.is_some(), vec![]));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some((ordered, items)) = list_stack.pop() {
                    let items: Vec<Vec<Span>> = items.into_iter().filter(|i| !plain(i).trim().is_empty()).collect();
                    if let Some((_, parent)) = list_stack.last_mut() {
                        for it in items {
                            let mut s = vec![Span { text: "    – ".into(), ..Default::default() }];
                            s.extend(it);
                            parent.push(s);
                        }
                    } else {
                        blocks.push(Block::List { ordered, items });
                    }
                }
            }
            Event::Start(Tag::Item) => {
                in_item += 1;
                spans.clear();
            }
            Event::End(TagEnd::Item) => {
                in_item -= 1;
                if let Some((_, items)) = list_stack.last_mut() {
                    let it = std::mem::take(&mut spans);
                    if !plain(&it).trim().is_empty() {
                        items.push(it);
                    }
                }
            }
            Event::Start(Tag::CodeBlock(_)) => code = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(c) = code.take() {
                    blocks.push(Block::Code(c.trim_end().to_string()));
                }
            }
            Event::Start(Tag::Table(_)) => table = Some((vec![], vec![], vec![], false)),
            Event::End(TagEnd::Table) => {
                if let Some((head, rows, _, _)) = table.take() {
                    blocks.push(Block::Table { head, rows });
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.3 = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.0 = std::mem::take(&mut t.2);
                    t.3 = false;
                }
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                if let Some(t) = table.as_mut() {
                    let row = std::mem::take(&mut t.2);
                    t.1.push(row);
                }
            }
            Event::Start(Tag::TableCell) => cell.clear(),
            Event::End(TagEnd::TableCell) => {
                if let Some(t) = table.as_mut() {
                    t.2.push(cell.trim().to_string());
                }
            }
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold -= 1,
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic -= 1,
            Event::Text(t) => {
                if let Some(c) = code.as_mut() {
                    c.push_str(&t);
                } else if table.is_some() {
                    cell.push_str(&t);
                } else {
                    push_text(&mut spans, &t, bold, italic, false);
                }
            }
            Event::Code(t) => {
                if table.is_some() {
                    cell.push_str(&t);
                } else {
                    push_text(&mut spans, &t, bold, italic, true);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if table.is_some() {
                    cell.push(' ');
                } else {
                    push_text(&mut spans, if code.is_some() { "\n" } else { " " }, bold, italic, false);
                }
            }
            Event::TaskListMarker(done) => push_text(&mut spans, if done { "☑ " } else { "☐ " }, 0, 0, false),
            Event::Rule => blocks.push(Block::Rule),
            Event::Html(h) | Event::InlineHtml(h) => push_text(&mut spans, &h, bold, italic, false),
            _ => {}
        }
    }
    if !spans.is_empty() {
        blocks.push(Block::Para(spans));
    }
    blocks
}

// ───────────────────────────── Các định dạng ─────────────────────────────

const CSS: &str = "body{font-family:'Times New Roman',serif;font-size:13pt;line-height:1.5;max-width:860px;margin:32px auto;padding:0 24px;color:#111}\
h1{font-size:20pt}h2{font-size:16pt}h3{font-size:14pt}table{border-collapse:collapse;margin:10px 0}\
th,td{border:1px solid #555;padding:5px 9px;text-align:left}th{background:#eef}code,pre{font-family:Menlo,Consolas,monospace;font-size:11pt}\
pre{background:#f5f5f5;padding:10px;border-radius:6px;overflow:auto}blockquote{border-left:3px solid #88f;margin:8px 0;padding:2px 12px;color:#444}";

pub fn to_html(md: &str, title: &str) -> String {
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, Parser::new_ext(md, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS));
    format!(
        "<!doctype html><html lang=\"vi\"><head><meta charset=\"utf-8\"><title>{}</title><style>{CSS}</style></head><body>{body}</body></html>",
        html_escape(title)
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn to_text(md: &str) -> String {
    let mut out = vec![];
    for b in parse(md) {
        match b {
            Block::Heading(_, s) => out.push(plain(&s).to_uppercase()),
            Block::Para(s) | Block::Quote(s) => out.push(plain(&s)),
            Block::List { ordered, items } => out.push(
                items.iter().enumerate().map(|(i, it)| format!("{} {}", if ordered { format!("{}.", i + 1) } else { "•".into() }, plain(it).trim())).collect::<Vec<_>>().join("\n"),
            ),
            Block::Code(c) => out.push(c),
            Block::Table { head, rows } => {
                let mut t = vec![head.join(" | ")];
                t.extend(rows.iter().map(|r| r.join(" | ")));
                out.push(t.join("\n"));
            }
            Block::Rule => out.push("—".repeat(20)),
        }
    }
    out.join("\n\n")
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n', ';']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

pub fn to_csv(md: &str) -> String {
    let tables: Vec<(Vec<String>, Vec<Vec<String>>)> = parse(md)
        .into_iter()
        .filter_map(|b| if let Block::Table { head, rows } = b { Some((head, rows)) } else { None })
        .collect();
    let mut out = String::from("\u{feff}"); // BOM để Excel đọc đúng tiếng Việt
    if tables.is_empty() {
        for l in to_text(md).lines() {
            out.push_str(&csv_escape(l));
            out.push('\n');
        }
        return out;
    }
    for (i, (head, rows)) in tables.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&head.iter().map(|c| csv_escape(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
        for r in rows {
            out.push_str(&r.iter().map(|c| csv_escape(c)).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
    }
    out
}

/// Số trong bảng: "1.250" / "1,250.5" / "12%" → số để Excel tính được.
fn as_number(s: &str) -> Option<f64> {
    let t = s.trim().trim_end_matches('%').replace(' ', "");
    if t.is_empty() || t.chars().any(|c| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-')) {
        return None;
    }
    let (dots, commas) = (t.matches('.').count(), t.matches(',').count());
    let normalized = if dots > 0 && commas > 0 {
        // có cả hai: dấu xuất hiện sau cùng là dấu thập phân
        if t.rfind(',') > t.rfind('.') { t.replace('.', "").replace(',', ".") } else { t.replace(',', "") }
    } else {
        let sep = if dots > 0 { '.' } else { ',' };
        let n = dots + commas;
        let tail = t.rsplit(sep).next().unwrap_or("");
        if n == 0 {
            t.clone()
        } else if n > 1 || tail.len() == 3 {
            // "1.250.000" hoặc "1.250": phân cách hàng nghìn
            t.replace(sep, "")
        } else {
            // "320,5" / "3.14": dấu thập phân
            t.replace(sep, ".")
        }
    };
    normalized.parse().ok()
}

pub fn write_xlsx(md: &str, title: &str, path: &Path) -> Result<()> {
    use rust_xlsxwriter::{Format, FormatBorder, Workbook};
    let mut wb = Workbook::new();
    let head_fmt = Format::new().set_bold().set_background_color("#DDE3FF").set_border(FormatBorder::Thin);
    let cell_fmt = Format::new().set_border(FormatBorder::Thin);
    let blocks = parse(md);
    let tables: Vec<_> = blocks.iter().filter_map(|b| if let Block::Table { head, rows } = b { Some((head, rows)) } else { None }).collect();
    if tables.is_empty() {
        let ws = wb.add_worksheet();
        ws.set_name("Nội dung")?;
        ws.write_string_with_format(0, 0, title, &head_fmt)?;
        for (i, line) in to_text(md).lines().enumerate() {
            ws.write_string(i as u32 + 2, 0, line)?;
        }
        ws.set_column_width(0, 100)?;
    } else {
        for (ti, (head, rows)) in tables.iter().enumerate() {
            let ws = wb.add_worksheet();
            ws.set_name(format!("Bảng {}", ti + 1))?;
            for (c, h) in head.iter().enumerate() {
                ws.write_string_with_format(0, c as u16, h, &head_fmt)?;
            }
            for (r, row) in rows.iter().enumerate() {
                for (c, v) in row.iter().enumerate() {
                    match as_number(v) {
                        Some(n) => ws.write_number_with_format(r as u32 + 1, c as u16, n, &cell_fmt)?,
                        None => ws.write_string_with_format(r as u32 + 1, c as u16, v, &cell_fmt)?,
                    };
                }
            }
            ws.autofit();
        }
    }
    wb.save(path).with_context(|| format!("Không ghi được {}", path.display()))?;
    Ok(())
}

pub fn write_docx(md: &str, title: &str, path: &Path) -> Result<()> {
    use docx_rs::*;
    let font = RunFonts::new().ascii("Times New Roman").hi_ansi("Times New Roman").east_asia("Times New Roman").cs("Times New Roman");
    let mut doc = Docx::new().default_fonts(font).default_size(26); // 13pt
    let run_of = |s: &Span| {
        let mut r = Run::new().add_text(&s.text);
        if s.bold {
            r = r.bold();
        }
        if s.italic {
            r = r.italic();
        }
        if s.code {
            r = r.fonts(RunFonts::new().ascii("Courier New").hi_ansi("Courier New"));
        }
        r
    };
    let gap = || LineSpacing::new().after(120).line(300); // cách đoạn 6pt, giãn dòng 1,25
    let para_of = |spans: &[Span]| spans.iter().fold(Paragraph::new().line_spacing(gap()), |p, s| p.add_run(run_of(s)));
    let blocks = parse(md);
    if !matches!(blocks.first(), Some(Block::Heading(1, _))) && !title.is_empty() {
        doc = doc.add_paragraph(Paragraph::new().align(AlignmentType::Center).add_run(Run::new().add_text(title).bold().size(36)));
    }
    for b in blocks {
        doc = match b {
            Block::Heading(l, s) => {
                let size = match l { 1 => 36, 2 => 30, 3 => 28, _ => 26 };
                let mut p = Paragraph::new().line_spacing(LineSpacing::new().before(200).after(100));
                if l == 1 {
                    p = p.align(AlignmentType::Center);
                }
                doc.add_paragraph(s.iter().fold(p, |p, sp| p.add_run(run_of(sp).bold().size(size))))
            }
            Block::Para(s) => doc.add_paragraph(para_of(&s).align(AlignmentType::Both)),
            Block::Quote(s) => doc.add_paragraph(s.iter().fold(Paragraph::new().indent(Some(567), None, None, None), |p, sp| p.add_run(run_of(sp).italic()))),
            Block::List { ordered, items } => {
                let mut d = doc;
                for (i, it) in items.iter().enumerate() {
                    let mark = if ordered { format!("{}. ", i + 1) } else { "• ".into() };
                    let p = Paragraph::new().indent(Some(567), Some(SpecialIndentType::Hanging(283)), None, None).add_run(Run::new().add_text(mark));
                    d = d.add_paragraph(it.iter().fold(p, |p, sp| p.add_run(run_of(sp))));
                }
                d
            }
            Block::Code(c) => {
                let mut d = doc;
                for line in c.lines() {
                    d = d.add_paragraph(Paragraph::new().add_run(Run::new().add_text(line).fonts(RunFonts::new().ascii("Courier New").hi_ansi("Courier New")).size(20)));
                }
                d
            }
            Block::Table { head, rows } => {
                let cols = head.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(1)).max(1);
                let col_w = 9000 / cols;
                let cell = |t: &str, b: bool| {
                    let r = Run::new().add_text(t);
                    TableCell::new().width(col_w, WidthType::Dxa).add_paragraph(Paragraph::new().add_run(if b { r.bold() } else { r }))
                };
                let mut trs = vec![TableRow::new(head.iter().map(|h| cell(h, true).shading(Shading::new().fill("DDE3FF"))).collect())];
                for r in rows {
                    trs.push(TableRow::new(r.iter().map(|c| cell(c, false)).collect()));
                }
                // bảng rộng hết trang (A4 trừ lề ≈ 9000 dxa), chia đều các cột
                doc.add_table(Table::new(trs).width(9000, WidthType::Dxa).layout(TableLayoutType::Fixed).set_grid(vec![col_w; cols])).add_paragraph(Paragraph::new())
            }
            Block::Rule => doc.add_paragraph(Paragraph::new().add_run(Run::new().add_text("—".repeat(30)))),
        };
    }
    let f = std::fs::File::create(path).with_context(|| format!("Không tạo được {}", path.display()))?;
    doc.build().pack(f).map_err(|e| anyhow!("Lỗi tạo Word: {e}"))?;
    Ok(())
}

/// Phông chữ có dấu tiếng Việt cho PDF (macOS: Arial; Windows/Linux: các vị trí phổ biến).
fn pdf_font() -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>> {
    let sets: &[[&str; 4]] = &[
        ["/System/Library/Fonts/Supplemental/Arial.ttf", "/System/Library/Fonts/Supplemental/Arial Bold.ttf", "/System/Library/Fonts/Supplemental/Arial Italic.ttf", "/System/Library/Fonts/Supplemental/Arial Bold Italic.ttf"],
        ["/Library/Fonts/Arial.ttf", "/Library/Fonts/Arial Bold.ttf", "/Library/Fonts/Arial Italic.ttf", "/Library/Fonts/Arial Bold Italic.ttf"],
        ["C:\\Windows\\Fonts\\arial.ttf", "C:\\Windows\\Fonts\\arialbd.ttf", "C:\\Windows\\Fonts\\ariali.ttf", "C:\\Windows\\Fonts\\arialbi.ttf"],
        ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf"],
    ];
    for s in sets {
        if s.iter().all(|p| Path::new(p).exists()) {
            let load = |p: &str| genpdf::fonts::FontData::new(std::fs::read(p)?, None).map_err(|e| anyhow!("Phông chữ lỗi: {e}"));
            return Ok(genpdf::fonts::FontFamily { regular: load(s[0])?, bold: load(s[1])?, italic: load(s[2])?, bold_italic: load(s[3])? });
        }
    }
    Err(anyhow!("Không tìm thấy phông chữ có dấu tiếng Việt để tạo PDF"))
}

pub fn write_pdf(md: &str, title: &str, path: &Path) -> Result<()> {
    use genpdf::{elements, style, Element};
    let mut doc = genpdf::Document::new(pdf_font()?);
    doc.set_title(title);
    doc.set_font_size(11);
    doc.set_line_spacing(1.3);
    let mut deco = genpdf::SimplePageDecorator::new();
    deco.set_margins(18);
    doc.set_page_decorator(deco);
    let styled = |spans: &[Span], extra: style::Style| -> elements::Paragraph {
        let mut p = elements::Paragraph::default();
        for s in spans {
            let mut st = extra;
            if s.bold {
                st = st.bold();
            }
            if s.italic {
                st = st.italic();
            }
            p.push_styled(s.text.clone(), st);
        }
        p
    };
    let blocks = parse(md);
    if !matches!(blocks.first(), Some(Block::Heading(1, _))) && !title.is_empty() {
        doc.push(elements::Paragraph::new(title).aligned(genpdf::Alignment::Center).styled(style::Style::new().bold().with_font_size(18)));
        doc.push(elements::Break::new(1));
    }
    for b in blocks {
        match b {
            Block::Heading(l, s) => {
                let size = match l { 1 => 18, 2 => 15, 3 => 13, _ => 12 };
                doc.push(elements::Break::new(0.6));
                doc.push(styled(&s, style::Style::new().bold().with_font_size(size)));
            }
            Block::Para(s) => {
                doc.push(styled(&s, style::Style::new()));
                doc.push(elements::Break::new(0.4));
            }
            Block::Quote(s) => doc.push(styled(&s, style::Style::new().italic()).padded(genpdf::Margins::trbl(0, 0, 0, 8))),
            Block::List { ordered, items } => {
                if ordered {
                    let mut l = elements::OrderedList::new();
                    for it in items {
                        l.push(styled(&it, style::Style::new()));
                    }
                    doc.push(l);
                } else {
                    let mut l = elements::UnorderedList::new();
                    for it in items {
                        l.push(styled(&it, style::Style::new()));
                    }
                    doc.push(l);
                }
                doc.push(elements::Break::new(0.4));
            }
            Block::Code(c) => {
                for line in c.lines() {
                    doc.push(elements::Paragraph::new(line).styled(style::Style::new().with_font_size(9)));
                }
            }
            Block::Table { head, rows } => {
                let cols = head.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(1)).max(1);
                let mut t = elements::TableLayout::new(vec![1; cols]);
                t.set_cell_decorator(elements::FrameCellDecorator::new(true, true, false));
                let mut row = t.row();
                for c in 0..cols {
                    row.push_element(elements::Paragraph::new(head.get(c).cloned().unwrap_or_default()).styled(style::Style::new().bold()).padded(1));
                }
                row.push().map_err(|e| anyhow!("Lỗi bảng PDF: {e}"))?;
                for r in rows {
                    let mut row = t.row();
                    for c in 0..cols {
                        row.push_element(elements::Paragraph::new(r.get(c).cloned().unwrap_or_default()).padded(1));
                    }
                    row.push().map_err(|e| anyhow!("Lỗi bảng PDF: {e}"))?;
                }
                doc.push(t);
                doc.push(elements::Break::new(0.6));
            }
            Block::Rule => doc.push(elements::Break::new(1)),
        }
    }
    doc.render_to_file(path).map_err(|e| anyhow!("Lỗi tạo PDF: {e}"))?;
    Ok(())
}

/// Xuất nội dung Markdown ra tệp theo định dạng (đuôi tệp).
pub fn export(md: &str, title: &str, format: &str, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match format {
        "docx" => write_docx(md, title, path),
        "pdf" => write_pdf(md, title, path),
        "xlsx" => write_xlsx(md, title, path),
        "csv" => Ok(std::fs::write(path, to_csv(md))?),
        "html" | "htm" => Ok(std::fs::write(path, to_html(md, title))?),
        "txt" => Ok(std::fs::write(path, to_text(md))?),
        "md" | "markdown" => Ok(std::fs::write(path, md)?),
        f => Err(anyhow!("Chưa hỗ trợ định dạng .{f}")),
    }
}

// ───────────────────────────── Tên tệp & khối tệp trong câu trả lời AI ─────────────────────────────

pub fn safe_name(name: &str) -> String {
    let n: String = name
        .trim()
        .chars()
        .map(|c| if "/\\:*?\"<>|\0".contains(c) || c.is_control() { '-' } else { c })
        .collect();
    let n = n.trim_matches(|c: char| c == '.' || c == ' ' || c == '-').to_string();
    let n: String = n.chars().take(120).collect();
    if n.is_empty() { "ket-qua".into() } else { n }
}

/// Tên tệp chưa tồn tại trong thư mục: "a.docx" → "a (2).docx"…
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.to_string(), String::new()),
    };
    (2..1000).map(|i| dir.join(format!("{stem} ({i}){ext}"))).find(|p| !p.exists()).unwrap_or(p)
}

pub fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

/// Tách các khối `<<<TỆP: ten.docx>>> … <<<HẾT TỆP>>>` khỏi câu trả lời.
/// Trả về (danh sách (tên, nội dung), câu trả lời đã thay khối bằng dòng ghi chú).
pub fn extract_file_blocks(out: &str) -> (Vec<(String, String)>, String) {
    let mut files = vec![];
    let mut cleaned = vec![];
    let mut cur: Option<(String, Vec<&str>)> = None;
    for line in out.lines() {
        let t = line.trim();
        let start = t.strip_prefix(FILE_START).or_else(|| t.strip_prefix("<<<TEP:")).or_else(|| t.strip_prefix("<<<FILE:"));
        if let (None, Some(rest)) = (&cur, start) {
            let name = rest.trim().trim_end_matches(">>>").trim().to_string();
            cur = Some((name, vec![]));
            continue;
        }
        if cur.is_some() && (t == FILE_END || t == "<<<HET TEP>>>" || t == "<<<END>>>" || t == "<<<END FILE>>>") {
            let (name, body) = cur.take().unwrap();
            let mut content = body.join("\n");
            // AI hay bọc nội dung trong ```markdown … ```
            let tr = content.trim();
            if tr.starts_with("```") && tr.ends_with("```") {
                content = tr.trim_start_matches(|c| c != '\n').trim_end_matches('`').trim().to_string();
            }
            cleaned.push(format!("📄 Đã tạo tệp **{}**", safe_name(&name)));
            files.push((name, content));
            continue;
        }
        match cur.as_mut() {
            Some((_, body)) => body.push(line),
            None => cleaned.push(line.to_string()),
        }
    }
    // khối chưa đóng: vẫn tạo tệp với phần đã có
    if let Some((name, body)) = cur {
        cleaned.push(format!("📄 Đã tạo tệp **{}**", safe_name(&name)));
        files.push((name, body.join("\n")));
    }
    (files, cleaned.join("\n"))
}

// ───────────────────────────── Dò tệp đã tạo/sửa trong thư mục ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileRec {
    pub path: String,
    pub name: String,
    pub size: u64,
    /// tao (mới) | sua (đã sửa) | xuat (ứng dụng xuất từ nội dung AI)
    pub kind: String,
}

pub type Snapshot = HashMap<PathBuf, (SystemTime, u64)>;

const SKIP: &[&str] = &[".git", "node_modules", "target", "dist", "build", "__pycache__", ".venv", "venv", ".next", ".idea", ".vscode", "DerivedData"];

pub fn snapshot(root: &Path) -> Snapshot {
    fn walk(dir: &Path, depth: usize, out: &mut Snapshot) {
        if depth == 0 || out.len() > 5000 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || SKIP.contains(&name.as_str()) {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                walk(&p, depth - 1, out);
            } else if let Ok(m) = e.metadata() {
                out.insert(p, (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()));
            }
        }
    }
    let mut out = HashMap::new();
    walk(root, 5, &mut out);
    out
}

pub fn diff(before: &Snapshot, after: &Snapshot) -> Vec<FileRec> {
    let mut v: Vec<FileRec> = after
        .iter()
        .filter_map(|(p, (t, size))| {
            let kind = match before.get(p) {
                None => "tao",
                Some((t0, s0)) if t0 != t || s0 != size => "sua",
                _ => return None,
            };
            Some(FileRec { path: p.to_string_lossy().into(), name: p.file_name()?.to_string_lossy().into(), size: *size, kind: kind.into() })
        })
        .collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v.truncate(100);
    v
}

/// Tạo tệp thật từ các khối tệp AI trả về.
pub fn materialize(blocks: &[(String, String)], dir: &Path, title: &str) -> Vec<FileRec> {
    let mut out = vec![];
    for (name, content) in blocks {
        let mut name = safe_name(name);
        let mut ext = ext_of(&name);
        if !FORMATS.iter().any(|(e, _)| *e == ext) {
            name.push_str(".md");
            ext = "md".into();
        }
        let path = unique_path(dir, &name);
        match export(content, title, &ext, &path) {
            Ok(()) => {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                out.push(FileRec { path: path.to_string_lossy().into(), name: path.file_name().unwrap().to_string_lossy().into(), size, kind: "xuat".into() });
            }
            Err(e) => {
                // không tạo được định dạng yêu cầu → lưu tạm dạng Markdown để không mất nội dung
                let fallback = unique_path(dir, &format!("{name}.md"));
                if std::fs::write(&fallback, format!("<!-- Không tạo được {name}: {e} -->\n\n{content}")).is_ok() {
                    out.push(FileRec { path: fallback.to_string_lossy().into(), name: fallback.file_name().unwrap().to_string_lossy().into(), size: content.len() as u64, kind: "xuat".into() });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Báo cáo quý III\n\nDoanh thu **tăng** so với *cùng kỳ*.\n\n## Số liệu\n\n| Chỉ tiêu | Giá trị |\n|---|---|\n| Doanh thu | 1.250 |\n| Lợi nhuận | 320,5 |\n\n- Ý một\n- Ý hai\n  - ý con\n\n1. Bước đầu\n2. Bước sau\n\n> Ghi chú quan trọng\n\n```\nlet x = 1;\n```\n";

    #[test]
    fn parses_blocks() {
        let b = parse(SAMPLE);
        assert!(matches!(&b[0], Block::Heading(1, s) if plain(s) == "Báo cáo quý III"));
        assert!(b.iter().any(|x| matches!(x, Block::Table { head, rows } if head[0] == "Chỉ tiêu" && rows.len() == 2)));
        assert!(b.iter().any(|x| matches!(x, Block::List { ordered: false, items } if items.len() == 3)));
        assert!(b.iter().any(|x| matches!(x, Block::List { ordered: true, .. })));
        assert!(b.iter().any(|x| matches!(x, Block::Quote(_))));
        assert!(b.iter().any(|x| matches!(x, Block::Code(c) if c.contains("let x"))));
        let bold = b.iter().find_map(|x| if let Block::Para(s) = x { s.iter().find(|s| s.bold).cloned() } else { None }).unwrap();
        assert_eq!(bold.text, "tăng");
    }

    #[test]
    fn exports_every_format() {
        let dir = tempfile::tempdir().unwrap();
        for (ext, _) in FORMATS {
            let p = dir.path().join(format!("bao-cao.{ext}"));
            export(SAMPLE, "Báo cáo", ext, &p).unwrap_or_else(|e| panic!("{ext}: {e}"));
            let len = std::fs::metadata(&p).unwrap().len();
            assert!(len > 50, "{ext} quá nhỏ: {len}");
        }
        // Word có bảng thật và giữ tiếng Việt
        let f = std::fs::File::open(dir.path().join("bao-cao.docx")).unwrap();
        let mut z = zip::ZipArchive::new(f).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut xml).unwrap();
        assert!(xml.contains("<w:tbl>") && xml.contains("Báo cáo quý III") && xml.contains("Lợi nhuận"));
        assert!(std::fs::read(dir.path().join("bao-cao.pdf")).unwrap().starts_with(b"%PDF"));
        let csv = std::fs::read_to_string(dir.path().join("bao-cao.csv")).unwrap();
        assert!(csv.contains("Chỉ tiêu,Giá trị") && csv.contains("Doanh thu,1.250"));
    }

    #[test]
    fn numbers_in_tables() {
        assert_eq!(as_number("1.250"), Some(1250.0));
        assert_eq!(as_number("320,5"), Some(320.5));
        assert_eq!(as_number("1,250.75"), Some(1250.75));
        assert_eq!(as_number("12%"), Some(12.0));
        assert_eq!(as_number("Hà Nội"), None);
    }

    #[test]
    fn file_blocks_are_extracted_and_materialized() {
        let out = "Đây là kết quả.\n<<<TỆP: Báo cáo/quý 3.docx>>>\n```markdown\n# Tiêu đề\n\nNội dung\n```\n<<<HẾT TỆP>>>\nvà một bảng:\n<<<TỆP: so-lieu.xlsx>>>\n| A | B |\n|---|---|\n| 1 | 2 |\n<<<HẾT TỆP>>>\nXong.";
        let (files, cleaned) = extract_file_blocks(out);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].1, "# Tiêu đề\n\nNội dung");
        assert!(cleaned.contains("📄 Đã tạo tệp **Báo cáo-quý 3.docx**") && cleaned.contains("Xong.") && !cleaned.contains("<<<"));
        let dir = tempfile::tempdir().unwrap();
        let recs = materialize(&files, dir.path(), "Thử");
        assert_eq!(recs.len(), 2);
        assert!(recs.iter().all(|r| Path::new(&r.path).exists() && r.kind == "xuat"));
        // tạo lại cùng tên → không ghi đè
        let again = materialize(&files[..1], dir.path(), "Thử");
        assert!(again[0].name.contains("(2)"));
    }

    #[test]
    fn snapshot_diff_detects_new_and_changed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cu.txt"), "a").unwrap();
        let before = snapshot(dir.path());
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join("cu.txt"), "abc").unwrap();
        std::fs::write(dir.path().join("moi.md"), "x").unwrap();
        let d = diff(&before, &snapshot(dir.path()));
        assert_eq!(d.iter().map(|f| (f.name.as_str(), f.kind.as_str())).collect::<Vec<_>>(), vec![("cu.txt", "sua"), ("moi.md", "tao")]);
    }
}
