//! Tạo tệp mẫu để xem bằng mắt. Chạy: OUT=/đường/dẫn cargo test --test export_sample -- --ignored
#[test]
#[ignore]
fn write_samples() {
    let out = std::path::PathBuf::from(std::env::var("OUT").unwrap());
    let md = "# Báo cáo hoạt động quý III/2026\n\nPhòng Kế hoạch tổng hợp báo cáo **kết quả** thực hiện nhiệm vụ trong quý, *số liệu sơ bộ*.\n\n## 1. Số liệu chính\n\n| Chỉ tiêu | Kế hoạch | Thực hiện | Tỷ lệ |\n|---|---|---|---|\n| Lượt khám ngoại trú | 120.000 | 126.450 | 105% |\n| Bệnh nhân nội trú | 18.500 | 17.980 | 97% |\n| Phẫu thuật | 6.200 | 6.540 | 105% |\n\n## 2. Nhận xét\n\n- Ngoại trú vượt kế hoạch nhờ mở rộng khám buổi tối.\n- Nội trú đạt 97%, cần rà soát công suất giường.\n\n## 3. Đề xuất\n\n1. Duy trì khám ngoài giờ.\n2. Điều phối giường giữa các khoa.\n\n> Số liệu cập nhật đến ngày 30/9/2026.\n";
    for ext in ["docx", "pdf", "xlsx"] {
        ai_office_lib::export::export(md, "Báo cáo quý III", ext, &out.join(format!("mau.{ext}"))).unwrap();
    }
}
