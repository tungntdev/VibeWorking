//! Băm mật khẩu (Argon2), phiên đăng nhập và phân quyền 3 cấp.

use anyhow::{anyhow, Result};
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::RngCore;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const ROLE_MANAGER: &str = "manager"; // Quản lý: toàn quyền
pub const ROLE_COORDINATOR: &str = "coordinator"; // Điều phối: giao việc, phân công
pub const ROLE_VIEWER: &str = "viewer"; // Quan sát: chỉ xem

pub fn role_rank(role: &str) -> u8 {
    match role {
        ROLE_MANAGER => 3,
        ROLE_COORDINATOR => 2,
        ROLE_VIEWER => 1,
        _ => 0,
    }
}

pub fn valid_role(role: &str) -> bool {
    role_rank(role) > 0
}

pub fn hash_password(pw: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow!("không băm được mật khẩu: {e}"))
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

pub fn check_password_policy(pw: &str) -> Result<()> {
    if pw.chars().count() < 6 {
        return Err(anyhow!("Mật khẩu phải có ít nhất 6 ký tự"));
    }
    Ok(())
}

pub fn check_username(u: &str) -> Result<()> {
    let ok = !u.is_empty()
        && u.len() <= 40
        && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    if !ok {
        return Err(anyhow!("Tên đăng nhập chỉ gồm chữ không dấu, số, dấu . _ - (tối đa 40 ký tự)"));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Session {
    pub user_id: i64,
    pub last_seen: Instant,
}

const SESSION_IDLE: Duration = Duration::from_secs(12 * 3600);
const MAX_FAILS: u32 = 5;
const LOCK_FOR: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Sessions {
    map: HashMap<String, Session>,
    fails: HashMap<String, (u32, Instant)>,
}

impl Sessions {
    pub fn create(&mut self, user_id: i64) -> String {
        let mut b = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut b);
        let token: String = b.iter().map(|x| format!("{x:02x}")).collect();
        self.map.insert(token.clone(), Session { user_id, last_seen: Instant::now() });
        token
    }

    pub fn touch(&mut self, token: &str) -> Option<i64> {
        let now = Instant::now();
        self.map.retain(|_, s| now.duration_since(s.last_seen) < SESSION_IDLE);
        let s = self.map.get_mut(token)?;
        s.last_seen = now;
        Some(s.user_id)
    }

    pub fn remove(&mut self, token: &str) {
        self.map.remove(token);
    }

    pub fn remove_user(&mut self, user_id: i64) {
        self.map.retain(|_, s| s.user_id != user_id);
    }

    /// Trả lỗi nếu tài khoản đang bị khoá tạm do nhập sai nhiều lần.
    pub fn check_lock(&mut self, username: &str) -> Result<()> {
        let key = username.to_lowercase();
        if let Some((n, t)) = self.fails.get(&key) {
            if *n >= MAX_FAILS {
                let left = LOCK_FOR.saturating_sub(t.elapsed());
                if left.is_zero() {
                    self.fails.remove(&key);
                } else {
                    return Err(anyhow!(
                        "Nhập sai quá nhiều lần. Thử lại sau {} giây",
                        left.as_secs().max(1)
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn record_fail(&mut self, username: &str) {
        let e = self.fails.entry(username.to_lowercase()).or_insert((0, Instant::now()));
        e.0 += 1;
        e.1 = Instant::now();
    }

    pub fn clear_fail(&mut self, username: &str) {
        self.fails.remove(&username.to_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let h = hash_password("bimat123").unwrap();
        assert!(verify_password("bimat123", &h));
        assert!(!verify_password("sai", &h));
        assert!(!verify_password("x", "rác"));
    }

    #[test]
    fn roles_ordered() {
        assert!(role_rank(ROLE_MANAGER) > role_rank(ROLE_COORDINATOR));
        assert!(role_rank(ROLE_COORDINATOR) > role_rank(ROLE_VIEWER));
        assert!(!valid_role("admin"));
    }

    #[test]
    fn lockout_after_failures() {
        let mut s = Sessions::default();
        for _ in 0..MAX_FAILS {
            assert!(s.check_lock("An").is_ok());
            s.record_fail("an");
        }
        assert!(s.check_lock("AN").is_err());
        s.clear_fail("an");
        assert!(s.check_lock("an").is_ok());
    }

    #[test]
    fn sessions_roundtrip() {
        let mut s = Sessions::default();
        let t = s.create(7);
        assert_eq!(s.touch(&t), Some(7));
        s.remove_user(7);
        assert_eq!(s.touch(&t), None);
    }

    #[test]
    fn username_rules() {
        assert!(check_username("quan.ly_01").is_ok());
        assert!(check_username("quản lý").is_err());
        assert!(check_password_policy("12345").is_err());
    }
}
