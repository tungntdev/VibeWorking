//! Thư viện mẫu nhân viên AI (từ cơ bản đến chuyên gia) và bộ chọn ứng viên ngoại tuyến.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub key: String,
    pub title: String,
    pub department: String,
    pub level: String,
    pub skills: Vec<String>,
    pub system_prompt: String,
    pub avatar: String,
    pub color: String,
    #[serde(skip)]
    pub keywords: Vec<String>,
}

/// Ứng viên do AI (hoặc bộ chọn ngoại tuyến) đề xuất khi tuyển dụng.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub department: String,
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub ai_reason: String,
    #[serde(default)]
    pub effort: String,
}

fn default_level() -> String {
    "trung_cap".into()
}

pub const LEVELS: [(&str, &str); 4] = [
    ("co_ban", "Cơ bản"),
    ("trung_cap", "Trung cấp"),
    ("chuyen_nghiep", "Chuyên nghiệp"),
    ("chuyen_gia", "Chuyên gia"),
];

pub fn normalize_level(l: &str) -> String {
    let n = fold(l);
    for (k, v) in LEVELS {
        if n == k || n == fold(v) || n.replace(' ', "_") == k {
            return k.into();
        }
    }
    match n.as_str() {
        "basic" | "junior" => "co_ban",
        "intermediate" | "mid" => "trung_cap",
        "professional" | "senior" | "pro" => "chuyen_nghiep",
        "expert" | "lead" | "principal" => "chuyen_gia",
        _ => "trung_cap",
    }
    .into()
}

/// Nhân vật 3D: bộ Kenney Mini Characters (character-*) và Blocky Characters (blocky-*).
pub const AVATARS: &[&str] = &[
    "character-female-a", "character-female-b", "character-female-c", "character-female-d",
    "character-female-e", "character-female-f", "character-male-a", "character-male-b",
    "character-male-c", "character-male-d", "character-male-e", "character-male-f",
    "character-male-g", "character-robot", "character-worker", "character-soldier",
    "character-knight", "character-alien",
    "blocky-a", "blocky-b", "blocky-c", "blocky-d", "blocky-e", "blocky-f", "blocky-g", "blocky-h", "blocky-i",
    "blocky-j", "blocky-k", "blocky-l", "blocky-m", "blocky-n", "blocky-o", "blocky-p", "blocky-q", "blocky-r",
];

pub fn valid_avatar(a: &str) -> bool {
    AVATARS.contains(&a)
}

const LEVEL_STYLE: &str = "Luôn trả lời bằng tiếng Việt, trình bày Markdown rõ ràng, đi thẳng vào kết quả, nêu giả định nếu thiếu thông tin.";

fn t(key: &str, title: &str, dept: &str, level: &str, skills: &[&str], role: &str, avatar: &str, color: &str, kw: &[&str]) -> Template {
    Template {
        key: key.into(),
        title: title.into(),
        department: dept.into(),
        level: level.into(),
        skills: skills.iter().map(|s| s.to_string()).collect(),
        system_prompt: format!("Bạn là {title} trong một văn phòng AI. {role} {LEVEL_STYLE}"),
        avatar: avatar.into(),
        color: color.into(),
        keywords: kw.iter().map(|s| fold(s)).collect(),
    }
}

pub fn dispatcher_template() -> Template {
    Template {
        key: "dieu_phoi".into(),
        title: "Điều phối viên".into(),
        department: "Ban điều hành".into(),
        level: "chuyen_gia".into(),
        skills: vec!["Phân rã công việc".into(), "Phân công".into(), "Tổng hợp báo cáo".into(), "Kiểm soát chất lượng".into()],
        system_prompt: format!(
            "Bạn là Điều phối viên của văn phòng AI: nhận nhiệm vụ từ Quản lý, phân rã thành đầu việc rõ ràng, \
             giao cho đúng nhân viên có chuyên môn phù hợp, theo dõi và tổng hợp kết quả thành báo cáo cuối. {LEVEL_STYLE}"
        ),
        avatar: "character-robot".into(),
        color: "#f59e0b".into(),
        keywords: vec![],
    }
}

pub fn library() -> Vec<Template> {
    vec![
        // ── Cơ bản
        t("thu_ky", "Thư ký hành chính", "Hành chính", "co_ban",
          &["Soạn email", "Lập lịch", "Ghi biên bản", "Sắp xếp tài liệu"],
          "Bạn hỗ trợ việc hành chính: soạn email, thư mời, biên bản họp, nhắc việc, lập danh sách.",
          "character-female-a", "#ec4899",
          &["thu ky", "hanh chinh", "email", "lich", "bien ban", "hop", "van thu", "cong van"]),
        t("cskh", "Nhân viên chăm sóc khách hàng", "Kinh doanh", "co_ban",
          &["Trả lời khách hàng", "Kịch bản tư vấn", "FAQ"],
          "Bạn soạn câu trả lời khách hàng lịch sự, kịch bản tư vấn, bộ câu hỏi thường gặp.",
          "character-female-b", "#14b8a6",
          &["khach hang", "cham soc", "tu van", "ho tro", "faq", "phan hoi", "ban hang"]),
        t("nhap_lieu", "Nhân viên nhập liệu & tổng hợp số liệu", "Hành chính", "co_ban",
          &["Chuẩn hoá dữ liệu", "Lập bảng", "Kiểm tra trùng lặp"],
          "Bạn chuẩn hoá, làm sạch và trình bày dữ liệu thành bảng gọn gàng.",
          "character-male-a", "#64748b",
          &["nhap lieu", "so lieu", "bang", "excel", "thong ke", "danh sach"]),
        // ── Trung cấp
        t("soan_thao", "Chuyên viên soạn thảo văn bản", "Văn phòng", "trung_cap",
          &["Văn bản hành chính", "Báo cáo", "Tờ trình", "Kế hoạch"],
          "Bạn soạn văn bản hành chính, báo cáo, tờ trình, kế hoạch đúng thể thức, văn phong chuẩn mực.",
          "character-female-c", "#8b5cf6",
          &["soan thao", "van ban", "bao cao", "to trinh", "ke hoach", "quyet dinh", "thong bao"]),
        t("content", "Chuyên viên nội dung & marketing", "Truyền thông", "trung_cap",
          &["Viết bài", "Mạng xã hội", "SEO", "Kịch bản video"],
          "Bạn viết nội dung truyền thông, bài mạng xã hội, bài chuẩn SEO, kịch bản video ngắn hấp dẫn.",
          "character-female-d", "#f97316",
          &["marketing", "noi dung", "content", "truyen thong", "mang xa hoi", "facebook", "seo", "quang cao", "bai viet"]),
        t("dich_thuat", "Biên dịch viên", "Văn phòng", "trung_cap",
          &["Dịch Anh–Việt", "Hiệu đính", "Thuật ngữ chuyên ngành"],
          "Bạn dịch và hiệu đính tài liệu Anh–Việt, Việt–Anh chính xác, giữ thuật ngữ chuyên ngành.",
          "character-male-b", "#06b6d4",
          &["dich", "tieng anh", "ngoai ngu", "bien dich", "translate"]),
        t("phan_tich_dl", "Chuyên viên phân tích dữ liệu", "Phân tích", "trung_cap",
          &["Thống kê", "SQL", "Python", "Trực quan hoá"],
          "Bạn phân tích dữ liệu, viết truy vấn SQL/Python, đưa ra nhận định có số liệu và đề xuất biểu đồ.",
          "character-male-c", "#3b82f6",
          &["du lieu", "phan tich", "data", "sql", "thong ke", "bao cao so", "dashboard", "chi so"]),
        t("tester", "Kỹ sư kiểm thử (QA)", "Công nghệ", "trung_cap",
          &["Test case", "Kiểm thử tự động", "Báo lỗi"],
          "Bạn thiết kế kịch bản kiểm thử, tìm lỗi, viết test tự động và báo cáo chất lượng.",
          "character-female-e", "#84cc16",
          &["kiem thu", "test", "qa", "chat luong", "loi", "bug"]),
        t("ke_toan", "Kế toán viên", "Tài chính", "trung_cap",
          &["Hạch toán", "Lập dự toán", "Đối chiếu chứng từ"],
          "Bạn lập dự toán, bảng chi phí, đối chiếu số liệu kế toán và giải thích rõ ràng.",
          "character-male-d", "#22c55e",
          &["ke toan", "tai chinh", "chi phi", "du toan", "ngan sach", "thue", "hoa don"]),
        t("nhan_su", "Chuyên viên nhân sự", "Nhân sự", "trung_cap",
          &["Mô tả công việc", "Tuyển dụng", "Đào tạo", "Đánh giá KPI"],
          "Bạn xây dựng mô tả công việc, quy trình tuyển dụng, chương trình đào tạo, bộ KPI.",
          "character-female-f", "#e11d48",
          &["nhan su", "tuyen dung", "dao tao", "kpi", "luong", "hr"]),
        // ── Chuyên nghiệp
        t("lap_trinh", "Lập trình viên Full-stack", "Công nghệ", "chuyen_nghiep",
          &["Rust", "TypeScript", "Python", "API", "Cơ sở dữ liệu"],
          "Bạn thiết kế và viết mã nguồn chất lượng cao, có giải thích, kèm hướng dẫn chạy và kiểm thử.",
          "character-male-e", "#6366f1",
          &["lap trinh", "code", "phan mem", "ung dung", "web", "app", "api", "rust", "python", "javascript", "lap trinh vien"]),
        t("thiet_ke", "Nhà thiết kế UI/UX", "Sáng tạo", "chuyen_nghiep",
          &["Wireframe", "Hệ thống thiết kế", "Trải nghiệm người dùng"],
          "Bạn thiết kế giao diện và trải nghiệm người dùng, mô tả bố cục, màu sắc, luồng thao tác cụ thể.",
          "character-female-a", "#d946ef",
          &["thiet ke", "giao dien", "ui", "ux", "do hoa", "logo", "nhan dien"]),
        t("phap_che", "Chuyên viên pháp chế", "Pháp chế", "chuyen_nghiep",
          &["Rà soát hợp đồng", "Tra cứu quy định", "Tư vấn tuân thủ"],
          "Bạn rà soát hợp đồng, văn bản pháp lý, chỉ ra rủi ro và căn cứ quy định liên quan (lưu ý người dùng cần kiểm chứng).",
          "character-male-f", "#78716c",
          &["phap che", "phap ly", "hop dong", "luat", "quy dinh", "tuan thu", "nghi dinh", "thong tu"]),
        t("nghien_cuu", "Nhà nghiên cứu", "Nghiên cứu", "chuyen_nghiep",
          &["Tổng quan tài liệu", "Phương pháp nghiên cứu", "Viết báo khoa học"],
          "Bạn tổng quan tài liệu, đề xuất phương pháp nghiên cứu, viết đề cương và bài báo khoa học có cấu trúc.",
          "character-male-g", "#0ea5e9",
          &["nghien cuu", "khoa hoc", "de tai", "luan van", "tai lieu", "tong quan", "bai bao"]),
        t("devops", "Kỹ sư DevOps", "Công nghệ", "chuyen_nghiep",
          &["CI/CD", "Docker", "Giám sát hệ thống", "Sao lưu"],
          "Bạn thiết kế hạ tầng triển khai, CI/CD, giám sát và sao lưu, kèm kịch bản cụ thể.",
          "character-worker", "#f59e0b",
          &["devops", "trien khai", "may chu", "server", "docker", "ha tang", "sao luu", "mang"]),
        t("du_an", "Quản lý dự án", "Ban điều hành", "chuyen_nghiep",
          &["Lập kế hoạch", "Quản lý rủi ro", "Theo dõi tiến độ"],
          "Bạn lập kế hoạch dự án, chia mốc, phân tích rủi ro, lập bảng theo dõi tiến độ.",
          "character-female-b", "#a855f7",
          &["du an", "tien do", "ke hoach", "moc", "rui ro", "quan ly"]),
        t("y_te", "Chuyên viên nghiệp vụ y tế", "Chuyên môn", "chuyen_nghiep",
          &["Quy trình chuyên môn", "Quản lý chất lượng bệnh viện", "Thống kê y tế"],
          "Bạn hỗ trợ nghiệp vụ y tế: quy trình chuyên môn, quản lý chất lượng, thống kê và báo cáo y tế (không thay thế chẩn đoán của bác sĩ).",
          "character-female-c", "#10b981",
          &["y te", "benh vien", "bac si", "dieu duong", "kham", "benh nhan", "chuyen mon", "kham chua benh"]),
        // ── Chuyên gia
        t("kien_truc", "Kiến trúc sư giải pháp", "Công nghệ", "chuyen_gia",
          &["Kiến trúc hệ thống", "Đánh giá công nghệ", "Bảo mật"],
          "Bạn thiết kế kiến trúc tổng thể, so sánh phương án công nghệ, đánh giá đánh đổi và rủi ro.",
          "character-male-a", "#4f46e5",
          &["kien truc", "he thong", "giai phap", "chuyen doi so", "tich hop", "cong nghe"]),
        t("bao_mat", "Chuyên gia an toàn thông tin", "Công nghệ", "chuyen_gia",
          &["Đánh giá rủi ro", "Rà soát mã nguồn", "Chính sách bảo mật"],
          "Bạn đánh giá rủi ro an toàn thông tin, rà soát cấu hình và mã nguồn, đề xuất biện pháp phòng vệ.",
          "character-soldier", "#dc2626",
          &["bao mat", "an toan thong tin", "an ninh mang", "security", "lo hong"]),
        t("chien_luoc", "Chuyên gia chiến lược", "Ban điều hành", "chuyen_gia",
          &["Phân tích SWOT", "Mô hình kinh doanh", "Lộ trình chiến lược"],
          "Bạn phân tích bối cảnh, xây dựng chiến lược, lộ trình và chỉ số đo lường thành công.",
          "character-knight", "#b45309",
          &["chien luoc", "kinh doanh", "swot", "tam nhin", "dinh huong", "thi truong"]),
    ]
}

/// Bỏ dấu tiếng Việt và viết thường để so khớp từ khoá.
pub fn fold(s: &str) -> String {
    const MAP: &[(&str, char)] = &[
        ("àáạảãâầấậẩẫăằắặẳẵ", 'a'), ("èéẹẻẽêềếệểễ", 'e'), ("ìíịỉĩ", 'i'),
        ("òóọỏõôồốộổỗơờớợởỡ", 'o'), ("ùúụủũưừứựửữ", 'u'), ("ỳýỵỷỹ", 'y'), ("đ", 'd'),
    ];
    s.to_lowercase()
        .chars()
        .map(|c| {
            for (set, r) in MAP {
                if set.contains(c) {
                    return *r;
                }
            }
            c
        })
        .collect()
}

const HO: [&str; 10] = ["Nguyễn", "Trần", "Lê", "Phạm", "Hoàng", "Vũ", "Đặng", "Bùi", "Đỗ", "Ngô"];
const TEN_NU: [&str; 12] = ["Lan", "Mai", "Hà", "Linh", "Trang", "Ngọc", "Thảo", "Hương", "Vy", "An", "Yến", "Chi"];
const TEN_NAM: [&str; 12] = ["Minh", "Hùng", "Nam", "Dũng", "Quân", "Phúc", "Huy", "Long", "Khôi", "Tuấn", "Bảo", "Sơn"];
const DEM_NU: [&str; 4] = ["Thị", "Ngọc", "Thu", "Minh"];
const DEM_NAM: [&str; 4] = ["Văn", "Đức", "Quang", "Minh"];

pub fn random_name(avatar: &str, seed: usize) -> String {
    let r = rand::random::<usize>().wrapping_add(seed * 7919);
    let female = avatar.contains("female") || (avatar.starts_with("blocky") && r % 2 == 0);
    let ho = HO[r % HO.len()];
    if avatar.contains("robot") || avatar.contains("alien") {
        return format!("AI-{}", 100 + r % 900);
    }
    if female {
        format!("{ho} {} {}", DEM_NU[(r / 7) % 4], TEN_NU[(r / 13) % TEN_NU.len()])
    } else {
        format!("{ho} {} {}", DEM_NAM[(r / 7) % 4], TEN_NAM[(r / 13) % TEN_NAM.len()])
    }
}

/// Chọn ứng viên theo từ khoá khi không có AI (hoặc AI trả về không hợp lệ).
pub fn offline_candidates(job: &str, count: usize) -> Vec<Candidate> {
    let j = fold(job);
    let mut scored: Vec<(usize, Template)> = library()
        .into_iter()
        .map(|t| {
            let s = t.keywords.iter().filter(|k| j.contains(k.as_str())).count() * 10
                + t.skills.iter().filter(|k| j.contains(&fold(k))).count() * 5;
            (s, t)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out: Vec<Candidate> = Vec::new();
    for (i, (score, t)) in scored.into_iter().enumerate() {
        if out.len() >= count {
            break;
        }
        out.push(Candidate {
            name: random_name(&t.avatar, i),
            title: t.title.clone(),
            department: t.department.clone(),
            level: t.level.clone(),
            skills: t.skills.clone(),
            system_prompt: t.system_prompt.clone(),
            avatar: t.avatar.clone(),
            color: t.color.clone(),
            reason: if score > 0 {
                format!("Khớp {} từ khoá trong mô tả công việc", score / 5)
            } else {
                "Vị trí nền tảng, hữu ích cho mọi văn phòng".into()
            },
            provider_id: String::new(),
            model: String::new(),
            ai_reason: String::new(),
            effort: String::new(),
        });
    }
    out
}

pub fn palette(i: usize) -> &'static str {
    const C: [&str; 10] = ["#6366f1", "#ec4899", "#14b8a6", "#f97316", "#3b82f6", "#84cc16", "#a855f7", "#ef4444", "#0ea5e9", "#eab308"];
    C[i % C.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_removes_diacritics() {
        assert_eq!(fold("Phân Tích Dữ Liệu Đầu"), "phan tich du lieu dau");
    }

    #[test]
    fn offline_picks_relevant_roles() {
        let c = offline_candidates("Cần đội viết phần mềm web, kiểm thử lỗi và triển khai lên máy chủ", 3);
        let titles: Vec<_> = c.iter().map(|x| x.title.as_str()).collect();
        assert!(titles.contains(&"Lập trình viên Full-stack"), "{titles:?}");
        assert!(titles.contains(&"Kỹ sư kiểm thử (QA)"), "{titles:?}");
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn offline_always_returns_something() {
        assert!(offline_candidates("xyz", 4).len() >= 2);
    }

    #[test]
    fn levels_normalize() {
        assert_eq!(normalize_level("Chuyên gia"), "chuyen_gia");
        assert_eq!(normalize_level("senior"), "chuyen_nghiep");
        assert_eq!(normalize_level("???"), "trung_cap");
    }

    #[test]
    fn library_avatars_valid() {
        for t in library() {
            assert!(valid_avatar(&t.avatar), "{}", t.avatar);
        }
    }
}
