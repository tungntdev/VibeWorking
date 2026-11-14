// Cảnh văn phòng 3D: dựng phòng từ bộ mô hình Kenney (Furniture Kit + Mini/Blocky Characters),
// đặt mỗi nhân viên AI vào một bàn làm việc, diễn hoạt trạng thái và di chuyển có tránh vật cản.

import * as THREE from '../vendor/three/three.module.js';
import { GLTFLoader } from '../vendor/three/addons/loaders/GLTFLoader.js';
import { OrbitControls } from '../vendor/three/addons/controls/OrbitControls.js';
import { CSS2DRenderer, CSS2DObject } from '../vendor/three/addons/renderers/CSS2DRenderer.js';
import * as SkeletonUtils from '../vendor/three/addons/utils/SkeletonUtils.js';
import { esc } from './md.js';
import { themeOf, paletteFor, DEFAULT_THEME } from './themes.js';

const FURN = 'assets/furniture/';
const CHAR = 'assets/characters/';
const COLS = 4; // số bàn mỗi dãy
const SX = 1.35; // khoảng cách bàn theo trục X
const SZ = 1.75; // khoảng cách giữa các dãy
const ROW0 = 0.75; // vị trí dãy bàn đầu tiên
const DISP = new THREE.Vector3(0, 0, -1.55); // vị trí điều phối viên
const WALK_SPEED = 1.1;
const CHAR_HEIGHT = 0.6; // chiều cao chuẩn hoá cho mọi bộ nhân vật
const CELL = 0.15; // ô lưới dẫn đường
const BODY = 0.17; // bán kính thân người khi tính va chạm với đồ đạc
const PERSONAL = 0.38; // khoảng cách tối thiểu giữa hai người đang đi
const STATUS_TEXT = { working: 'Đang làm việc', thinking: 'Đang suy nghĩ', done: 'Hoàn thành!', error: 'Gặp lỗi', walking: 'Đang di chuyển', asking: 'Đang hỏi bạn' };
const VISIT_SLOTS = [0, -0.45, 0.45, -0.9, 0.9, -1.35, 1.35];
const SEAT_TOP = 0.233; // độ cao mặt nệm ghế chairDesk
const SEAT_Z = -0.1; // tâm mặt nệm so với vị trí ngồi (ghế đặt lùi 0.12)

const loader = new GLTFLoader();
const cache = new Map();
function load(url) {
  if (!cache.has(url)) cache.set(url, loader.loadAsync(url));
  return cache.get(url);
}

// ───────────────────────────── Vật liệu theo phong cách ─────────────────────────────
const matCache = new Map();
function recolor(obj, map) {
  if (!map) return;
  obj.traverse((o) => {
    if (!o.isMesh) return;
    const mats = Array.isArray(o.material) ? o.material : [o.material];
    const out = mats.map((m) => {
      const col = map[m.name];
      if (!col) return m;
      const key = m.uuid + col;
      if (!matCache.has(key)) {
        const c = m.clone();
        c.color = new THREE.Color(col);
        matCache.set(key, c);
      }
      return matCache.get(key);
    });
    o.material = Array.isArray(o.material) ? out : out[0];
  });
}

/** Bọc mô hình nội thất: đặt tâm đáy về gốc toạ độ, đổi màu theo phong cách. */
async function furniture(name, palette) {
  const g = await load(FURN + name + '.glb');
  if (!g.__prepped) {
    const box = new THREE.Box3().setFromObject(g.scene);
    const c = box.getCenter(new THREE.Vector3());
    g.scene.position.set(-c.x, -box.min.y, -c.z);
    g.scene.traverse((o) => { if (o.isMesh) { o.castShadow = true; o.receiveShadow = true; } });
    g.__size = box.getSize(new THREE.Vector3());
    g.__prepped = true;
  }
  const wrap = new THREE.Group();
  const inner = g.scene.clone(true);
  recolor(inner, palette);
  wrap.add(inner);
  wrap.userData.size = g.__size.clone();
  return wrap;
}

function makeRing(color) {
  const m = new THREE.Mesh(
    new THREE.RingGeometry(0.36, 0.45, 40),
    new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.0, side: THREE.DoubleSide, depthWrite: false })
  );
  m.rotation.x = -Math.PI / 2;
  m.position.y = 0.03;
  return m;
}

// ───────────────────────────── Lưới dẫn đường + A* ─────────────────────────────
class NavGrid {
  constructor(b) {
    this.x0 = b.xMin;
    this.z0 = b.zMin;
    this.w = Math.ceil((b.xMax - b.xMin) / CELL);
    this.h = Math.ceil((b.zMax - b.zMin) / CELL);
    this.blocked = new Uint8Array(this.w * this.h);
    // tường bao quanh
    for (let i = 0; i < this.w; i++) for (let j = 0; j < this.h; j++) {
      const [x, z] = this.toWorld(i, j);
      if (x < b.xMin + 0.28 || z < b.zMin + 0.3 || x > b.xMax - 0.2 || z > b.zMax - 0.2) this.blocked[j * this.w + i] = 1;
    }
  }
  toCell(x, z) { return [Math.floor((x - this.x0) / CELL), Math.floor((z - this.z0) / CELL)]; }
  toWorld(i, j) { return [this.x0 + (i + 0.5) * CELL, this.z0 + (j + 0.5) * CELL]; }
  inside(i, j) { return i >= 0 && j >= 0 && i < this.w && j < this.h; }
  idx(i, j) { return j * this.w + i; }
  free(i, j, dyn) { return this.inside(i, j) && !this.blocked[this.idx(i, j)] && !(dyn && dyn.has(this.idx(i, j))); }
  blockBox(box, margin = BODY) {
    const [i0, j0] = this.toCell(box.min.x - margin, box.min.z - margin);
    const [i1, j1] = this.toCell(box.max.x + margin, box.max.z + margin);
    for (let i = Math.max(0, i0); i <= Math.min(this.w - 1, i1); i++)
      for (let j = Math.max(0, j0); j <= Math.min(this.h - 1, j1); j++) this.blocked[this.idx(i, j)] = 1;
  }
  isFreeAt(x, z, dyn) { const [i, j] = this.toCell(x, z); return this.free(i, j, dyn); }
  nearestFree(x, z, dyn) {
    const [ci, cj] = this.toCell(x, z);
    if (this.free(ci, cj, dyn)) return [ci, cj];
    for (let r = 1; r < 14; r++) {
      let best = null, bd = Infinity;
      for (let i = ci - r; i <= ci + r; i++) for (let j = cj - r; j <= cj + r; j++) {
        if (Math.max(Math.abs(i - ci), Math.abs(j - cj)) !== r || !this.free(i, j, dyn)) continue;
        const [wx, wz] = this.toWorld(i, j);
        const d = (wx - x) ** 2 + (wz - z) ** 2;
        if (d < bd) { bd = d; best = [i, j]; }
      }
      if (best) return best;
    }
    return null;
  }
  lineFree(a, b, dyn) {
    const d = Math.hypot(b.x - a.x, b.z - a.z);
    const n = Math.max(1, Math.ceil(d / (CELL * 0.5)));
    for (let k = 0; k <= n; k++) {
      const t = k / n;
      if (!this.isFreeAt(a.x + (b.x - a.x) * t, a.z + (b.z - a.z) * t, dyn)) return false;
    }
    return true;
  }
  /** A* 8 hướng, không cắt góc; trả về danh sách điểm đã làm mượt. */
  findPath(from, to, dyn) {
    const s = this.nearestFree(from.x, from.z, dyn);
    const g = this.nearestFree(to.x, to.z, dyn);
    if (!s || !g) return null;
    const N = this.w * this.h;
    const gScore = new Float32Array(N).fill(Infinity);
    const came = new Int32Array(N).fill(-1);
    const closed = new Uint8Array(N);
    const heap = [];
    const push = (f, k) => { heap.push([f, k]); let c = heap.length - 1; while (c > 0) { const p = (c - 1) >> 1; if (heap[p][0] <= heap[c][0]) break; [heap[p], heap[c]] = [heap[c], heap[p]]; c = p; } };
    const pop = () => { const top = heap[0]; const last = heap.pop(); if (heap.length) { heap[0] = last; let c = 0; for (;;) { const l = 2 * c + 1, r = l + 1; let m = c; if (l < heap.length && heap[l][0] < heap[m][0]) m = l; if (r < heap.length && heap[r][0] < heap[m][0]) m = r; if (m === c) break; [heap[m], heap[c]] = [heap[c], heap[m]]; c = m; } } return top; };
    const hfn = (i, j) => { const dx = Math.abs(i - g[0]), dz = Math.abs(j - g[1]); return Math.max(dx, dz) + 0.414 * Math.min(dx, dz); };
    const sk = this.idx(s[0], s[1]), gk = this.idx(g[0], g[1]);
    gScore[sk] = 0;
    push(hfn(s[0], s[1]), sk);
    let found = false;
    while (heap.length) {
      const [, k] = pop();
      if (closed[k]) continue;
      closed[k] = 1;
      if (k === gk) { found = true; break; }
      const i = k % this.w, j = (k / this.w) | 0;
      for (let di = -1; di <= 1; di++) for (let dj = -1; dj <= 1; dj++) {
        if (!di && !dj) continue;
        const ni = i + di, nj = j + dj;
        if (!this.free(ni, nj, dyn)) continue;
        if (di && dj && (!this.free(i + di, j, dyn) || !this.free(i, j + dj, dyn))) continue;
        const nk = this.idx(ni, nj);
        const cost = gScore[k] + (di && dj ? 1.414 : 1);
        if (cost < gScore[nk]) { gScore[nk] = cost; came[nk] = k; push(cost + hfn(ni, nj), nk); }
      }
    }
    if (!found) return null;
    const cells = [];
    for (let k = gk; k !== -1; k = came[k]) cells.push(k);
    cells.reverse();
    const pts = cells.map((k) => { const [x, z] = this.toWorld(k % this.w, (k / this.w) | 0); return new THREE.Vector3(x, 0, z); });
    pts.push(new THREE.Vector3(to.x, 0, to.z));
    // làm mượt: bỏ các điểm trung gian nếu nhìn thẳng được
    const out = [pts[0]];
    let a = 0;
    while (a < pts.length - 1) {
      let b = pts.length - 1;
      while (b > a + 1 && !this.lineFree(pts[a], pts[b], dyn)) b--;
      out.push(pts[b]);
      a = b;
    }
    return out.slice(1);
  }
}

// ───────────────────────────── Nhân viên ─────────────────────────────
class Worker {
  constructor(office, agent, gltf) {
    this.office = office;
    this.agent = agent;
    this.root = new THREE.Group();
    this.gltf = gltf;
    this.sitT = 0;
    this.model = SkeletonUtils.clone(gltf.scene);
    // chuẩn hoá chiều cao để các bộ nhân vật khác nhau đứng ngang nhau
    if (!gltf.__norm) {
      const box = new THREE.Box3().setFromObject(gltf.scene);
      const hgt = Math.max(0.1, box.max.y - Math.max(0, box.min.y));
      gltf.__norm = { s: CHAR_HEIGHT / hgt };
    }
    this.scale = gltf.__norm.s;
    this.model.scale.multiplyScalar(this.scale);
    this.model.traverse((o) => { if (o.isMesh) { o.castShadow = true; o.frustumCulled = false; } });
    this.root.add(this.model);
    this.mixer = new THREE.AnimationMixer(this.model);
    this.actions = {};
    for (const clip of gltf.animations) this.actions[clip.name] = this.mixer.clipAction(clip);
    this.current = null;
    this.queue = [];
    this.busy = false;
    this.status = 'idle';
    this.seat = new THREE.Vector3();
    this.seatRot = 0;
    this.sitting = false;
    this.route = null;
    this.waitT = 0;

    const hit = new THREE.Mesh(new THREE.CylinderGeometry(0.3, 0.3, 0.85, 10), new THREE.MeshBasicMaterial({ visible: false }));
    hit.position.y = 0.42;
    hit.userData.agentId = agent.id;
    this.root.add(hit);
    this.hit = hit;

    this.ring = makeRing(agent.color || '#7c83ff');
    office.scene.add(this.ring);

    this.tagEl = document.createElement('div');
    this.tagEl.className = 'tag' + (agent.is_dispatcher ? ' dispatcher' : '');
    this.tagEl.innerHTML = `<div class="bubble-slot"></div><div class="state idle"></div>`;
    this.tagEl.addEventListener('click', (e) => { e.stopPropagation(); office.onSelect?.(agent.id); });
    this.label = new CSS2DObject(this.tagEl);
    this.label.position.set(0, 0.84, 0);
    this.root.add(this.label);
    this.update(agent);
  }

  update(agent) {
    this.agent = agent;
    this.office.drawPlate(agent);
    this.applyStatus();
  }

  play(name, { once = false, fade = 0.25 } = {}) {
    const a = this.actions[name] || this.actions.idle;
    if (!a) return 0;
    if (this.current === a && !once) return a.getClip().duration;
    a.reset();
    a.setLoop(once ? THREE.LoopOnce : THREE.LoopRepeat, once ? 1 : Infinity);
    a.clampWhenFinished = once;
    a.enabled = true;
    a.fadeIn(fade).play();
    if (this.current && this.current !== a) this.current.fadeOut(fade);
    this.current = a;
    return a.getClip().duration;
  }

  sitDown() {
    this.root.position.copy(this.seat);
    this.root.rotation.y = this.seatRot;
    const off = this.gltf.__sit || this.office.sitOffset;
    this.model.position.set(0, off.y, off.z);
    this.sitT = 0;
    this.play('sit');
    this.sitting = true;
    this.applyStatus();
  }

  standUp() {
    this.model.position.set(0, 0, 0);
    this.sitting = false;
  }

  setStatus(status, note = '') {
    this.status = status;
    this.note = note;
    this.applyStatus();
    const st = this.tagEl.querySelector('.state');
    st.className = 'state ' + status;
    st.textContent = status === 'idle' ? '' : (STATUS_TEXT[status] || status) + (note && status !== 'done' ? ': ' + note : '');
    st.title = note || '';
    this.office.setScreen(this.agent.id, status === 'working' || status === 'thinking' ? (this.agent.color || '#7c83ff') : null);
    if (status === 'done') this.enqueue(() => this.celebrate(true));
    if (status === 'error') this.enqueue(() => this.celebrate(false));
  }

  applyStatus() {
    const s = this.status;
    this.ringTarget = s === 'working' || s === 'thinking' ? 0.85 : s === 'error' || s === 'asking' ? 0.9 : 0;
    this.ring.material.color.set(s === 'error' ? '#ef4444' : s === 'asking' ? '#fbbf24' : this.agent.color || '#7c83ff');
  }

  bubble(text, ms = 2600) {
    if (this.question) return; // đang hỏi: giữ bóng câu hỏi
    const slot = this.tagEl.querySelector('.bubble-slot');
    slot.innerHTML = `<div class="bubble">${esc(text)}</div>`;
    clearTimeout(this._bt);
    this._bt = setTimeout(() => { slot.innerHTML = ''; }, ms);
  }

  /** Đứng lên, ra trước bàn, quay về phía người xem và hỏi (giữ đến khi được trả lời). */
  async askQuestion(q) {
    if (this.pendingQ !== q) return; // đã được trả lời trước khi kịp đứng lên hỏi
    this.question = q;
    const slot = this.tagEl.querySelector('.bubble-slot');
    slot.innerHTML = `<div class="bubble ask"><b>❓ Hỏi bạn:</b> ${esc(q)}<span class="ask-cta">Bấm để trả lời</span></div>`;
    slot.querySelector('.bubble').addEventListener('click', (e) => { e.stopPropagation(); this.office.onQuestion?.(this.agent.id); });
    await this.leaveSeat();
    const front = new THREE.Vector3(this.seat.x, 0, this.seat.z + 1.0);
    if (this.question) await this.walkTo(front);
    if (this.question) {
      const cam = this.office.camera.position;
      this.face(new THREE.Vector3(cam.x, 0, cam.z));
      const d = this.play('interact-right', { once: true });
      await this.wait(Math.max(900, d * 1000));
      this.play('idle');
    }
    while (this.question) await this.wait(300);
    this.bubble('Cảm ơn, tôi làm tiếp!', 1800);
    await this.returnToSeat();
  }

  /** Đứng lên, ra khu trình bày phía trước phòng, quay về phía người xem và trình báo cáo. */
  async presentReport(title, onOpen) {
    const slot = this.tagEl.querySelector('.bubble-slot');
    const show = () => {
      slot.innerHTML = `<div class="bubble report"><b>📋 Báo cáo đã xong!</b><span>${esc(title)}</span><span class="ask-cta">Bấm để xem</span></div>`;
      slot.querySelector('.bubble').addEventListener('click', (e) => { e.stopPropagation(); this.presenting = false; onOpen?.(); });
    };
    this.presenting = true;
    show();
    await this.leaveSeat();
    const b = this.office.bounds;
    const spot = this.office.presentSpot(this);
    await this.walkTo(spot);
    const cam = this.office.camera.position;
    this.face(new THREE.Vector3(cam.x, 0, cam.z));
    let d = this.play('emote-yes', { once: true });
    await this.wait(Math.max(900, d * 1000));
    d = this.play('interact-right', { once: true });
    await this.wait(Math.max(900, d * 1000));
    this.play('idle');
    // đứng trình bày ~12 giây hoặc đến khi quản lý bấm xem
    for (let i = 0; i < 40 && this.presenting; i++) await this.wait(300);
    this.presenting = false;
    if (slot.querySelector('.bubble.report')) slot.innerHTML = '';
    void b;
    await this.returnToSeat();
  }

  answered() {
    this.pendingQ = null;
    this.question = null;
    const slot = this.tagEl.querySelector('.bubble-slot');
    if (slot.querySelector('.bubble.ask')) slot.innerHTML = '';
  }

  enqueue(fn) {
    this.queue.push(fn);
    if (!this.busy) this.next();
  }

  async next() {
    const fn = this.queue.shift();
    if (!fn) { this.busy = false; return; }
    this.busy = true;
    try { await fn(); } catch (e) { console.warn(e); }
    this.next();
  }

  wait(ms) { return new Promise((r) => setTimeout(r, ms)); }

  /** Điểm đứng phía sau ghế — nơi bắt đầu/kết thúc mọi chuyến đi. */
  approach() { return new THREE.Vector3(this.seat.x, 0, this.seat.z - 0.6); }

  /** Đi theo đường tìm được trên lưới; tránh đồ đạc và nhường đường người khác. */
  walkTo(goal) {
    this.standUp();
    this.play('walk');
    return new Promise((resolve) => {
      const plan = () => this.office.nav?.findPath(this.root.position, goal, this.office.dynamicBlocks(this, goal)) || [goal.clone()];
      this.route = { pts: plan(), i: 0, goal: goal.clone(), resolve, plan, replans: 0 };
    });
  }

  /** Đi thẳng một đoạn ngắn (ra/vào ghế) không qua lưới. */
  stepTo(p) {
    this.standUp();
    this.play('walk');
    return new Promise((resolve) => {
      this.route = { pts: [p.clone()], i: 0, goal: p.clone(), resolve, plan: null, replans: 0, direct: true };
    });
  }

  async leaveSeat() {
    if (!this.sitting) return;
    await this.stepTo(this.approach());
  }

  async returnToSeat() {
    await this.walkTo(this.approach());
    await this.stepTo(this.seat);
    this.sitDown();
  }

  face(target) {
    this.root.rotation.y = Math.atan2(target.x - this.root.position.x, target.z - this.root.position.z);
  }

  async celebrate(ok) {
    if (!this.sitting) return;
    this.standUp();
    const d = this.play(ok ? 'emote-yes' : 'emote-no', { once: true });
    if (ok) this.bubble('Xong rồi! ✔', 2200);
    await this.wait(Math.max(900, d * 1000));
    this.sitDown();
  }

  /** Đi tới bàn điều phối nhận việc rồi quay về chỗ. */
  async fetchTask(title) {
    const disp = this.office.dispatcher();
    if (!disp || disp === this) { this.bubble('Nhận việc: ' + title); return; }
    const slot = this.office.reserveSlot(this);
    await this.leaveSeat();
    await this.walkTo(slot);
    this.face(disp.root.position);
    disp.bubble('Giao: ' + title, 2400);
    const d = this.play('interact-right', { once: true });
    await this.wait(Math.max(1100, d * 1000));
    this.bubble('Đã rõ!', 1400);
    this.office.releaseSlot(this);
    await this.returnToSeat();
  }

  /** Sang bàn đồng nghiệp để trao đổi kết quả (khi việc của mình cần kết quả của họ). */
  async exchangeWith(other, title) {
    if (!other || other === this) return;
    const spot = this.office.besideDesk(other, this);
    await this.leaveSeat();
    await this.walkTo(spot);
    this.face(other.root.position);
    other.bubble('Kết quả của tôi đây', 2200);
    const d = this.play('interact-left', { once: true });
    await this.wait(Math.max(1300, d * 1000));
    this.bubble('Cảm ơn, tôi làm tiếp: ' + title, 2000);
    await this.returnToSeat();
  }

  blockedBy() {
    const me = this.root.position;
    const r = this.route;
    const target = r.pts[r.i];
    const dir = new THREE.Vector3(target.x - me.x, 0, target.z - me.z).normalize();
    for (const o of this.office.workers.values()) {
      if (o === this || o.sitting) continue;
      const v = new THREE.Vector3(o.root.position.x - me.x, 0, o.root.position.z - me.z);
      const d = v.length();
      if (d < PERSONAL && v.dot(dir) > 0) {
        // nhường đường: người đứng yên luôn chặn; hai người cùng đi thì ID nhỏ được đi trước
        if (!o.route || o.agent.id < this.agent.id) return o;
      }
    }
    return null;
  }

  /** Đo khớp hông khi đang ngồi để đặt nhân vật đúng lên mặt ghế (mỗi bộ nhân vật đo một lần). */
  calibrateSit() {
    let hip = null;
    this.model.traverse((b) => { if (!hip && /^(leg-left|leg\.l|leftupleg|thigh)/i.test(b.name)) hip = b; });
    if (!hip) { this.gltf.__sit = this.office.sitOffset.clone(); return; }
    const p = hip.getWorldPosition(new THREE.Vector3());
    const cur = this.model.position;
    const y = cur.y + (SEAT_TOP + 0.02 - p.y);
    const z = cur.z + (this.seat.z + SEAT_Z - p.z);
    this.gltf.__sit = new THREE.Vector3(0, Math.min(0.4, Math.max(0, y)), Math.min(0.15, Math.max(-0.25, z)));
    for (const wk of this.office.workers.values()) if (wk.gltf === this.gltf && wk.sitting) wk.model.position.copy(this.gltf.__sit);
  }

  tick(dt) {
    this.mixer.update(dt);
    if (this.sitting && !this.gltf.__sit) {
      this.sitT += dt;
      if (this.sitT > 0.45) this.calibrateSit();
    }
    const r = this.route;
    if (r) {
      const pos = this.root.position;
      const target = r.pts[r.i];
      const blocker = r.direct ? null : this.blockedBy();
      if (blocker) {
        this.waitT += dt;
        if (this.current !== this.actions.idle) this.play('idle', { fade: 0.15 });
        if (this.waitT > 0.9 && r.plan && r.replans < 6) {
          // chờ lâu → tìm đường vòng quanh người đang chắn
          r.replans++;
          r.pts = r.plan();
          r.i = 0;
          this.waitT = 0;
        }
      } else {
        if (this.waitT > 0) { this.waitT = 0; this.play('walk', { fade: 0.15 }); }
        const d = new THREE.Vector3(target.x - pos.x, 0, target.z - pos.z);
        const dist = d.length();
        const step = WALK_SPEED * dt;
        if (dist <= step) {
          pos.set(target.x, 0, target.z);
          r.i++;
          if (r.i >= r.pts.length) {
            this.route = null;
            this.play('idle');
            r.resolve();
          }
        } else {
          d.normalize();
          pos.addScaledVector(d, step);
          const want = Math.atan2(d.x, d.z);
          let diff = want - this.root.rotation.y;
          diff = Math.atan2(Math.sin(diff), Math.cos(diff));
          this.root.rotation.y += diff * Math.min(1, dt * 12);
        }
      }
    }
    const m = this.ring.material;
    const want = (this.ringTarget || 0) * (0.65 + 0.35 * Math.sin(performance.now() / 300));
    m.opacity += (want - m.opacity) * Math.min(1, dt * 6);
    this.ring.position.x = this.root.position.x;
    this.ring.position.z = this.root.position.z;
  }

  dispose() {
    this.office.scene.remove(this.root);
    this.office.scene.remove(this.ring);
    this.root.remove(this.label);
    this.tagEl.remove();
  }
}

// ───────────────────────────── Văn phòng ─────────────────────────────
export class Office {
  constructor(container, { onSelect } = {}) {
    this.container = container;
    this.onSelect = onSelect;
    this.workers = new Map();
    this.stations = new Map();
    this.screens = new Map();
    this.solids = new Set(); // đồ đạc chặn đường đi
    this.slots = new Map(); // vị trí trước bàn điều phối → người đang đứng
    this.plates = new Map(); // agentId → bảng tên trên bàn
    this.sitOffset = new THREE.Vector3(0, 0.24, -0.06); // mặc định trước khi đo
    this.clock = new THREE.Clock();
    this.rows = 0;
    this.themeKey = DEFAULT_THEME;
    this.theme = themeOf(DEFAULT_THEME);
    this.lastAgents = [];
  }

  async init(onProgress, themeKey) {
    if (themeKey) { this.themeKey = themeKey; this.theme = themeOf(themeKey); }
    const w = this.container.clientWidth, h = this.container.clientHeight;
    const r = (this.renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: 'high-performance' }));
    r.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    r.setSize(w, h);
    r.shadowMap.enabled = true;
    r.shadowMap.type = THREE.PCFSoftShadowMap;
    r.outputColorSpace = THREE.SRGBColorSpace;
    r.toneMapping = THREE.ACESFilmicToneMapping;
    this.container.appendChild(r.domElement);

    this.labels = new CSS2DRenderer();
    this.labels.setSize(w, h);
    Object.assign(this.labels.domElement.style, { position: 'absolute', inset: '0', pointerEvents: 'none' });
    this.labels.domElement.className = 'label-root';
    this.container.appendChild(this.labels.domElement);

    const scene = (this.scene = new THREE.Scene());
    scene.fog = new THREE.Fog('#141a33', 22, 45);
    this.camera = new THREE.PerspectiveCamera(38, w / h, 0.1, 200);
    this.camera.position.set(7.2, 7.4, 9.6);
    const c = (this.controls = new OrbitControls(this.camera, r.domElement));
    c.enableDamping = true;
    c.dampingFactor = 0.08;
    c.target.set(0.9, 0, 0.9);
    c.maxPolarAngle = Math.PI * 0.43;
    c.minDistance = 2.5;
    c.maxDistance = 24;
    c.screenSpacePanning = false;

    this.hemi = new THREE.HemisphereLight('#e8eeff', '#8a7c6a', 1.35);
    scene.add(this.hemi);
    const sun = (this.sun = new THREE.DirectionalLight('#fff3e0', 2.4));
    sun.castShadow = true;
    sun.shadow.mapSize.set(2048, 2048);
    sun.shadow.bias = -0.0004;
    sun.shadow.normalBias = 0.02;
    scene.add(sun, sun.target);
    const fill = new THREE.DirectionalLight('#a5b4fc', 0.5);
    fill.position.set(-10, 6, -6);
    scene.add(fill);
    this.applyLights();

    const furn = ['floorFull', 'wall', 'wallWindow', 'desk', 'chairDesk', 'computerScreen', 'computerKeyboard', 'computerMouse', 'laptop',
      'pottedPlant', 'plantSmall1', 'plantSmall2', 'bookcaseClosedWide', 'bookcaseOpen', 'bookcaseClosedDoors', 'loungeSofa', 'loungeDesignSofa', 'loungeChair', 'tableCoffee',
      'kitchenBar', 'kitchenCoffeeMachine', 'kitchenFridgeSmall', 'rugRectangle', 'rugRound', 'lampSquareFloor', 'lampRoundFloor', 'trashcan', 'coatRackStanding',
      'speaker', 'cardboardBoxClosed', 'tableRound', 'chair', 'radio'];
    let done = 0;
    const total = furn.length + 1;
    await Promise.all(furn.map((n) => load(FURN + n + '.glb').then(() => onProgress?.(++done / total))));
    await load(CHAR + 'character-male-a.glb').then(() => onProgress?.(++done / total));

    const desk = await furniture('desk');
    this.deskH = desk.userData.size.y;
    this.deskDepth = desk.userData.size.z;
    this.board = this.makeBoard();
    await this.buildRoom(1);

    const ray = new THREE.Raycaster();
    const ndc = new THREE.Vector2();
    let down = null;
    r.domElement.addEventListener('pointerdown', (e) => { down = [e.clientX, e.clientY]; });
    r.domElement.addEventListener('pointerup', (e) => {
      if (!down || Math.hypot(e.clientX - down[0], e.clientY - down[1]) > 5) return;
      const rect = r.domElement.getBoundingClientRect();
      ndc.set(((e.clientX - rect.left) / rect.width) * 2 - 1, -((e.clientY - rect.top) / rect.height) * 2 + 1);
      ray.setFromCamera(ndc, this.camera);
      const hits = ray.intersectObjects([...this.workers.values()].map((wk) => wk.hit).concat([...this.plates.values()].map((p) => p.mesh)), false);
      this.onSelect?.(hits[0] ? hits[0].object.userData.agentId : null);
    });
    window.addEventListener('resize', () => this.resize());
    this.loop();
  }

  applyLights() {
    const t = this.theme;
    this.scene.background = new THREE.Color(t.bg);
    this.scene.fog.color.set(t.bg);
    this.hemi.color.set(t.hemiSky);
    this.hemi.groundColor.set(t.hemiGround);
    this.hemi.intensity = t.hemi;
    this.sun.color.set(t.sun);
    this.sun.intensity = t.sunI;
    this.renderer.toneMappingExposure = t.exposure;
  }

  /** Đổi phong cách văn phòng: dựng lại phòng và bàn làm việc với bảng màu mới. */
  async setTheme(key) {
    if (key === this.themeKey && this.roomGroup) return;
    this.themeKey = key;
    this.theme = themeOf(key);
    this.applyLights();
    this.rows = 0;
    await this.buildRoom(Math.max(1, this.rows || 1));
    // dựng lại bàn ghế theo màu mới, giữ nguyên nhân viên
    for (const [id, st] of this.stations) { this.scene.remove(st); this.dropSolids(st); this.stations.delete(id); }
    this.plates.clear();
    for (const wk of this.workers.values()) { wk.dispose(); }
    this.workers.clear();
    this.screens.clear();
    await this.setAgents(this.lastAgents);
  }

  resize() {
    const w = this.container.clientWidth, h = this.container.clientHeight;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h);
    this.labels.setSize(w, h);
  }

  loop() {
    requestAnimationFrame(() => this.loop());
    const dt = Math.min(this.clock.getDelta(), 0.05);
    for (const wk of this.workers.values()) wk.tick(dt);
    if (this.neonLights) {
      const t = performance.now() / 1000;
      this.neonLights.forEach((l, i) => { l.intensity = 0.9 + 0.3 * Math.sin(t * 1.3 + i); });
    }
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
    this.labels.render(this.scene, this.camera);
  }

  deskPos(i) {
    const col = i % COLS, row = Math.floor(i / COLS);
    return new THREE.Vector3((col - (COLS - 1) / 2) * SX, 0, ROW0 + row * SZ);
  }

  /** Đặt một món đồ. role: furn | floor | floorAlt | wall | none; solid: có chặn đường đi không. */
  async add(name, x, z, { rot = 0, parent = this.roomGroup, y = 0, scale = 1, role = 'furn', solid = true } = {}) {
    const o = await furniture(name, role === 'none' ? null : paletteFor(this.theme, role));
    o.position.set(x, y, z);
    o.rotation.y = rot;
    o.scale.setScalar(scale);
    parent.add(o);
    if (solid) { o.userData.solid = true; this.solids.add(o); }
    return o;
  }

  dropSolids(group) {
    group.traverse((o) => this.solids.delete(o));
  }

  /** Tính lại lưới dẫn đường từ toàn bộ đồ đạc. */
  rebuildNav() {
    if (!this.bounds) return;
    const nav = new NavGrid(this.bounds);
    for (const o of this.solids) {
      if (!o.parent) continue;
      o.updateWorldMatrix(true, true);
      nav.blockBox(new THREE.Box3().setFromObject(o));
    }
    this.nav = nav;
  }

  /** Người khác đang đứng/đi được coi là vật cản tạm thời khi tìm đường. */
  dynamicBlocks(me, goal) {
    if (!this.nav) return null;
    const set = new Set();
    const [gi, gj] = this.nav.toCell(goal.x, goal.z);
    for (const o of this.workers.values()) {
      if (o === me || o.sitting) continue;
      const p = o.route ? null : o.root.position; // chỉ người đứng yên
      if (!p) continue;
      const [ci, cj] = this.nav.toCell(p.x, p.z);
      const rr = Math.ceil(0.28 / CELL);
      for (let i = ci - rr; i <= ci + rr; i++) for (let j = cj - rr; j <= cj + rr; j++) {
        if (i === gi && j === gj) continue;
        if (this.nav.inside(i, j)) set.add(this.nav.idx(i, j));
      }
    }
    return set;
  }

  reserveSlot(w) {
    for (const dx of VISIT_SLOTS) {
      if (![...this.slots.values()].includes(dx) || this.slots.get(w) === dx) {
        this.slots.set(w, dx);
        return new THREE.Vector3(DISP.x + dx, 0, DISP.z + 1.28);
      }
    }
    return new THREE.Vector3(DISP.x, 0, DISP.z + 1.6);
  }

  releaseSlot(w) { this.slots.delete(w); }

  /** Chỗ đứng cạnh bàn đồng nghiệp (bên trái hoặc phải, chọn bên còn trống). */
  besideDesk(other, me) {
    const s = other.seat;
    const cands = [
      new THREE.Vector3(s.x - SX / 2, 0, s.z + 0.05),
      new THREE.Vector3(s.x + SX / 2, 0, s.z + 0.05),
      new THREE.Vector3(s.x, 0, s.z - 0.62),
    ];
    const taken = [...this.workers.values()].filter((o) => o !== me && !o.sitting).map((o) => o.root.position);
    for (const c of cands) {
      if (this.nav && !this.nav.isFreeAt(c.x, c.z)) continue;
      if (taken.some((p) => p.distanceTo(c) < PERSONAL)) continue;
      return c;
    }
    return cands[2];
  }

  /** Dựng sàn, tường, góc cà phê, kệ sách... theo số dãy bàn và phong cách. */
  async buildRoom(rows) {
    rows = Math.max(1, rows);
    if (this.roomGroup && this.rows === rows) return;
    this.rows = rows;
    if (this.roomGroup) { this.dropSolids(this.roomGroup); this.scene.remove(this.roomGroup); }
    if (this.neonLights) this.neonLights = null;
    const g = (this.roomGroup = new THREE.Group());
    this.scene.add(g);
    const t = this.theme;

    const xMin = -4, xMax = 6;
    const zMin = -3, zMax = Math.max(5, Math.ceil(ROW0 + (rows - 1) * SZ + 1.6));
    this.bounds = { xMin, xMax, zMin, zMax };

    const slab = new THREE.Mesh(
      new THREE.BoxGeometry(xMax - xMin + 0.6, 0.5, zMax - zMin + 0.6),
      new THREE.MeshStandardMaterial({ color: t.slab, roughness: 0.9 })
    );
    slab.position.set((xMin + xMax) / 2, -0.26, (zMin + zMax) / 2);
    slab.receiveShadow = true;
    g.add(slab);

    const opt = (o) => ({ parent: g, ...o });
    const tiles = [];
    for (let x = xMin; x < xMax; x++) for (let z = zMin; z < zMax; z++)
      tiles.push(this.add('floorFull', x + 0.5, z + 0.5, opt({ y: -0.04, role: (x + z) % 2 ? 'floorAlt' : 'floor', solid: false })));
    await Promise.all(tiles);
    const walls = [];
    for (let x = xMin; x < xMax; x++) walls.push(this.add((x - xMin) % 3 === 1 ? 'wallWindow' : 'wall', x + 0.5, zMin + 0.03, opt({ role: 'wall', solid: false })));
    for (let z = zMin; z < zMax; z++) walls.push(this.add((z - zMin) % 3 === 1 ? 'wallWindow' : 'wall', xMin + 0.03, z + 0.5, opt({ rot: Math.PI / 2, role: 'wall', solid: false })));
    await Promise.all(walls);

    const back = zMin + 0.2;
    await this.add('rugRectangle', DISP.x, DISP.z + 0.55, opt({ scale: 1.3, solid: false }));
    await this.add(t.bookcase, -2.15, back + 0.12, opt());
    await this.add(t.bookcase === 'bookcaseOpen' ? 'bookcaseClosedWide' : 'bookcaseOpen', -3.05, back + 0.12, opt());
    await this.add(t.bookcase, 2.15, back + 0.12, opt());
    await this.add(t.lamps ? 'lampRoundFloor' : 'lampSquareFloor', 1.35, back + 0.1, opt());
    await this.add('coatRackStanding', -3.65, 0.2, opt());
    await this.add('trashcan', -3.65, 1.0, opt());
    await this.add('cardboardBoxClosed', -3.55, back + 1.0, opt({ rot: 0.3 }));
    this.board.position.set(DISP.x, 0.72, zMin + 0.08);
    g.add(this.board);

    // góc cà phê
    await this.add('kitchenBar', 4.15, back + 0.15, opt());
    await this.add('kitchenBar', 4.75, back + 0.15, opt());
    await this.add('kitchenCoffeeMachine', 4.15, back + 0.12, opt({ y: 0.42, solid: false }));
    await this.add('kitchenFridgeSmall', 5.5, back + 0.15, opt());

    // góc thư giãn
    await this.add('rugRound', 4.75, 0.9, opt({ scale: 1.25, solid: false }));
    await this.add(t.sofa, 4.85, 0.05, opt());
    await this.add('loungeChair', 5.55, 1.1, opt({ rot: -Math.PI / 2 }));
    await this.add('tableCoffee', 4.75, 0.95, opt());
    await this.add('plantSmall2', 4.75, 0.95, opt({ y: 0.24, solid: false }));
    await this.add('speaker', 5.7, 2.0, opt({ rot: -Math.PI / 2 }));

    // bàn họp nhỏ
    const mz = zMax - 1.4;
    await this.add('rugRound', 4.7, mz, opt({ scale: 1.1, solid: false }));
    await this.add('tableRound', 4.7, mz, opt());
    for (const [dx, dz, rr] of [[0.55, 0, -Math.PI / 2], [-0.55, 0, Math.PI / 2], [0, 0.55, Math.PI], [0, -0.55, 0]])
      await this.add('chair', 4.7 + dx, mz + dz, opt({ rot: rr + Math.PI }));

    // cây xanh theo phong cách
    const plantSpots = [[5.6, -0.65], [5.6, zMax - 0.45], [-3.6, zMax - 0.5], [-3.65, -1.8], [3.3, back + 0.2], [-1.2, back + 0.2], [5.6, 3.0], [-3.65, 2.6]];
    for (const [px, pz] of plantSpots.slice(0, t.plants)) await this.add('pottedPlant', px, pz, opt());

    // dải đèn neon cho phong cách công nghệ
    if (t.neon) {
      this.neonLights = [];
      const mk = (col, x, z, w, rot) => {
        const m = new THREE.Mesh(new THREE.BoxGeometry(w, 0.03, 0.03), new THREE.MeshBasicMaterial({ color: col, toneMapped: false }));
        m.position.set(x, 1.22, z);
        m.rotation.y = rot;
        g.add(m);
      };
      mk(t.neon[0], (xMin + xMax) / 2, zMin + 0.1, xMax - xMin - 0.2, 0);
      mk(t.neon[1], xMin + 0.1, (zMin + zMax) / 2, zMax - zMin - 0.2, Math.PI / 2);
      mk(t.neon[2], (xMin + xMax) / 2, zMin + 0.1 + 0.001, xMax - xMin - 0.2, 0);
      for (const [i, [lx, lz]] of [[-2, -2], [2.5, -2], [0, 2.5], [4.5, 2]].entries()) {
        const l = new THREE.PointLight(t.neon[i % t.neon.length], 1, 6, 1.5);
        l.position.set(lx, 1.4, lz);
        g.add(l);
        this.neonLights.push(l);
      }
    }

    const s = this.sun.shadow.camera;
    const span = Math.max(xMax - xMin, zMax - zMin) / 2 + 2;
    Object.assign(s, { left: -span, right: span, top: span, bottom: -span, near: 1, far: 50 });
    s.updateProjectionMatrix();
    this.sun.target.position.set((xMin + xMax) / 2, 0, (zMin + zMax) / 2);
    this.sun.position.set(this.sun.target.position.x + 7, 16, this.sun.target.position.z + 9);
    this.rebuildNav();
  }

  /** Bàn làm việc của một nhân viên: bàn, màn hình, bàn phím, ghế. */
  async buildStation(agent) {
    const grp = new THREE.Group();
    const isD = agent.is_dispatcher;
    const pos = isD ? DISP.clone() : this.deskPos(agent.desk);
    grp.position.copy(pos);
    this.scene.add(grp);
    const o = (extra) => ({ parent: grp, ...extra });
    const k = isD ? 1.3 : 1;
    await this.add('desk', 0, 0.42 * k, o({ scale: k }));
    const top = this.deskH * k;
    const screen = await this.add('computerScreen', isD ? -0.18 : 0, 0.5 * k, o({ rot: Math.PI, y: top, solid: false }));
    await this.add('computerKeyboard', isD ? -0.18 : 0, 0.3 * k, o({ rot: Math.PI, y: top, solid: false }));
    await this.add('computerMouse', isD ? 0.05 : 0.2, 0.3 * k, o({ rot: Math.PI, y: top, solid: false }));
    if (isD) await this.add('laptop', 0.3, 0.45 * k, o({ rot: Math.PI * 0.9, y: top, solid: false }));
    if (!isD && agent.desk % 3 === 0) await this.add('plantSmall1', -0.27, 0.55, o({ y: top, solid: false }));
    if (!isD && agent.desk % 3 === 1) await this.add('radio', 0.27, 0.55, o({ rot: Math.PI, y: top, solid: false }));
    await this.add('chairDesk', 0, -0.12, o());
    // bảng tên gắn ở mặt trước bàn (hướng về phía người xem)
    const canvas = document.createElement('canvas');
    canvas.width = 560;
    canvas.height = 160;
    const tex = new THREE.CanvasTexture(canvas);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.anisotropy = 4;
    const plate = new THREE.Mesh(new THREE.PlaneGeometry(0.7 * k, 0.2 * k), new THREE.MeshBasicMaterial({ map: tex, toneMapped: false, transparent: true }));
    plate.position.set(0, 0.21 * k, 0.42 * k + this.deskDepth * k / 2 + 0.012);
    plate.userData.agentId = agent.id;
    grp.add(plate);
    this.plates.set(agent.id, { canvas, tex, mesh: plate });
    this.drawPlate(agent);
    screen.traverse((m) => { if (m.isMesh) m.material = m.material.clone(); });
    this.screens.set(agent.id, screen);
    this.stations.set(agent.id, grp);
    this.setScreen(agent.id, null);
    return { pos, seatRot: 0 };
  }

  /** Vẽ bảng tên: họ tên + chức danh, viền theo màu nhận diện. */
  drawPlate(agent, selected = this.selectedId === agent.id) {
    const pl = this.plates.get(agent.id);
    if (!pl) return;
    const c = pl.canvas.getContext('2d');
    const W = 560, H = 160;
    c.clearRect(0, 0, W, H);
    const r = 22;
    c.beginPath();
    c.roundRect(4, 4, W - 8, H - 8, r);
    c.fillStyle = selected ? '#1e1b4b' : '#0f172a';
    c.fill();
    c.lineWidth = selected ? 10 : 6;
    c.strokeStyle = selected ? '#fbbf24' : agent.color || '#7c83ff';
    c.stroke();
    c.fillStyle = agent.color || '#7c83ff';
    c.beginPath();
    c.arc(40, H / 2, 15, 0, Math.PI * 2);
    c.fill();
    const fit = (text, size, weight, maxW) => {
      let f = size;
      do { c.font = `${weight} ${f}px -apple-system, "Segoe UI", sans-serif`; f -= 2; } while (c.measureText(text).width > maxW && f > 14);
    };
    c.fillStyle = '#f8fafc';
    fit(agent.name, 58, 800, W - 92);
    c.fillText(agent.name, 68, 74);
    c.fillStyle = '#c7d2fe';
    fit(agent.title, 38, 600, W - 92);
    c.fillText(agent.title, 68, 124);
    pl.tex.needsUpdate = true;
  }

  setScreen(agentId, color) {
    const s = this.screens.get(agentId);
    if (!s) return;
    const base = this.theme.screenGlow;
    s.traverse((m) => {
      if (m.isMesh && m.material.emissive) {
        m.material.emissive.set(color || (base ? this.theme.neon[0] : '#000000'));
        m.material.emissiveIntensity = color ? 0.55 : base || 0;
      }
    });
  }

  dispatcher() {
    for (const wk of this.workers.values()) if (wk.agent.is_dispatcher) return wk;
    return null;
  }

  /** Đồng bộ danh sách nhân viên với cảnh 3D, chạy tuần tự để không tạo trùng. */
  setAgents(agents) {
    this.lastAgents = agents;
    this._sync = (this._sync || Promise.resolve()).then(() => this._setAgents(agents)).catch((e) => console.error(e));
    return this._sync;
  }

  async _setAgents(agents) {
    const staff = agents.filter((a) => !a.is_dispatcher);
    const maxDesk = staff.reduce((m, a) => Math.max(m, a.desk), -1);
    await this.buildRoom(Math.floor(Math.max(0, maxDesk) / COLS) + 1);
    const ids = new Set(agents.map((a) => a.id));
    let changed = false;
    for (const [id, wk] of this.workers) {
      const a = agents.find((x) => x.id === id);
      if (!ids.has(id) || (a && (a.avatar !== wk.avatarKey || a.desk !== wk.deskKey))) {
        wk.dispose();
        this.workers.delete(id);
        this.slots.delete(wk);
        const st = this.stations.get(id);
        if (st) { this.dropSolids(st); this.scene.remove(st); }
        this.stations.delete(id);
        this.screens.delete(id);
        this.plates.delete(id);
        changed = true;
      }
    }
    for (const a of agents) {
      const wk = this.workers.get(a.id);
      if (wk) { wk.update(a); continue; }
      const gltf = await load(CHAR + (a.avatar || 'character-male-a') + '.glb').catch(() => load(CHAR + 'character-male-a.glb'));
      if (this.workers.has(a.id)) continue;
      const { pos, seatRot } = await this.buildStation(a);
      const worker = new Worker(this, a, gltf);
      worker.avatarKey = a.avatar;
      worker.deskKey = a.desk;
      worker.seat.set(pos.x, 0, pos.z);
      worker.seatRot = seatRot;
      this.scene.add(worker.root);
      this.workers.set(a.id, worker);
      worker.sitDown();
      worker.setStatus(a.status || 'idle');
      changed = true;
    }
    if (changed) this.rebuildNav();
  }

  setStatus(id, status, note) {
    this.workers.get(id)?.setStatus(status, note);
  }

  /** Nhận việc: nhân viên đi tới bàn điều phối. */
  assign(fromId, toId, title) {
    const wk = this.workers.get(toId);
    if (!wk) return;
    wk.enqueue(() => wk.fetchTask(title || 'việc mới'));
  }

  /** Trao đổi: nhân viên `fromId` sang bàn đồng nghiệp `toId` lấy kết quả bước trước. */
  exchange(fromId, toId, title) {
    const wk = this.workers.get(fromId);
    const other = this.workers.get(toId);
    if (!wk || !other || wk === other) return;
    wk.enqueue(() => wk.exchangeWith(other, title || ''));
  }

  focus(id) {
    const wk = this.workers.get(id);
    if (!wk) return;
    const to = wk.root.position.clone();
    const from = this.controls.target.clone();
    const camOff = this.camera.position.clone().sub(from).setLength(9);
    const t0 = performance.now();
    const anim = () => {
      const k = Math.min(1, (performance.now() - t0) / 700);
      const e = 1 - Math.pow(1 - k, 3);
      this.controls.target.lerpVectors(from, to, e);
      this.camera.position.copy(this.controls.target).add(camOff);
      if (k < 1) requestAnimationFrame(anim);
    };
    anim();
  }

  resetView() {
    this.controls.target.set(0.9, 0, 0.9);
    this.camera.position.set(7.2, 7.4, 9.6);
  }

  highlight(id) {
    const prev = this.selectedId;
    this.selectedId = id;
    for (const [wid, wk] of this.workers) {
      wk.tagEl.classList.toggle('selected', wid === id);
      if (wid === id || wid === prev) this.drawPlate(wk.agent);
    }
  }

  /** Chỗ đứng trình bày: khoảng trống phía trước các dãy bàn, giữa phòng. */
  presentSpot(me) {
    const b = this.bounds;
    const z = Math.min(b.zMax - 0.7, ROW0 + (this.rows - 1) * SZ + 1.35);
    const cands = [0.9, 0.3, 1.5, -0.3, 2.1].map((x) => new THREE.Vector3(x, 0, z));
    for (const c of cands) {
      if (this.nav && !this.nav.isFreeAt(c.x, c.z)) continue;
      if ([...this.workers.values()].some((o) => o !== me && !o.sitting && o.root.position.distanceTo(c) < PERSONAL)) continue;
      return c;
    }
    return cands[0];
  }

  /** Người trình bày đứng lên báo cáo kết quả. */
  present(agentId, title, onOpen) {
    const wk = this.workers.get(agentId);
    if (!wk) return;
    wk.enqueue(() => wk.presentReport(title, onOpen));
  }

  /** Nhân viên đứng lên hỏi quản lý. */
  ask(agentId, question) {
    const wk = this.workers.get(agentId);
    if (!wk || wk.pendingQ === question) return;
    const already = !!wk.pendingQ;
    wk.pendingQ = question;
    if (already) { if (wk.question) wk.question = question; return; }
    wk.enqueue(() => wk.askQuestion(question));
  }

  answered(agentId) {
    this.workers.get(agentId)?.answered();
  }

  makeBoard() {
    const canvas = document.createElement('canvas');
    canvas.width = 1024;
    canvas.height = 384;
    this.boardCanvas = canvas;
    this.boardTex = new THREE.CanvasTexture(canvas);
    this.boardTex.colorSpace = THREE.SRGBColorSpace;
    const frame = new THREE.Mesh(new THREE.BoxGeometry(1.95, 0.74, 0.04), new THREE.MeshStandardMaterial({ color: '#111827', roughness: 0.4 }));
    const screen = new THREE.Mesh(new THREE.PlaneGeometry(1.88, 0.705), new THREE.MeshBasicMaterial({ map: this.boardTex, toneMapped: false }));
    screen.position.z = 0.03;
    frame.add(screen);
    this.setBoard({ office: 'Văn phòng AI', agents: 0, working: 0, active: 0, done: 0 });
    return frame;
  }

  setBoard({ office, agents, working, active, waiting, done }) {
    const c = this.boardCanvas.getContext('2d');
    const g = c.createLinearGradient(0, 0, 1024, 384);
    g.addColorStop(0, '#1e1b4b');
    g.addColorStop(1, '#0f172a');
    c.fillStyle = g;
    c.fillRect(0, 0, 1024, 384);
    c.fillStyle = '#c7d2fe';
    c.font = '600 46px -apple-system, "Segoe UI", sans-serif';
    c.fillText(office || 'Văn phòng AI', 40, 78);
    c.fillStyle = '#64748b';
    c.font = '500 26px -apple-system, "Segoe UI", sans-serif';
    c.fillText(new Date().toLocaleString('vi-VN'), 40, 120);
    const cards = [['Nhân viên', agents, '#a5b4fc'], ['Đang làm', working, '#60a5fa'], ['Đang chạy', active, '#fbbf24'], ['Chờ duyệt', waiting ?? 0, '#f472b6'], ['Hoàn thành', done, '#34d399']];
    cards.forEach(([label, v, col], i) => {
      const x = 40 + i * 192;
      c.fillStyle = 'rgba(255,255,255,0.06)';
      c.fillRect(x, 160, 172, 180);
      c.fillStyle = col;
      c.font = '700 76px -apple-system, "Segoe UI", sans-serif';
      c.fillText(String(v ?? 0), x + 20, 260);
      c.fillStyle = '#cbd5e1';
      c.font = '500 26px -apple-system, "Segoe UI", sans-serif';
      c.fillText(label, x + 20, 312);
    });
    this.boardTex.needsUpdate = true;
  }
}
