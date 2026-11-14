// Cầu nối tới backend Rust (Tauri). Khi mở bằng trình duyệt thường sẽ dùng bản demo giả lập.

const T = window.__TAURI__;
export const isDesktop = !!(T && T.core);
let token = sessionStorage.getItem('aio_token') || '';

export function setToken(t) {
  token = t || '';
  try { t ? sessionStorage.setItem('aio_token', t) : sessionStorage.removeItem('aio_token'); } catch {}
}
export const getToken = () => token;

export async function call(cmd, args = {}) {
  const payload = { token, ...args };
  if (isDesktop) return T.core.invoke(cmd, payload);
  const { demoCall } = await import('./demo.js');
  return demoCall(cmd, payload);
}

export async function onEvent(cb) {
  if (isDesktop) return T.event.listen('office-event', (e) => cb(e.payload));
  const { demoListen } = await import('./demo.js');
  demoListen(cb);
}
