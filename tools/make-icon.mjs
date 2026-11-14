// Sinh icon ứng dụng (PNG RGBA 1024x1024) không cần thư viện ngoài.
import fs from 'fs'; import zlib from 'zlib';
const N = 1024, px = Buffer.alloc(N * N * 4);
const mix = (a, b, t) => a + (b - a) * t;
function set(x, y, r, g, b, a = 255) { const i = (y * N + x) * 4; const A = a / 255, B = px[i + 3] / 255;
  const oa = A + B * (1 - A); if (oa <= 0) return;
  px[i] = (r * A + px[i] * B * (1 - A)) / oa; px[i + 1] = (g * A + px[i + 1] * B * (1 - A)) / oa; px[i + 2] = (b * A + px[i + 2] * B * (1 - A)) / oa; px[i + 3] = oa * 255; }
function inRound(x, y, x0, y0, x1, y1, r) { const cx = Math.max(x0 + r, Math.min(x, x1 - r)), cy = Math.max(y0 + r, Math.min(y, y1 - r)); return (x - cx) ** 2 + (y - cy) ** 2 <= r * r; }
for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
  if (inRound(x, y, 80, 80, 944, 944, 200)) { const t = (x + y) / (2 * N);
    set(x, y, mix(36, 99, t), mix(52, 102, t), mix(140, 241, t)); }
}
// sàn văn phòng isometric
const iso = (u, v) => [512 + (u - v) * 230, 610 + (u + v) * 115 - 230];
function poly(pts, col, a = 255) { let mnx = 1e9, mxx = -1e9, mny = 1e9, mxy = -1e9; for (const [x, y] of pts) { mnx = Math.min(mnx, x); mxx = Math.max(mxx, x); mny = Math.min(mny, y); mxy = Math.max(mxy, y); }
  for (let y = Math.floor(mny); y <= mxy; y++) for (let x = Math.floor(mnx); x <= mxx; x++) { let ins = false;
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) { const [xi, yi] = pts[i], [xj, yj] = pts[j]; if ((yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi) ins = !ins; }
    if (ins && inRound(x, y, 80, 80, 944, 944, 200)) set(x, y, ...col, a); } }
poly([iso(0, 0), iso(1, 0), iso(1, 1), iso(0, 1)], [235, 240, 255], 235);
// bàn làm việc
const box = (u0, v0, u1, v1, h, top, l, r) => { const up = ([x, y]) => [x, y - h];
  poly([iso(u0, v1), iso(u1, v1), up(iso(u1, v1)), up(iso(u0, v1))], l); poly([iso(u1, v0), iso(u1, v1), up(iso(u1, v1)), up(iso(u1, v0))], r);
  poly([up(iso(u0, v0)), up(iso(u1, v0)), up(iso(u1, v1)), up(iso(u0, v1))], top); };
box(0.15, 0.2, 0.45, 0.8, 90, [255, 200, 87], [214, 150, 50], [190, 128, 40]);
box(0.6, 0.2, 0.9, 0.8, 90, [255, 200, 87], [214, 150, 50], [190, 128, 40]);
// "đầu" AI phát sáng
for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) { const d = Math.hypot(x - 512, y - 300); if (d < 120) set(x, y, 120, 255, 214, 255 * Math.min(1, (120 - d) / 6)); else if (d < 190) set(x, y, 120, 255, 214, 70 * (1 - (d - 120) / 70)); }
for (const ex of [470, 554]) for (let y = 270; y < 320; y++) for (let x = ex - 14; x < ex + 14; x++) set(x, y, 30, 40, 90);
function crc(b) { let c, t = []; for (let n = 0; n < 256; n++) { c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; } c = ~0; for (const x of b) c = t[(c ^ x) & 255] ^ (c >>> 8); return (~c) >>> 0; }
const chunk = (t, d) => { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); };
const raw = Buffer.alloc((N * 4 + 1) * N); for (let y = 0; y < N; y++) { raw[y * (N * 4 + 1)] = 0; px.copy(raw, y * (N * 4 + 1) + 1, y * N * 4, (y + 1) * N * 4); }
const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(N, 0); ihdr.writeUInt32BE(N, 4); ihdr[8] = 8; ihdr[9] = 6;
fs.writeFileSync(process.argv[2], Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]));
console.log('icon ->', process.argv[2]);
