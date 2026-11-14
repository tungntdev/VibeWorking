//! Xác thực bằng chủ sở hữu máy (Touch ID hoặc mật khẩu đăng nhập macOS) qua LocalAuthentication.
//! Dùng khi quên mật khẩu ứng dụng: chứng minh là người dùng máy tính → đặt lại mật khẩu / đăng nhập.

use anyhow::{anyhow, Result};

/// Máy có hỗ trợ xác thực chủ sở hữu (mật khẩu máy / Touch ID) không, kèm mô tả.
pub fn available() -> (bool, String) {
    imp::available()
}

/// Hiện hộp thoại hệ thống và chờ người dùng xác thực (chặn luồng hiện tại, tối đa 3 phút).
pub fn authenticate(reason: &str) -> Result<()> {
    imp::authenticate(reason)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;

    pub fn available() -> (bool, String) {
        let ctx = unsafe { LAContext::new() };
        match unsafe { ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication) } {
            Ok(()) => (true, "Touch ID hoặc mật khẩu đăng nhập máy tính".into()),
            Err(e) => (false, e.localizedDescription().to_string()),
        }
    }

    pub fn authenticate(reason: &str) -> Result<()> {
        let ctx = unsafe { LAContext::new() };
        unsafe { ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication) }
            .map_err(|e| anyhow!("Máy không hỗ trợ xác thực: {}", e.localizedDescription()))?;
        let (tx, rx) = mpsc::channel::<std::result::Result<(), String>>();
        let tx = Mutex::new(Some(tx));
        let block = RcBlock::new(move |ok: Bool, err: *mut NSError| {
            let res = if ok.as_bool() {
                Ok(())
            } else {
                Err(unsafe { err.as_ref() }
                    .map(|e| e.localizedDescription().to_string())
                    .unwrap_or_else(|| "Xác thực không thành công".into()))
            };
            if let Some(t) = tx.lock().ok().and_then(|mut g| g.take()) {
                let _ = t.send(res);
            }
        });
        unsafe {
            ctx.evaluatePolicy_localizedReason_reply(LAPolicy::DeviceOwnerAuthentication, &NSString::from_str(reason), &block);
        }
        let r = rx
            .recv_timeout(Duration::from_secs(180))
            .map_err(|_| anyhow!("Hết thời gian chờ xác thực"))?;
        drop(ctx);
        r.map_err(|e| anyhow!("Xác thực máy tính không thành công: {e}"))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;
    pub fn available() -> (bool, String) {
        (false, "Chỉ hỗ trợ trên macOS".into())
    }
    pub fn authenticate(_reason: &str) -> Result<()> {
        Err(anyhow!("Xác thực bằng mật khẩu máy hiện chỉ hỗ trợ trên macOS"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn availability_query_does_not_prompt() {
        // chỉ hỏi khả năng, không hiện hộp thoại
        let (ok, msg) = super::available();
        println!("xác thực máy: {ok} — {msg}");
        assert!(!msg.is_empty());
    }
}
