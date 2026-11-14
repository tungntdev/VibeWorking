//! Quét máy thật (chỉ đọc). Chạy: cargo test --test scan_local -- --ignored --nocapture
use ai_office_lib::{db, engine, providers, setup, state::Core, templates};

#[tokio::test]
#[ignore]
async fn scan_this_machine() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::new(db::open_memory().unwrap(), dir.path().into(), providers::resolve_path_env(), Box::new(|_| {}));
    engine::seed(&core).unwrap();
    let v = setup::scan(&core).await.unwrap();
    println!("tools: {}", v["tools"]);
    for p in v["providers"].as_array().unwrap() {
        println!("{:<28} {:<9} {} {}", p["name"].as_str().unwrap(), p["status"].as_str().unwrap(), p["version"].as_str().unwrap(), p["path"]);
    }
    let avail = setup::available(&core).await.unwrap();
    let mut c = templates::offline_candidates("lập trình, dịch tài liệu, thư ký hành chính, kiến trúc hệ thống", 4);
    engine::assign_ai(&mut c, &avail);
    for x in &c {
        println!("{} [{}] -> {} / {} :: {}", x.title, x.level, x.provider_id, x.model, x.ai_reason);
    }
}
