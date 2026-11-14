import { call, onEvent, setToken, getToken, isDesktop } from './api.js';
import { Office } from './office3d.js';
import { md, esc } from './md.js';
import { paintIcons, icon } from './icons.js';
import { THEMES } from './themes.js';

// ───────────────────────────── Nhãn tiếng Việt ─────────────────────────────
const ROLE = { manager: 'Quản lý', coordinator: 'Điều phối', viewer: 'Quan sát' };
const RANK = { viewer: 1, coordinator: 2, manager: 3 };
const MSTATUS = { moi: 'Mới', cho_tra_loi: 'Chờ bạn trả lời', cho_duyet: 'Chờ duyệt', dang_lap_ke_hoach: 'Đang lập kế hoạch', dang_thuc_hien: 'Đang thực hiện', dang_tong_hop: 'Đang tổng hợp', hoan_thanh: 'Hoàn thành', loi: 'Có lỗi', da_huy: 'Đã huỷ' };
const SSTATUS = { cho: 'Chờ', dang_lam: 'Đang làm', xong: 'Xong', loi: 'Lỗi', huy: 'Đã huỷ', hoi: 'Đang hỏi bạn' };
const PRIO = { thap: 'Thấp', trung_binh: 'Trung bình', cao: 'Cao', khan: 'Khẩn' };
const MODE = { auto: 'Tự động', review: 'AI đề xuất, duyệt', manual: 'Phân công tay', pipeline: 'Theo quy trình' };
const LEVEL = { co_ban: 'Cơ bản', trung_cap: 'Trung cấp', chuyen_nghiep: 'Chuyên nghiệp', chuyen_gia: 'Chuyên gia' };
const ASTATUS = { idle: 'Rảnh', working: 'Đang làm việc', thinking: 'Đang suy nghĩ', error: 'Gặp lỗi', done: 'Vừa xong', asking: 'Đang hỏi bạn' };
const KIND = { cli: 'Dòng lệnh (CLI)', anthropic: 'Claude API', openai: 'OpenAI-tương thích', gemini: 'Gemini API', mock: 'Mô phỏng' };
const ACTIVE = ['dang_lap_ke_hoach', 'dang_thuc_hien', 'dang_tong_hop'];

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const st = { user: null, agents: [], providers: [], missions: [], users: [], templates: null, view: 'office', missionId: null, selected: null, filter: 'all' };
const can = (role) => st.user && RANK[st.user.role] >= RANK[role];
const agentById = (id) => st.agents.find((a) => a.id === id);
const provName = (id) => st.providers.find((p) => p.id === id)?.name || id;
const initials = (n) => (n || '?').split(/\s+/).filter(Boolean).slice(-2).map((w) => w[0]).join('').toUpperCase();
const avatarImg = (a) => `assets/characters/previews/${a}.png`;

function toast(msg, kind = '') {
  const el = document.createElement('div');
  el.className = 'toast ' + kind;
  el.textContent = msg;
  $('#toasts').appendChild(el);
  setTimeout(() => el.remove(), kind === 'err' ? 6500 : 3500);
}
async function run(fn, okMsg) {
  try {
    const r = await fn();
    if (okMsg) toast(okMsg, 'ok');
    return r;
  } catch (e) {
    const msg = String(e?.message || e);
    toast(msg, 'err');
    if (/hết hạn|đăng nhập lại/.test(msg)) showAuth(false);
    throw e;
  }
}

// ───────────────────────────── Hộp thoại ─────────────────────────────
function modal(html, { wide = false, onMount } = {}) {
  return new Promise((resolve) => {
    const box = $('#modal-box');
    box.className = 'modal' + (wide ? ' wide' : '');
    box.innerHTML = html;
    $('#modal').classList.remove('hidden');
    paintIcons(box);
    const close = (v) => { $('#modal').classList.add('hidden'); box.innerHTML = ''; resolve(v); };
    box.querySelectorAll('[data-close]').forEach((b) => b.addEventListener('click', () => close(null)));
    onMount?.(box, close);
  });
}
function confirmBox(title, text, danger = true) {
  return modal(`<h3>${esc(title)}</h3><p class="muted">${esc(text)}</p>
    <div class="modal-actions"><button class="btn" data-close>Huỷ</button><button class="btn ${danger ? 'danger' : 'primary'}" id="ok">Đồng ý</button></div>`, {
    onMount: (b, close) => $('#ok', b).addEventListener('click', () => close(true)),
  });
}
document.addEventListener('click', (e) => {
  const z = e.target.closest('.tbl-zoom');
  if (!z) return;
  e.preventDefault();
  e.stopPropagation();
  const t = z.parentElement.querySelector('table');
  $('#zoom-body').innerHTML = `<div class="tbl-wrap">${t.outerHTML}</div>`;
  $('#zoom').classList.remove('hidden');
});
$('#zoom-close').addEventListener('click', () => $('#zoom').classList.add('hidden'));
$('#zoom').addEventListener('click', (e) => { if (e.target.id === 'zoom') $('#zoom').classList.add('hidden'); });
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && !$('#zoom').classList.contains('hidden')) { $('#zoom').classList.add('hidden'); e.stopImmediatePropagation(); }
}, true);
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && !$('#modal').classList.contains('hidden')) $('#modal-box [data-close]')?.click();
});

// ───────────────────────────── 3D ─────────────────────────────
const office = new Office($('#scene'), { onSelect: (id) => selectAgent(id) });
window.__office = office; // tiện gỡ lỗi từ DevTools

async function boot() {
  paintIcons();
  const loadingText = $('#loading p');
  const info = await call('app_info');
  st.style = info.office_style || 'hien_dai';
  await office.init((p) => { loadingText.textContent = `Đang dựng văn phòng 3D… ${Math.round(p * 100)}%`; }, st.style).catch((e) => {
    console.error(e);
    toast('Không dựng được cảnh 3D: ' + e.message, 'err');
  });
  $('#loading').classList.add('fade');
  setTimeout(() => $('#loading').remove(), 600);
  setupDragDrop();
  $('#office-name').textContent = info.office_name || 'Văn phòng AI';
  if (info.needs_setup) return showAuth(true);
  if (getToken()) {
    try {
      st.user = await call('me');
      return enterApp();
    } catch { setToken(''); }
  }
  showAuth(false);
}

// ───────────────────────────── Đăng nhập ─────────────────────────────
function showAuth(setup) {
  st.user = null;
  $('#app').classList.add('hidden');
  $('#auth').classList.remove('hidden');
  $('#auth-title').textContent = setup ? 'Khởi tạo văn phòng' : 'Đăng nhập';
  $('#auth-sub').textContent = setup ? 'Lần đầu sử dụng: tạo tài khoản Quản lý — người giao nhiệm vụ cho văn phòng.' : 'Chào mừng trở lại văn phòng AI của bạn.';
  $('#auth-name-row').classList.toggle('hidden', !setup);
  $('#auth-confirm-row').classList.toggle('hidden', !setup);
  $('#auth-style-row').classList.toggle('hidden', !setup);
  if (setup) {
    const host = $('#auth-styles');
    host.innerHTML = styleCards(st.style || 'hien_dai');
    bindStyleCards(host, (k) => { st.style = k; office.setTheme(k); });
  }
  $('#auth-btn').textContent = setup ? 'Tạo tài khoản và vào văn phòng' : 'Đăng nhập';
  $('#auth-form').dataset.setup = setup ? '1' : '';
  $('#auth-error').textContent = '';
  $('#auth-form').classList.remove('hidden');
  $('#forgot-box').classList.add('hidden');
  $('#forgot-row').classList.add('hidden');
  if (!setup) {
    call('os_auth_info').then((r) => {
      if (!r?.available) return;
      $('#os-method').textContent = r.method;
      $('#forgot-row').classList.remove('hidden');
    }).catch(() => {});
  }
  setTimeout(() => $('#auth-form [name=' + (setup ? 'display_name' : 'username') + ']').focus(), 50);
}

$('#auth-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  const setup = e.target.dataset.setup === '1';
  $('#auth-error').textContent = '';
  try {
    let r;
    if (setup) {
      if (f.get('password') !== f.get('confirm')) throw 'Mật khẩu nhập lại không khớp';
      r = await call('setup_manager', { username: f.get('username'), displayName: f.get('display_name'), password: f.get('password'), officeStyle: st.style || 'hien_dai' });
    } else {
      r = await call('login', { username: f.get('username'), password: f.get('password') });
    }
    setToken(r.token);
    st.user = r.user;
    e.target.reset();
    enterApp();
  } catch (err) {
    $('#auth-error').textContent = String(err?.message || err);
  }
});

// ── Quên mật khẩu: xác thực bằng máy tính (Touch ID / mật khẩu đăng nhập macOS)
$('#forgot-btn').addEventListener('click', () => {
  $('#auth-form').classList.add('hidden');
  $('#forgot-box').classList.remove('hidden');
  $('#fg-error').textContent = '';
  $('#fg-user').value = $('#auth-form [name=username]').value || '';
  setTimeout(() => $('#fg-user').focus(), 50);
});
$('#fg-back').addEventListener('click', () => {
  $('#forgot-box').classList.add('hidden');
  $('#auth-form').classList.remove('hidden');
});
async function forgotAction(btn, fn) {
  const user = $('#fg-user').value.trim();
  $('#fg-error').textContent = '';
  if (!user) { $('#fg-error').textContent = 'Hãy nhập tên đăng nhập'; return; }
  const old = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'Đang chờ xác thực của máy tính…';
  try {
    const r = await fn(user);
    setToken(r.token);
    st.user = r.user;
    $('#fg-new').value = $('#fg-again').value = '';
    $('#forgot-box').classList.add('hidden');
    $('#auth-form').classList.remove('hidden');
    enterApp();
    return r;
  } catch (err) {
    $('#fg-error').textContent = String(err?.message || err);
  } finally {
    btn.disabled = false;
    btn.textContent = old;
  }
}
$('#fg-login').addEventListener('click', (e) => forgotAction(e.currentTarget, (username) => call('os_login', { username })));
$('#fg-reset').addEventListener('click', (e) => {
  const pw = $('#fg-new').value, again = $('#fg-again').value;
  if (pw.length < 6) { $('#fg-error').textContent = 'Mật khẩu mới phải có ít nhất 6 ký tự'; return; }
  if (pw !== again) { $('#fg-error').textContent = 'Mật khẩu nhập lại không khớp'; return; }
  forgotAction(e.currentTarget, (username) => call('os_reset_password', { username, newPassword: pw })).then((r) => { if (r) toast('Đã đặt mật khẩu mới', 'ok'); });
});

let eventsBound = false;
async function enterApp() {
  $('#auth').classList.add('hidden');
  $('#forgot-box').classList.add('hidden');
  $('#app').classList.remove('hidden');
  $('#user-name').textContent = st.user.display_name;
  $('#user-role').textContent = ROLE[st.user.role];
  $('#user-initial').textContent = initials(st.user.display_name);
  $$('[data-min]').forEach((el) => el.classList.toggle('hidden', !can(el.dataset.min)));
  await Promise.all([loadAgents(), loadProviders(), loadMissions(), loadSettings()]);
  refreshStats();
  loadTicker();
  if (!eventsBound) {
    eventsBound = true;
    onEvent(handleEvent);
  }
  loadQuestions();
  // Lần đầu: trợ lý thiết lập (kết nối AI → mô tả công việc → sinh nhân viên)
  scanMachine()
    .then(() => {
      if (readyAIs().length) {
        // đã có AI kết nối → đánh dấu đã thiết lập, không hỏi lại nữa
        if (!st.settings?.onboarded && can('manager')) call('set_onboarded', { done: true }).then(() => { st.settings.onboarded = true; }).catch(() => {});
      } else if (can('manager') && !st.settings?.onboarded) {
        setupWizard().catch((e) => console.error(e));
      }
    })
    .catch(() => null);
}

$('#user-btn').addEventListener('click', (e) => { e.stopPropagation(); $('#user-dropdown').classList.toggle('hidden'); });
document.addEventListener('click', () => $('#user-dropdown').classList.add('hidden'));
$('#user-dropdown').addEventListener('click', async (e) => {
  const act = e.target.closest('button')?.dataset.act;
  if (act === 'logout') {
    await call('logout').catch(() => {});
    setToken('');
    closePanel();
    selectAgent(null);
    showAuth(false);
  }
  if (act === 'password') changePassword();
});

function changePassword() {
  modal(`<h3>Đổi mật khẩu</h3><form class="form" id="f">
    <label>Mật khẩu hiện tại<input type="password" name="old" required></label>
    <label>Mật khẩu mới (≥ 6 ký tự)<input type="password" name="new" required minlength="6"></label>
    <label>Nhập lại mật khẩu mới<input type="password" name="again" required></label>
    <div class="modal-actions"><button type="button" class="btn" data-close>Huỷ</button><button class="btn primary">Lưu</button></div></form>`, {
    onMount: (b, close) => $('#f', b).addEventListener('submit', async (e) => {
      e.preventDefault();
      const f = new FormData(e.target);
      if (f.get('new') !== f.get('again')) return toast('Mật khẩu nhập lại không khớp', 'err');
      await run(() => call('change_password', { oldPassword: f.get('old'), newPassword: f.get('new') }), 'Đã đổi mật khẩu');
      close(true);
    }),
  });
}

// ───────────────────────────── Dữ liệu ─────────────────────────────
async function loadAgents() {
  st.agents = await call('list_agents');
  await office.setAgents(st.agents);
  if (st.view === 'staff') renderStaff();
  if (st.selected) renderAgentCard(st.selected);
}
async function loadProviders() { st.providers = await call('list_providers'); }
async function loadMissions() {
  st.missions = await call('list_missions');
  if (st.view === 'missions' && !st.missionId) renderMissions();
}
async function loadSettings() {
  const s = await call('get_settings');
  st.settings = s;
  $('#office-name').textContent = s.office_name || 'Văn phòng AI';
}
async function refreshStats() {
  const d = await call('dashboard').catch(() => null);
  if (!d) return;
  const chip = (go, label, v, color, tip) => `<button class="chip" data-go="${go}" title="${tip}"><span class="dot" style="background:${color}"></span>${label} <b>${v}</b></button>`;
  $('#stats').innerHTML = chip('staff', 'Nhân viên', d.agents, '#a5b4fc', 'Xem danh sách nhân viên')
    + chip('working', 'Đang làm', d.working, '#60a5fa', 'Xem ai đang làm việc gì')
    + chip('active', 'Đang chạy', d.missions_active, '#fbbf24', 'Nhiệm vụ đang chạy')
    + chip('wait', 'Chờ duyệt', d.missions_waiting, '#f472b6', 'Nhiệm vụ chờ duyệt / chờ bạn trả lời')
    + chip('done', 'Hoàn thành', d.missions_done, '#34d399', 'Nhiệm vụ đã hoàn thành');
  office.setBoard({ office: st.settings?.office_name, agents: d.agents, working: d.working, active: d.missions_active, waiting: d.missions_waiting, done: d.missions_done });
}

// ───────────────────────────── Sự kiện thời gian thực ─────────────────────────────
let refreshTimer = null;
function scheduleRefresh() {
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(async () => {
    await loadMissions().catch(() => {});
    if (st.view === 'missions' && st.missionId) renderMissionDetail(st.missionId);
    refreshStats();
  }, 250);
}

function handleEvent(ev) {
  switch (ev.type) {
    case 'agent': {
      const a = agentById(ev.id);
      if (a) a.status = ev.status === 'done' ? 'idle' : ev.status;
      office.setStatus(ev.id, ev.status, ev.note);
      if (st.selected === ev.id) renderAgentCard(ev.id);
      if (st.view === 'staff') renderStaff();
      refreshStats();
      break;
    }
    case 'exchange':
      office.exchange(ev.from, ev.to, ev.title);
      break;
    case 'style':
      st.style = ev.style;
      office.setTheme(ev.style);
      break;
    case 'assign':
      office.assign(ev.from, ev.to, ev.title);
      break;
    case 'mission_done':
      onMissionDone(ev);
      break;
    case 'mission_failed':
      showDoneCard({ ...ev, failed: true });
      notifyOS('Nhiệm vụ gặp lỗi', `${ev.title}: ${String(ev.error).slice(0, 120)}`);
      break;
    case 'memory': {
      const a = agentById(ev.agent_id);
      toast(`🧠 ${a?.name || 'Nhân viên'} đã ghi nhớ${ev.office ? ' (chung)' : ''}: ${ev.content}`, 'ok');
      if (st.selected === ev.agent_id) loadMemories(ev.agent_id);
      break;
    }
    case 'files':
      if (st.missionId === ev.mission_id) scheduleRefresh();
      toast(`📄 ${ev.files.length} tệp mới: ${ev.files.slice(0, 3).map((f) => f.name).join(', ')}`, 'ok');
      break;
    case 'mission':
    case 'subtask':
      scheduleRefresh();
      break;
    case 'activity':
      pushTicker(ev.item);
      if (st.view === 'activity') renderActivity();
      break;
    case 'install':
      onInstallEvent(ev);
      break;
    case 'progress': {
      (st.live[ev.subtask_id] ||= []).push(ev.line);
      if (st.live[ev.subtask_id].length > 300) st.live[ev.subtask_id].splice(0, 100);
      document.querySelectorAll(`[data-live="${ev.subtask_id}"]`).forEach((el) => {
        const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
        el.insertAdjacentHTML('beforeend', `<div>${esc(ev.line)}</div>`);
        if (atBottom) el.scrollTop = el.scrollHeight;
      });
      break;
    }
    case 'question':
      st.questions = st.questions.filter((q) => q.subtask_id !== ev.subtask_id).concat([{ subtask_id: ev.subtask_id, agent_id: ev.agent_id, mission_id: ev.mission_id, question: ev.question, title: ev.title }]);
      office.ask(ev.agent_id, ev.question);
      renderQuestions();
      if (st.selected === ev.agent_id) renderAgentCard(ev.agent_id);
      break;
    case 'answered':
      st.questions = st.questions.filter((q) => q.subtask_id !== ev.subtask_id);
      if (ev.agent_id) office.answered(ev.agent_id);
      renderQuestions();
      break;
    case 'agents_changed':
      loadAgents().then(refreshStats);
      break;
  }
}

function pushTicker(item) {
  const el = document.createElement('div');
  el.className = 'tick';
  el.innerHTML = `<span class="t">${esc(item.ts.slice(11, 16))}</span><b>${esc(item.actor)}</b> ${esc(item.message)}`;
  const t = $('#ticker');
  t.prepend(el);
  while (t.children.length > 4) t.lastChild.remove();
  setTimeout(() => el.remove(), 15000);
}
async function loadTicker() {
  const items = await call('list_activity', { limit: 3 }).catch(() => []);
  $('#ticker').innerHTML = '';
  items.reverse().forEach(pushTicker);
}

// ───────────────────────────── Điều hướng ─────────────────────────────
const VIEWS = {
  missions: ['Nhiệm vụ', renderMissions, true],
  pipelines: ['Quy trình nhiều AI', renderPipelines, true],
  templates: ['Mẫu tài liệu & báo cáo', renderDocTemplates, true],
  staff: ['Nhân sự AI', renderStaff, false],
  recruit: ['Tuyển dụng nhân viên AI', renderRecruit, true],
  providers: ['Kết nối AI', renderProviders, true],
  users: ['Người dùng & phân quyền', renderUsers, false],
  activity: ['Nhật ký hoạt động', renderActivity, false],
  settings: ['Cài đặt văn phòng', renderSettings, false],
};
$('#stats').addEventListener('click', (e) => {
  const go = e.target.closest('[data-go]')?.dataset.go;
  if (!go) return;
  if (go === 'staff') return openView('staff');
  if (go === 'working') {
    st.staffFilter = 'working';
    openView('staff');
    const busy = st.agents.find((a) => a.status === 'working' || a.status === 'thinking' || a.status === 'asking');
    if (busy) { selectAgent(busy.id); office.focus(busy.id); } else toast('Hiện không có nhân viên nào đang làm việc');
    return;
  }
  st.filter = go; // active | wait | done
  openView('missions');
});

$('#sidenav').addEventListener('click', (e) => {
  const b = e.target.closest('button[data-view]');
  if (b) { st.staffFilter = 'all'; if (b.dataset.view === 'missions') st.filter = 'all'; openView(b.dataset.view); }
});
$('#panel-close').addEventListener('click', closePanel);
function closePanel() { openView('office'); }

function openView(view) {
  st.view = view;
  st.missionId = null;
  $$('#sidenav button').forEach((b) => b.classList.toggle('active', b.dataset.view === view));
  if (view === 'office') {
    $('#panel').classList.add('hidden');
    $('#hint').classList.remove('hidden');
    return;
  }
  const [title, render, wide] = VIEWS[view];
  $('#panel').classList.remove('hidden');
  $('#panel').classList.toggle('wide', wide);
  $('#hint').classList.add('hidden');
  $('#panel-title').textContent = title;
  $('#panel-body').innerHTML = '<div class="muted">Đang tải…</div>';
  render();
}
function body(html) {
  const b = $('#panel-body');
  b.innerHTML = html;
  paintIcons(b);
  return b;
}

// ───────────────────────────── Nhiệm vụ ─────────────────────────────
$('#btn-new-mission').addEventListener('click', () => newMission());
$('#btn-coffee').addEventListener('click', () => coffeeBox());

// ───────────────────────────── Mời cafe tác giả ─────────────────────────────
const DONATE = { bank: 'Techcombank', name: 'NGUYEN THANH TUNG', account: '1907 5010 0490 17', amount: '100.000 ₫', note: 'Gui tien cafe' };
function coffeeBox() {
  modal(`<div class="coffee-box">
      <h3>☕ Mời tác giả một ly cafe</h3>
      <p class="muted">Nếu AI Office 3D giúp ích cho công việc của bạn, bạn có thể ủng hộ người phát triển một ly cafe. Mở app ngân hàng bất kỳ và quét mã VietQR dưới đây.</p>
      <img class="coffee-qr" src="assets/donate/qr-cafe.png" alt="Mã VietQR chuyển khoản Techcombank">
      <div class="coffee-info">
        <div><span>Ngân hàng</span><b>${DONATE.bank}</b></div>
        <div><span>Chủ tài khoản</span><b>${DONATE.name}</b></div>
        <div><span>Số tài khoản</span><b>${DONATE.account}</b><button class="btn small" id="cp-acc">Sao chép</button></div>
        <div><span>Số tiền gợi ý</span><b>${DONATE.amount}</b></div>
        <div><span>Nội dung</span><b>${DONATE.note}</b></div>
      </div>
      <p class="hint-text">Mã QR đã điền sẵn 100.000 ₫. Bạn có thể sửa số tiền trong app ngân hàng, hoặc chuyển khoản theo số tài khoản ở trên. Cảm ơn bạn rất nhiều! ❤️</p>
      <div class="modal-actions"><button class="btn primary" data-close>Đóng</button></div>
    </div>`, {
    onMount: (b) => $('#cp-acc', b).addEventListener('click', async () => {
      const v = DONATE.account.replace(/\s/g, '');
      try { await navigator.clipboard.writeText(v); } catch {
        const t = document.createElement('textarea'); t.value = v; document.body.append(t); t.select(); document.execCommand('copy'); t.remove();
      }
      toast('Đã sao chép số tài khoản ' + v, 'ok');
    }),
  });
}

function agentChip(id) {
  const a = agentById(id);
  if (!a) return '<span class="muted">(chưa giao)</span>';
  return `<span class="badge" style="background:${a.color}33;color:#fff">${esc(a.name)}</span>`;
}

function renderMissions() {
  if (st.view !== 'missions' || st.missionId) return;
  const filters = { all: 'Tất cả', active: 'Đang chạy', wait: 'Chờ duyệt', done: 'Hoàn thành', err: 'Lỗi / huỷ' };
  const pass = (m) => st.filter === 'all' || (st.filter === 'active' && (m.running || ACTIVE.includes(m.status))) || (st.filter === 'wait' && ['moi', 'cho_duyet'].includes(m.status)) || (st.filter === 'done' && m.status === 'hoan_thanh') || (st.filter === 'err' && ['loi', 'da_huy'].includes(m.status));
  const list = st.missions.filter(pass);
  const b = body(`
    <div class="toolbar">
      ${Object.entries(filters).map(([k, v]) => `<button class="btn small ${st.filter === k ? 'primary' : ''}" data-f="${k}">${v}</button>`).join('')}
      <span class="grow"></span>
      ${can('coordinator') ? `<button class="btn small primary" id="nm"><i data-icon="plus"></i>Giao nhiệm vụ</button>` : ''}
    </div>
    <div class="list">${list.length ? list.map((m) => `
      <div class="item" data-id="${m.id}">
        <div class="item-row">
          <div class="grow"><h4>${esc(m.title)}</h4>
            <p>#${m.id} · ${esc(m.created_at.slice(0, 16))} · ${MODE[m.mode] || m.mode}${m.deadline ? ' · hạn ' + esc(m.deadline) : ''}</p></div>
          <span class="badge p-${m.priority}">${PRIO[m.priority]}</span>
          <span class="badge ${m.status}">${MSTATUS[m.status] || m.status}</span>
        </div>
        ${m.total ? `<div class="progress"><div style="width:${Math.round((m.done / m.total) * 100)}%"></div></div>
          <p>${m.done}/${m.total} đầu việc · ${[...new Set(m.agent_ids)].map(agentChip).join(' ')}</p>` : ''}
      </div>`).join('') : `<div class="empty"><b>Chưa có nhiệm vụ</b>Bấm "Giao nhiệm vụ" để giao việc cho văn phòng AI.</div>`}
    </div>`);
  $$('[data-f]', b).forEach((x) => x.addEventListener('click', () => { st.filter = x.dataset.f; renderMissions(); }));
  $('#nm', b)?.addEventListener('click', () => newMission());
  $$('.item[data-id]', b).forEach((x) => x.addEventListener('click', () => openMission(+x.dataset.id)));
}

function openMission(id) {
  if (st.view !== 'missions') openView('missions');
  st.missionId = id;
  renderMissionDetail(id);
}

async function renderMissionDetail(id) {
  const data = await call('get_mission', { id }).catch((e) => { toast(String(e), 'err'); return null; });
  if (!data || st.missionId !== id) return;
  const { mission: m, subtasks: subs, running } = data;
  const busy = running || ACTIVE.includes(m.status);
  const coord = can('coordinator');
  const staff = st.agents;
  const subHtml = subs.map((s) => {
    const a = agentById(s.agent_id);
    return `<div class="sub ${s.status}">
      <div class="sub-head"><span class="step">B${s.step}</span>
        <div class="grow"><b>${esc(s.title)}</b><div class="hint-text">${a ? `<span style="color:${a.color}">●</span> ${esc(a.name)} · ${esc(a.title)}` : esc(s.agent_name || '(chưa giao)')}${s.duration_ms ? ` · ${(s.duration_ms / 1000).toFixed(1)} giây` : ''}</div>
          <div class="ai-chip ${s.provider_id ? 'own' : ''}">${esc(s.provider_id ? aiLabel(s.provider_id, s.model, s.effort || a?.effort, s.web) : a ? aiLabel(a.provider_id, s.model || a.model, s.effort || a.effort, s.web) : '')}${s.provider_id ? ' · AI riêng của bước' : ''}</div></div>
        <span class="badge ${s.status}">${SSTATUS[s.status] || s.status}</span>
        ${coord && !busy ? `<button class="icon-btn" data-edit="${s.id}" title="Sửa phân công"><i data-icon="edit"></i></button><button class="icon-btn" data-del="${s.id}" title="Xoá"><i data-icon="trash"></i></button>` : ''}
      </div>
      ${s.instruction ? `<details><summary>Chỉ dẫn</summary><div class="md">${md(s.instruction)}</div></details>` : ''}
      ${s.status === 'hoi' ? questionBox(s.id, s.question) : ''}
      ${s.status === 'dang_lam' || s.log ? `<details ${s.status === 'dang_lam' ? 'open' : ''}><summary>Nhật ký tiến trình</summary><div class="live-log" data-live="${s.id}">${liveLines(s).map((l) => `<div>${esc(l)}</div>`).join('')}</div></details>` : ''}
      ${s.files?.length ? `<div class="files-box sub-files">${s.files.map(fileChip).join('')}</div>` : ''}
      ${s.output ? `<details ${subs.length === 1 || s === subs[subs.length - 1] ? 'open' : ''}><summary>Kết quả <button class="btn tiny" data-export-sub="${s.id}" title="Xuất kết quả bước này ra tệp"><i data-icon="download"></i>Xuất tệp</button></summary><div class="md">${md(s.output)}</div></details>` : ''}
      ${s.error ? `<div class="errtext">${esc(s.error)}</div>` : ''}
    </div>`;
  }).join('');
  const coordName = st.users.find((u) => u.id === m.coordinator_id)?.display_name;
  const b = body(`
    <div class="toolbar"><button class="btn small ghost" id="back">← Danh sách</button><span class="grow"></span>
      ${coord && busy ? `<button class="btn small danger" id="cancel"><i data-icon="stop"></i>Huỷ</button>` : ''}
      ${coord && !busy ? `<button class="btn small" id="plan"><i data-icon="brain"></i>${subs.length ? 'Lập lại kế hoạch' : 'AI lập kế hoạch'}</button>
        <button class="btn small" id="addsub"><i data-icon="plus"></i>Thêm đầu việc</button>
        ${subs.length && !subs.some((x) => x.status === 'hoi') ? `<button class="btn small primary" id="run"><i data-icon="play"></i>${m.status === 'loi' || m.status === 'da_huy' ? 'Chạy lại phần chưa xong' : m.status === 'hoan_thanh' ? 'Chạy lại' : 'Duyệt & thực hiện'}</button>` : ''}` : ''}
      ${can('manager') && !busy ? `<button class="icon-btn" id="delm" title="Xoá nhiệm vụ"><i data-icon="trash"></i></button>` : ''}
    </div>
    <div class="mission-head"><h3>${esc(m.title)}</h3>
      <div class="kv"><span class="badge ${m.status}">${MSTATUS[m.status] || m.status}</span><span class="badge p-${m.priority}">Ưu tiên: ${PRIO[m.priority]}</span>
        <span class="badge">${MODE[m.mode] || m.mode}</span>${m.deadline ? `<span class="badge">Hạn: ${esc(m.deadline)}</span>` : ''}
        ${coordName ? `<span class="badge">Người phân công: ${esc(coordName)}</span>` : ''}${m.template_id ? `<span class="badge tpl" data-tplbadge="${m.template_id}">📋 Theo mẫu</span>` : ''}${busy ? '<span class="badge working">● đang xử lý</span>' : ''}</div></div>
    <div class="box md">${md(m.description)}</div>
    ${m.workdir || (m.attachments || []).length ? `<div class="section-title">Tài liệu & thư mục làm việc</div><div class="box files-box">
      ${m.workdir ? `<div class="fchip dir"><i data-icon="building"></i><span>Thư mục làm việc: <b>${esc(baseName(m.workdir))}</b> <small>${esc(m.workdir)}</small> · ${m.allow_edit ? '<span class="warn-text">AI được sửa tệp</span>' : '<span class="ok-text">chỉ đọc</span>'}</span><button class="btn small" data-open="${esc(m.workdir)}">Mở</button></div>` : ''}
      ${(m.attachments || []).map((p) => `<div class="fchip"><i data-icon="list"></i><span>${esc(baseName(p))} <small>${esc(p)}</small></span><button class="btn small" data-open="${esc(p)}">Mở</button></div>`).join('')}
      <div class="toolbar" style="margin:8px 0 0"><span class="grow"></span><button class="btn small" id="mfolder"><i data-icon="building"></i>Thư mục của nhiệm vụ</button></div>
    </div>` : ''}
    ${m.analysis ? `<div class="section-title">Nhận định của điều phối viên</div><div class="box md">${md(m.analysis)}</div>` : ''}
    <div class="section-title">Phân công (${subs.filter((s) => s.status === 'xong').length}/${subs.length} hoàn thành)</div>
    ${subHtml || `<div class="empty"><b>Chưa có đầu việc</b>${coord ? 'Dùng "AI lập kế hoạch" để điều phối viên phân công, hoặc "Thêm đầu việc" để tự giao cho đầu mối cụ thể.' : 'Đang chờ người phân công.'}</div>`}
    <div class="section-title">Tệp kết quả</div>
    <div class="box files-box" id="mfiles"><div class="hint-text">Đang tải…</div></div>
    ${m.summary ? `<div class="section-title sec-row">Báo cáo tổng hợp<span class="grow"></span><button class="btn small" data-export-sub=""><i data-icon="download"></i>Xuất báo cáo (Word, PDF, Excel…)</button></div><div class="box md">${md(m.summary)}</div>` : ''}
    ${coord && !busy && subs.length && !subs.some((x) => x.status === 'hoi') ? `<div class="section-title">Nhắn tiếp / yêu cầu chỉnh sửa</div>
      <div class="followup box"><textarea id="fu" rows="3" placeholder="VD: Rút gọn còn 1 trang, thêm bảng số liệu, đổi giọng văn trang trọng hơn…"></textarea>
        <div class="toolbar" style="margin:8px 0 0"><label class="hint-text" style="display:flex;gap:6px;align-items:center">Giao cho<select id="fu-agent">${agentOptions((subs[subs.length - 1] || {}).agent_id)}</select></label><span class="grow"></span>
          <button class="btn small primary" id="fu-send"><i data-icon="send"></i>Gửi yêu cầu tiếp</button></div></div>` : ''}
  `);
  bindQuestionBoxes(b);
  bindFileChips(b);
  loadMissionFiles(id);
  $$('[data-export-sub]', b).forEach((x) => x.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    exportDialog(id, x.dataset.exportSub ? +x.dataset.exportSub : null, m.title);
  }));
  $('#fu-send', b)?.addEventListener('click', async () => {
    const msg = $('#fu', b).value.trim();
    if (!msg) return toast('Hãy nhập nội dung yêu cầu tiếp', 'err');
    await run(() => call('followup_mission', { id, message: msg, agentId: +$('#fu-agent', b).value }), 'Đã gửi yêu cầu tiếp');
    renderMissionDetail(id);
  });
  $('#back', b).addEventListener('click', () => { st.missionId = null; renderMissions(); });
  $$('[data-open]', b).forEach((x) => x.addEventListener('click', () => run(() => call('open_path', { path: x.dataset.open }))));
  $('#mfolder', b)?.addEventListener('click', async () => { const p = await run(() => call('mission_folder', { id })); run(() => call('open_path', { path: p })); });
  $('#cancel', b)?.addEventListener('click', () => run(() => call('cancel_mission', { id }), 'Đã gửi lệnh huỷ'));
  $('#plan', b)?.addEventListener('click', () => run(() => call('plan_mission', { id }), 'Điều phối viên đang lập kế hoạch…'));
  $('#run', b)?.addEventListener('click', () => run(() => call('run_mission', { id }), 'Bắt đầu thực hiện'));
  $('#addsub', b)?.addEventListener('click', () => editSubtask(m, null, subs));
  $('#delm', b)?.addEventListener('click', async () => {
    if (await confirmBox('Xoá nhiệm vụ?', `Nhiệm vụ "${m.title}" và toàn bộ kết quả sẽ bị xoá.`)) {
      await run(() => call('delete_mission', { id }), 'Đã xoá nhiệm vụ');
      st.missionId = null;
      loadMissions().then(renderMissions);
    }
  });
  $$('[data-edit]', b).forEach((x) => x.addEventListener('click', () => editSubtask(m, subs.find((s) => s.id === +x.dataset.edit), subs)));
  $$('[data-del]', b).forEach((x) => x.addEventListener('click', async () => {
    await run(() => call('delete_subtask', { id: +x.dataset.del }));
    renderMissionDetail(id);
  }));
  void staff;
}

function agentOptions(selected) {
  return st.agents.map((a) => `<option value="${a.id}" ${a.id === selected ? 'selected' : ''}>${esc(a.name)} — ${esc(a.title)} (${LEVEL[a.level] || ''})</option>`).join('');
}

function editSubtask(m, s, subs) {
  const maxStep = subs.reduce((x, y) => Math.max(x, y.step), 1);
  modal(`<h3>${s ? 'Sửa phân công' : 'Giao đầu việc cho đầu mối'}</h3><p class="muted">Nhiệm vụ: ${esc(m.title)}</p>
    <form class="form" id="f">
      <label>Tên đầu việc<input name="title" required value="${esc(s?.title || '')}" placeholder="VD: Soạn dự thảo báo cáo"></label>
      <div class="row"><label>Giao cho<select name="agent">${agentOptions(s?.agent_id)}</select></label>
        <label>Bước (cùng bước chạy song song)<input type="number" name="step" min="1" max="10" value="${s?.step || maxStep}"></label></div>
      <label>Chỉ dẫn chi tiết<textarea name="ins" rows="5" placeholder="Nêu rõ đầu ra mong muốn…">${esc(s?.instruction || '')}</textarea></label>
      <div class="hint-text">Có thể cho bước này dùng AI khác với AI thường ngày của nhân viên (VD: Gemini để tra cứu, GPT để kiểm tra lại).</div>
      ${aiPickerHtml('st', s || {}, { inherit: true, web: true })}
      <div class="modal-actions"><button type="button" class="btn" data-close>Huỷ</button><button class="btn primary">Lưu phân công</button></div>
    </form>`, {
    wide: true,
    onMount: (b, close) => {
      bindAiPicker(b, 'st');
      $('#f', b).addEventListener('submit', async (e) => {
        e.preventDefault();
        const f = new FormData(e.target);
        await run(() => call('save_subtask', { subtask: { id: s?.id ?? null, mission_id: m.id, agent_id: +f.get('agent'), step: +f.get('step') || 1, title: f.get('title'), instruction: f.get('ins'), ...aiPickerValue(e.target, 'st') } }), 'Đã lưu phân công');
        close(true);
        renderMissionDetail(m.id);
      });
    },
  });
}

// ───────────────────────────── Bộ chọn AI: kết nối · model · mức suy luận ─────────────────────────────
const EFFORT = { '': 'Mặc định', low: 'Thấp', medium: 'Trung bình', high: 'Cao', xhigh: 'Rất cao', max: 'Tối đa' };
const EFFORT_TIP = 'Mức suy luận: cao hơn = suy nghĩ kỹ hơn, chậm và tốn hơn. Claude Code / Claude API hỗ trợ đủ 5 mức; Codex/OpenAI tối đa "Cao"; Gemini CLI không có tham số nên ứng dụng ghi yêu cầu vào lời nhắc.';
const modelCache = {};

async function modelsOf(pid) {
  if (!pid) return { models: [], live: false };
  if (!modelCache[pid]) modelCache[pid] = call('list_models', { providerId: pid }).catch(() => ({ models: [], live: false }));
  return modelCache[pid];
}

/** HTML bộ chọn AI. opts.inherit: thêm lựa chọn "Theo nhân viên"; opts.web: thêm ô tra cứu web. */
function aiPickerHtml(prefix, v = {}, opts = {}) {
  const provs = st.providers.map((p) => `<option value="${p.id}" ${p.id === v.provider_id ? 'selected' : ''}>${esc(p.name)}${p.enabled ? '' : ' (tắt)'}</option>`).join('');
  return `<div class="ai-picker" data-ai="${prefix}">
    <label>AI${opts.inherit ? ' cho bước này' : ''}<select name="${prefix}_prov">${opts.inherit ? `<option value="" ${!v.provider_id ? 'selected' : ''}>— Theo AI của nhân viên —</option>` : ''}${provs}</select></label>
    <label><span>Model <small class="model-src"></small></span><input name="${prefix}_model" list="${prefix}_models" value="${esc(v.model || '')}" placeholder="bỏ trống = mặc định" autocomplete="off"><datalist id="${prefix}_models"></datalist></label>
    <label title="${esc(EFFORT_TIP)}">Mức suy luận<select name="${prefix}_effort">${Object.entries(EFFORT).map(([k, l]) => `<option value="${k}" ${k === (v.effort || '') ? 'selected' : ''}>${l}</option>`).join('')}</select></label>
    ${opts.web ? `<label class="check web-check"><input type="checkbox" name="${prefix}_web" ${v.web ? 'checked' : ''}> Tra cứu web</label>` : ''}
  </div>`;
}

function bindAiPicker(root, prefix) {
  const box = root.querySelector(`[data-ai="${prefix}"]`);
  if (!box) return;
  const sel = box.querySelector(`[name="${prefix}_prov"]`);
  const dl = box.querySelector('datalist');
  const src = box.querySelector('.model-src');
  const input = box.querySelector(`[name="${prefix}_model"]`);
  const load = async (reset) => {
    if (reset) input.value = '';
    dl.innerHTML = '';
    src.textContent = '';
    if (!sel.value) { input.placeholder = 'theo nhân viên'; return; }
    src.textContent = '(đang tải…)';
    const r = await modelsOf(sel.value);
    dl.innerHTML = r.models.map((m) => `<option value="${esc(m)}">`).join('');
    src.textContent = r.models.length ? (r.live ? `(${r.models.length} model từ kết nối)` : '(gợi ý)') : '';
    input.placeholder = r.default_model ? `mặc định: ${r.default_model}` : 'bỏ trống = mặc định';
  };
  sel.addEventListener('change', () => load(true));
  load(false);
}

function aiPickerValue(form, prefix) {
  const f = new FormData(form);
  return { provider_id: f.get(`${prefix}_prov`) || '', model: String(f.get(`${prefix}_model`) || '').trim(), effort: f.get(`${prefix}_effort`) || '', web: !!f.get(`${prefix}_web`) };
}

/** Mô tả ngắn AI của nhân viên/bước: "Claude Code CLI · opus · suy luận Cao". */
function aiLabel(pid, model, effort, web) {
  return `${provName(pid)}${model ? ' · ' + model : ''}${effort ? ' · suy luận ' + EFFORT[effort] : ''}${web ? ' · web' : ''}`;
}

// ───────────────────────────── Quy trình nhiều AI ─────────────────────────────
st.pipelines = [];
async function loadPipelines() { st.pipelines = await call('list_pipelines').catch(() => []); return st.pipelines; }

function stageChain(p) {
  return `<div class="chain">${p.stages.map((s) => `<span class="chain-step"><b>B${s.step}</b> ${esc(s.title)}<small>${esc(s.provider_id ? provName(s.provider_id) : 'AI của nhân viên')}${s.model ? ' · ' + esc(s.model) : ''}${s.effort ? ' · ' + EFFORT[s.effort] : ''}${s.web ? ' · web' : ''}</small></span>`).join('<i class="arrow">→</i>')}</div>`;
}

async function renderPipelines() {
  if (st.view !== 'pipelines') return;
  await Promise.all([loadPipelines(), loadProviders()]);
  const b = body(`
    <p class="muted" style="margin-top:0">Một quy trình là chuỗi các bước, mỗi bước do một nhân viên làm với AI của hãng phù hợp nhất — ví dụ <b>Gemini</b> tra cứu thông tin → <b>Claude</b> viết tổng hợp → <b>GPT</b> kiểm tra lại. Bước sau đọc kết quả bước trước; các bước cùng số chạy song song.</p>
    <div class="toolbar"><span class="grow"></span>${can('coordinator') ? '<button class="btn small primary" id="newp"><i data-icon="plus"></i>Tạo quy trình</button>' : ''}</div>
    <div class="list">${st.pipelines.map((p) => `
      <div class="item static"><div class="item-row"><div class="grow"><h4>${esc(p.name)} ${p.builtin ? '<span class="badge">Mẫu</span>' : ''}</h4><p>${esc(p.description)}</p></div>
        ${can('coordinator') ? `<button class="btn small primary" data-use="${p.id}"><i data-icon="send"></i>Giao việc theo quy trình</button><button class="icon-btn" data-ed="${p.id}" title="Sửa"><i data-icon="edit"></i></button><button class="icon-btn" data-dup="${p.id}" title="Nhân bản"><i data-icon="plus"></i></button>` : ''}
        ${can('manager') ? `<button class="icon-btn" data-delp="${p.id}" title="Xoá"><i data-icon="trash"></i></button>` : ''}</div>
        ${stageChain(p)}</div>`).join('') || '<div class="empty"><b>Chưa có quy trình</b></div>'}</div>`);
  $('#newp', b)?.addEventListener('click', () => editPipeline(null));
  $$('[data-use]', b).forEach((x) => x.addEventListener('click', () => newMission({ pipelineId: +x.dataset.use })));
  $$('[data-ed]', b).forEach((x) => x.addEventListener('click', () => editPipeline(st.pipelines.find((p) => p.id === +x.dataset.ed))));
  $$('[data-dup]', b).forEach((x) => x.addEventListener('click', () => {
    const p = st.pipelines.find((y) => y.id === +x.dataset.dup);
    editPipeline({ ...structuredClone(p), id: 0, builtin: false, name: p.name + ' (bản sao)' });
  }));
  $$('[data-delp]', b).forEach((x) => x.addEventListener('click', async () => {
    const p = st.pipelines.find((y) => y.id === +x.dataset.delp);
    if (await confirmBox('Xoá quy trình?', `Xoá "${p.name}"? Các nhiệm vụ đã tạo không bị ảnh hưởng.`)) {
      await run(() => call('delete_pipeline', { id: p.id }), 'Đã xoá quy trình');
      renderPipelines();
    }
  }));
}

function editPipeline(p) {
  const pl = p ? structuredClone(p) : { id: 0, name: '', description: '', builtin: false, stages: [{ step: 1, title: '', instruction: '', agent_id: null, role: '', provider_id: '', model: '', effort: '', web: false }] };
  const staffOpts = (sel) => `<option value="">Tự chọn theo vai trò</option>` + st.agents.filter((a) => !a.is_dispatcher).map((a) => `<option value="${a.id}" ${a.id === sel ? 'selected' : ''}>${esc(a.name)} — ${esc(a.title)}</option>`).join('');
  modal('<div id="pe"></div>', {
    wide: true,
    onMount: (box, close) => {
      const w = $('#pe', box);
      const collect = () => {
        const form = $('#pf', w);
        if (!form) return;
        const f = new FormData(form);
        pl.name = f.get('name');
        pl.description = f.get('desc');
        pl.stages = pl.stages.map((s, i) => {
          const ai = aiPickerValue(form, 's' + i);
          return { ...s, step: +f.get('step' + i) || 1, title: f.get('title' + i), instruction: f.get('ins' + i), role: f.get('role' + i), agent_id: f.get('agent' + i) ? +f.get('agent' + i) : null, ...ai };
        });
      };
      const draw = () => {
        w.innerHTML = `<h3>${pl.id ? 'Sửa quy trình' : 'Tạo quy trình nhiều AI'}</h3>
          <form class="form" id="pf">
            <div class="row"><label>Tên quy trình<input name="name" required value="${esc(pl.name)}" placeholder="VD: Nghiên cứu → Tổng hợp → Kiểm chứng"></label></div>
            <label>Mô tả<input name="desc" value="${esc(pl.description)}"></label>
            ${pl.stages.map((s, i) => `<div class="stage-box">
              <div class="stage-head"><span class="step">B</span><input type="number" name="step${i}" min="1" max="12" value="${s.step}" class="step-in" title="Số bước (trùng số = chạy song song)">
                <input name="title${i}" required value="${esc(s.title)}" placeholder="Tên bước, VD: Tra cứu thông tin">
                <button type="button" class="icon-btn" data-up="${i}" title="Lên">↑</button><button type="button" class="icon-btn" data-delst="${i}" title="Bỏ bước"><i data-icon="trash"></i></button></div>
              <textarea name="ins${i}" rows="2" placeholder="Bước này cần làm gì, đầu ra mong muốn…">${esc(s.instruction)}</textarea>
              <div class="row"><label>Người làm<select name="agent${i}">${staffOpts(s.agent_id)}</select></label>
                <label>Từ khoá vai trò (khi tự chọn)<input name="role${i}" value="${esc(s.role)}" placeholder="VD: nghiên cứu, phân tích"></label></div>
              ${aiPickerHtml('s' + i, s, { inherit: true, web: true })}
            </div>`).join('')}
            <button type="button" class="btn small" id="addst"><i data-icon="plus"></i>Thêm bước</button>
            <div class="modal-actions"><button type="button" class="btn" data-x>Huỷ</button><button class="btn primary">Lưu quy trình</button></div>
          </form>`;
        paintIcons(w);
        pl.stages.forEach((_, i) => bindAiPicker(w, 's' + i));
        $('[data-x]', w).addEventListener('click', () => close(null));
        $('#addst', w).addEventListener('click', () => {
          collect();
          const last = pl.stages[pl.stages.length - 1];
          pl.stages.push({ step: (last?.step || 0) + 1, title: '', instruction: '', agent_id: null, role: '', provider_id: '', model: '', effort: '', web: false });
          draw();
        });
        $$('[data-delst]', w).forEach((x) => x.addEventListener('click', () => { collect(); pl.stages.splice(+x.dataset.delst, 1); if (!pl.stages.length) pl.stages.push({ step: 1, title: '', instruction: '', agent_id: null, role: '', provider_id: '', model: '', effort: '', web: false }); draw(); }));
        $$('[data-up]', w).forEach((x) => x.addEventListener('click', () => {
          collect();
          const i = +x.dataset.up;
          if (i > 0) { [pl.stages[i - 1], pl.stages[i]] = [pl.stages[i], pl.stages[i - 1]]; [pl.stages[i - 1].step, pl.stages[i].step] = [pl.stages[i].step, pl.stages[i - 1].step]; }
          draw();
        }));
        $('#pf', w).addEventListener('submit', async (e) => {
          e.preventDefault();
          collect();
          await run(() => call('save_pipeline', { pipeline: pl }), 'Đã lưu quy trình');
          close(true);
          if (st.view === 'pipelines') renderPipelines();
        });
      };
      draw();
    },
  });
}

// ───────────────────────────── Báo cáo hoàn thành ─────────────────────────────
async function notifyOS(title, body) {
  try {
    const N = window.__TAURI__?.notification;
    if (!N) return;
    let ok = await N.isPermissionGranted();
    if (!ok) ok = (await N.requestPermission()) === 'granted';
    if (ok) N.sendNotification({ title, body });
  } catch {}
}

function onMissionDone(ev) {
  const a = agentById(ev.presenter);
  // người trình bày đứng lên báo cáo trong văn phòng 3D
  if (ev.presenter) office.present(ev.presenter, ev.title, () => openMission(ev.mission_id));
  showDoneCard(ev);
  notifyOS('✅ Đã hoàn thành: ' + ev.title, `${a ? a.name + ' trình báo cáo' : 'Báo cáo đã sẵn sàng'}${ev.files ? ` · ${ev.files} tệp` : ''}`);
  if (ev.exported) toast('📄 Đã xuất theo mẫu: ' + ev.exported, 'ok');
}

/** Thẻ thông báo kết quả ở giữa phía dưới màn hình (tối đa 3 thẻ). */
function showDoneCard(ev) {
  const host = $('#done-cards');
  const a = agentById(ev.presenter);
  const el = document.createElement('div');
  el.className = 'done-card' + (ev.failed ? ' failed' : '');
  el.innerHTML = `<div class="item-row">
      ${a ? `<div class="avatar" style="background:${a.color};width:44px;height:44px"><img src="${avatarImg(a.avatar)}" style="width:40px;height:40px" alt=""></div>` : `<div class="avatar" style="background:${ev.failed ? '#b91c1c' : '#059669'}">${ev.failed ? '!' : '✓'}</div>`}
      <div class="grow"><div class="done-kicker">${ev.failed ? 'Nhiệm vụ gặp lỗi' : `${a ? esc(a.name) + ' trình báo cáo' : 'Đã hoàn thành'}`}${ev.template ? ` · theo mẫu “${esc(ev.template)}”` : ''}</div><h4>${esc(ev.title)}</h4></div>
      <button class="icon-btn" data-x title="Đóng">✕</button></div>
    <div class="done-body md">${ev.failed ? `<p class="errtext">${esc(String(ev.error).slice(0, 400))}</p>` : md(ev.excerpt || 'Báo cáo đã sẵn sàng.')}</div>
    <div class="toolbar" style="margin:8px 0 0">
      <button class="btn small primary" data-view-m><i data-icon="target"></i>Xem ${ev.failed ? 'nhiệm vụ' : 'báo cáo đầy đủ'}</button>
      ${ev.exported ? `<button class="btn small" data-open-f><i data-icon="download"></i>Mở tệp ${esc(extOf(ev.exported).toUpperCase())}</button>` : ''}
      ${!ev.failed ? `<button class="btn small" data-exp><i data-icon="download"></i>Xuất tệp…</button><button class="btn small" data-fu><i data-icon="send"></i>Nhắn tiếp</button>` : ''}
      <span class="grow"></span>${ev.files ? `<span class="hint-text">📄 ${ev.files} tệp</span>` : ''}</div>`;
  paintIcons(el);
  host.prepend(el);
  while (host.children.length > 3) host.lastChild.remove();
  const close = () => el.remove();
  $('[data-x]', el).addEventListener('click', close);
  $('[data-view-m]', el).addEventListener('click', () => { openMission(ev.mission_id); close(); });
  $('[data-open-f]', el)?.addEventListener('click', () => run(() => call('open_path', { path: ev.exported, reveal: false })));
  $('[data-exp]', el)?.addEventListener('click', () => exportDialog(ev.mission_id, null, ev.title));
  $('[data-fu]', el)?.addEventListener('click', async () => {
    openMission(ev.mission_id);
    close();
    setTimeout(() => { const t = $('#fu'); t?.scrollIntoView({ behavior: 'smooth', block: 'center' }); t?.focus(); }, 700);
  });
}

// ───────────────────────────── Trí nhớ nhân viên ─────────────────────────────
async function loadMemories(agentId) {
  const host = $('#agent-card #mem');
  if (!host) return;
  const list = await call('list_memories', { agentId }).catch(() => []);
  if (st.selected !== agentId || !$('#agent-card #mem')) return;
  const mine = list.filter((m) => m.agent_id === agentId);
  const office = list.filter((m) => m.agent_id == null);
  const row = (m) => `<div class="mem ${m.active ? '' : 'off'}"><span class="grow">${esc(m.content)}<small>${m.source === 'ai' ? 'AI tự ghi nhớ' : 'Quản lý thêm'}${m.agent_id == null ? ' · chung cả văn phòng' : ''} · ${esc((m.created_at || '').slice(0, 10))}</small></span>
    ${can('coordinator') ? `<button class="icon-btn" data-mtoggle="${m.id}" title="${m.active ? 'Tạm tắt' : 'Bật lại'}">${m.active ? '⏸' : '▶'}</button><button class="icon-btn" data-mdel="${m.id}" title="Xoá">✕</button>` : ''}</div>`;
  host.innerHTML = `<details class="mem-box" ${st.memOpen ? 'open' : ''}><summary>🧠 Trí nhớ (${mine.length} riêng · ${office.length} chung)</summary>
    <div class="hint-text">Những điều nhân viên luôn áp dụng: AI tự ghi nhớ khi bạn yêu cầu chỉnh sửa lặp lại, hoặc bạn thêm tay.</div>
    ${mine.map(row).join('') || '<div class="hint-text">Chưa có ghi nhớ riêng.</div>'}
    ${office.length ? `<div class="section-title">Quy định chung của văn phòng</div>${office.map(row).join('')}` : ''}
    ${can('coordinator') ? `<div class="mem-add"><input placeholder="VD: Báo cáo luôn có bảng số liệu ở đầu, tối đa 2 trang"><label class="check"><input type="checkbox"> chung cả văn phòng</label><button class="btn small primary">Ghi nhớ</button></div>` : ''}
  </details>`;
  const det = $('details', host);
  det.addEventListener('toggle', () => { st.memOpen = det.open; });
  $$('[data-mdel]', host).forEach((x) => x.addEventListener('click', async () => { await run(() => call('delete_memory', { id: +x.dataset.mdel })); loadMemories(agentId); }));
  $$('[data-mtoggle]', host).forEach((x) => x.addEventListener('click', async () => {
    const m = list.find((y) => y.id === +x.dataset.mtoggle);
    await run(() => call('save_memory', { memory: { ...m, active: !m.active } }));
    loadMemories(agentId);
  }));
  const add = $('.mem-add', host);
  add?.querySelector('button').addEventListener('click', async () => {
    const text = add.querySelector('input:not([type=checkbox])').value.trim();
    if (!text) return;
    const all = add.querySelector('input[type=checkbox]').checked;
    await run(() => call('save_memory', { memory: { id: 0, agent_id: all ? null : agentId, content: text, active: true } }), 'Đã ghi nhớ');
    st.memOpen = true;
    loadMemories(agentId);
  });
}

// ───────────────────────────── Mẫu tài liệu & báo cáo ─────────────────────────────
async function renderDocTemplates() {
  if (st.view !== 'templates') return;
  const list = await call('list_doc_templates').catch(() => []);
  const b = body(`
    <p class="muted" style="margin-top:0">Lưu các mẫu báo cáo/tài liệu bạn hay dùng. Khi giao việc, ứng dụng <b>tự chọn mẫu phù hợp</b> theo từ khoá (hoặc bạn chọn tay); nhân viên trình bày kết quả cuối <b>đúng cấu trúc mẫu</b> và ứng dụng tự xuất ra định dạng của mẫu (Word, PDF, Excel…). Có thể kéo thả tệp Word vào khi đang soạn mẫu.</p>
    <div class="toolbar"><span class="grow"></span>${can('coordinator') ? `<button class="btn small" id="imp"><i data-icon="download"></i>Nhập từ tệp Word/MD…</button><button class="btn small primary" id="newt"><i data-icon="plus"></i>Tạo mẫu</button>` : ''}</div>
    <div class="list">${list.map((t) => `<div class="item static"><div class="item-row"><div class="grow"><h4>📋 ${esc(t.name)} ${t.format ? `<span class="badge">→ .${esc(t.format)}</span>` : ''} ${t.auto ? '<span class="badge xong">Tự chọn</span>' : ''}</h4>
        <p>${esc(t.description || '')}${t.keywords ? ` · từ khoá: <i>${esc(t.keywords)}</i>` : ''}</p></div>
        ${can('coordinator') ? `<button class="btn small" data-tuse="${t.id}"><i data-icon="send"></i>Giao việc theo mẫu</button><button class="icon-btn" data-ted="${t.id}" title="Sửa"><i data-icon="edit"></i></button><button class="icon-btn" data-tdel="${t.id}" title="Xoá"><i data-icon="trash"></i></button>` : ''}</div>
        <details><summary>Xem mẫu</summary><div class="md tpl-preview">${md(t.content)}</div></details></div>`).join('') || '<div class="empty"><b>Chưa có mẫu nào</b>Tạo mẫu mới hoặc nhập từ tệp Word/Markdown của bạn.</div>'}</div>`);
  $('#newt', b)?.addEventListener('click', () => editDocTemplate(null));
  $('#imp', b)?.addEventListener('click', async () => {
    const T = window.__TAURI__;
    if (!T) return toast('Bản xem thử trên trình duyệt không mở được hộp thoại chọn tệp', 'err');
    const opts = { multiple: false, title: 'Chọn tệp mẫu', filters: [{ name: 'Tài liệu', extensions: ['docx', 'md', 'txt', 'doc', 'rtf', 'odt', 'html'] }] };
    const p = T.dialog?.open ? await T.dialog.open(opts) : await T.core.invoke('plugin:dialog|open', { options: opts });
    if (p) importTemplate(Array.isArray(p) ? p[0] : p);
  });
  $$('[data-ted]', b).forEach((x) => x.addEventListener('click', () => editDocTemplate(list.find((t) => t.id === +x.dataset.ted))));
  $$('[data-tuse]', b).forEach((x) => x.addEventListener('click', async () => {
    newMission({ tpl: +x.dataset.tuse });
  }));
  $$('[data-tdel]', b).forEach((x) => x.addEventListener('click', async () => {
    const t = list.find((y) => y.id === +x.dataset.tdel);
    if (await confirmBox('Xoá mẫu?', `Xoá mẫu “${t.name}”? Các nhiệm vụ đã làm không bị ảnh hưởng.`)) {
      await run(() => call('delete_doc_template', { id: t.id }), 'Đã xoá mẫu');
      renderDocTemplates();
    }
  }));
}

async function importTemplate(path) {
  const r = await run(() => call('import_template_file', { path }));
  editDocTemplate({ id: 0, name: r.name, description: '', keywords: r.name.toLowerCase(), content: r.content, format: extOf(path) === 'md' ? 'md' : 'docx', auto: true });
}

function editDocTemplate(t) {
  const tp = t || { id: 0, name: '', description: '', keywords: '', content: '# Tiêu đề báo cáo\n\n## 1. Tình hình chung\n\n## 2. Kết quả\n\n| Chỉ tiêu | Kế hoạch | Thực hiện |\n|---|---|---|\n|  |  |  |\n\n## 3. Đề xuất\n', format: 'docx', auto: true };
  const fmts = [['', 'Không tự xuất'], ['docx', 'Word (.docx)'], ['pdf', 'PDF'], ['xlsx', 'Excel (.xlsx)'], ['md', 'Markdown'], ['html', 'HTML'], ['txt', 'Văn bản']];
  modal(`<h3>${tp.id ? 'Sửa mẫu' : 'Mẫu tài liệu mới'}</h3>
    <form class="form" id="tf">
      <div class="row"><label>Tên mẫu<input name="name" required value="${esc(tp.name)}" placeholder="VD: Báo cáo tuần, Tờ trình mua sắm"></label>
        <label>Tự xuất khi xong<select name="format">${fmts.map(([k, l]) => `<option value="${k}" ${k === tp.format ? 'selected' : ''}>${l}</option>`).join('')}</select></label></div>
      <label>Dùng cho loại việc nào<input name="desc" value="${esc(tp.description)}" placeholder="VD: Báo cáo kết quả công tác hằng tuần của phòng"></label>
      <div class="row"><label>Từ khoá để tự chọn mẫu (phân cách bằng dấu phẩy)<input name="kw" value="${esc(tp.keywords)}" placeholder="báo cáo tuần, tuần qua, công tác tuần"></label>
        <label class="check" style="align-self:end"><input type="checkbox" name="auto" ${tp.auto ? 'checked' : ''}> Cho phép tự chọn mẫu này</label></div>
      <label>Nội dung mẫu (Markdown: # tiêu đề, bảng | a | b |, danh sách -) — hoặc kéo thả tệp Word vào đây
        <textarea name="content" rows="14" required style="font-family:var(--mono);font-size:12.5px">${esc(tp.content)}</textarea></label>
      <details><summary>Xem trước</summary><div class="md tpl-preview" id="tpv"></div></details>
      <div class="modal-actions"><button type="button" class="btn" data-close>Huỷ</button><button class="btn primary">Lưu mẫu</button></div>
    </form>`, {
    wide: true,
    onMount: (b, close) => {
      const ta = $('[name=content]', b);
      const pv = () => { $('#tpv', b).innerHTML = md(ta.value); };
      ta.addEventListener('input', pv);
      pv();
      // kéo thả tệp Word vào khi đang soạn mẫu
      st.dropHandler = async (paths) => {
        const r = await run(() => call('import_template_file', { path: paths[0] }));
        ta.value = r.content;
        if (!$('[name=name]', b).value) $('[name=name]', b).value = r.name;
        pv();
        toast('Đã nhập nội dung từ ' + r.name, 'ok');
      };
      $('#tf', b).addEventListener('submit', async (e) => {
        e.preventDefault();
        const f = new FormData(e.target);
        await run(() => call('save_doc_template', { template: { id: tp.id || 0, name: f.get('name'), description: f.get('desc'), keywords: f.get('kw'), content: f.get('content'), format: f.get('format'), auto: !!f.get('auto') } }), 'Đã lưu mẫu');
        close(true);
        if (st.view === 'templates') renderDocTemplates();
      });
    },
  }).then(() => { st.dropHandler = null; });
}

// ───────────────────────────── Tệp kết quả & xuất tệp ─────────────────────────────
const FILE_KIND = { tao: 'Mới tạo', sua: 'Đã sửa', xuat: 'Đã xuất' };
const FILE_ICON = { docx: '📝', doc: '📝', pdf: '📕', xlsx: '📊', xls: '📊', csv: '📊', md: '📄', txt: '📄', html: '🌐', htm: '🌐', pptx: '📽', png: '🖼', jpg: '🖼', jpeg: '🖼', zip: '🗜' };
const extOf = (n) => (String(n).split('.').pop() || '').toLowerCase();

function fileChip(f) {
  return `<div class="fchip file ${f.kind || ''}"><span class="ficon">${FILE_ICON[extOf(f.name)] || '📄'}</span>
    <span title="${esc(f.path)}"><b>${esc(f.name)}</b> <small>${fmtSize(f.size || 0)}${f.kind ? ' · ' + (FILE_KIND[f.kind] || f.kind) : ''}${f.by ? ' · ' + esc(f.by) : ''}</small></span>
    <button class="btn tiny" data-fopen="${esc(f.path)}">Mở</button><button class="btn tiny ghost" data-freveal="${esc(f.path)}" title="Hiện trong thư mục">Hiện</button></div>`;
}

function bindFileChips(root) {
  $$('[data-fopen]', root).forEach((x) => x.addEventListener('click', () => run(() => call('open_path', { path: x.dataset.fopen, reveal: false }))));
  $$('[data-freveal]', root).forEach((x) => x.addEventListener('click', () => run(() => call('open_path', { path: x.dataset.freveal, reveal: true }))));
}

async function loadMissionFiles(id) {
  const r = await call('mission_files', { id }).catch(() => null);
  const box = $('#mfiles');
  if (!box || st.missionId !== id || !r) return;
  box.innerHTML = (r.files.length ? r.files.map(fileChip).join('') : '<div class="hint-text">Chưa có tệp nào. Yêu cầu nhân viên "xuất file Word/PDF/Excel…" hoặc bấm "Xuất báo cáo".</div>')
    + `<div class="toolbar" style="margin:8px 0 0"><span class="grow"></span><button class="btn small" id="open-kq"><i data-icon="building"></i>Mở thư mục kết quả</button></div>`;
  paintIcons(box);
  bindFileChips(box);
  $('#open-kq', box).addEventListener('click', () => run(() => call('open_path', { path: r.folder })));
}

/** Hộp chọn định dạng xuất tệp cho báo cáo (subtaskId = null) hoặc kết quả một bước. */
async function exportDialog(missionId, subtaskId, title) {
  const fmts = await call('export_formats').catch(() => [{ ext: 'docx', label: 'Word (.docx)' }, { ext: 'pdf', label: 'PDF (.pdf)' }, { ext: 'xlsx', label: 'Excel (.xlsx)' }, { ext: 'md', label: 'Markdown (.md)' }]);
  const tips = { docx: 'Văn bản Word, giữ tiêu đề, bảng, danh sách', pdf: 'Bản in PDF, phông có dấu tiếng Việt', xlsx: 'Mỗi bảng thành một trang tính, số liệu dạng số', csv: 'Bảng dạng CSV (mở bằng Excel)', html: 'Trang web xem bằng trình duyệt', md: 'Văn bản Markdown gốc', txt: 'Văn bản thuần' };
  modal(`<h3>Xuất ${subtaskId ? 'kết quả bước' : 'báo cáo'} ra tệp</h3><p class="muted">${esc(title)}</p>
    <div class="fmt-grid">${fmts.map((f) => `<button class="fmt" data-fmt="${f.ext}"><span class="ficon big">${FILE_ICON[f.ext] || '📄'}</span><b>${esc(f.label)}</b><small>${tips[f.ext] || ''}</small></button>`).join('')}</div>
    <label class="check" style="margin-top:12px"><input type="checkbox" id="saveas"> Chọn nơi lưu (mặc định lưu vào thư mục kết quả của nhiệm vụ)</label>
    <div class="modal-actions"><button class="btn" data-close>Đóng</button></div>`, {
    onMount: (b, close) => $$('[data-fmt]', b).forEach((x) => x.addEventListener('click', async () => {
      const ext = x.dataset.fmt;
      let path = null;
      if ($('#saveas', b).checked && window.__TAURI__) {
        const T = window.__TAURI__;
        const opts = { defaultPath: `${title}.${ext}`.replace(/[\\/:*?"<>|]/g, '-'), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] };
        path = T.dialog?.save ? await T.dialog.save(opts) : await T.core.invoke('plugin:dialog|save', { options: opts });
        if (!path) return;
      }
      x.disabled = true;
      const out = await run(() => call('export_content', { missionId, subtaskId, format: ext, path }));
      close(true);
      toast('Đã xuất: ' + out, 'ok');
      call('open_path', { path: out, reveal: false }).catch(() => {});
      if (st.missionId === missionId) loadMissionFiles(missionId);
    })),
  });
}

// ───────────────────────────── Tệp & thư mục làm việc ─────────────────────────────
/** Bộ tệp của hộp thoại đang mở: { workdir, attachments[], allowEdit, render() } — nơi nhận tệp kéo thả. */
st.files = null;
const fmtSize = (n) => (n >= 1048576 ? (n / 1048576).toFixed(1) + ' MB' : n >= 1024 ? Math.round(n / 1024) + ' KB' : n + ' B');
const baseName = (p) => String(p).split(/[\\/]/).filter(Boolean).pop() || p;

async function pickPaths(directory) {
  const opts = { directory, multiple: !directory, title: directory ? 'Chọn thư mục làm việc' : 'Chọn tệp đính kèm' };
  try {
    const T = window.__TAURI__;
    if (!T) { toast('Bản xem thử trên trình duyệt không mở được hộp thoại chọn tệp — hãy kéo thả.', 'err'); return []; }
    const r = T.dialog?.open ? await T.dialog.open(opts) : await T.core.invoke('plugin:dialog|open', { options: opts });
    if (!r) return [];
    return Array.isArray(r) ? r : [r];
  } catch (e) { toast('Không mở được hộp thoại: ' + e, 'err'); return []; }
}

/** Thêm đường dẫn: thư mục đầu tiên thành thư mục làm việc, còn lại là tệp đính kèm. */
async function addPaths(files, paths) {
  if (!paths?.length) return;
  const info = await call('path_info', { paths }).catch(() => paths.map((p) => ({ path: p, name: baseName(p), is_dir: false, size: 0 })));
  for (const it of info) {
    if (it.is_dir && !files.workdir) files.workdir = it.path;
    else if (!files.attachments.some((a) => a.path === it.path) && it.path !== files.workdir) files.attachments.push(it);
  }
  files.render?.();
}

function filesHtml(files, compact = false) {
  const wd = files.workdir;
  return `<div class="dropzone ${wd || files.attachments.length ? 'has' : ''}">
      <div class="dz-hint"><i data-icon="download"></i><div><b>Kéo thả tệp hoặc thư mục vào cửa sổ</b>
        <small>Thư mục → AI làm việc ngay trong thư mục đó (như một project). Tệp → đính kèm, AI đọc nội dung.</small></div></div>
      ${wd ? `<div class="fchip dir"><i data-icon="building"></i><span title="${esc(wd)}">Thư mục làm việc: <b>${esc(baseName(wd))}</b> <small>${esc(wd)}</small></span><button type="button" class="icon-btn" data-drop-wd title="Bỏ"><i data-icon="x"></i></button></div>` : ''}
      ${files.attachments.map((a, i) => `<div class="fchip"><i data-icon="${a.is_dir ? 'building' : 'list'}"></i><span title="${esc(a.path)}">${esc(a.name)} <small>${a.is_dir ? 'thư mục' : fmtSize(a.size || 0)}</small></span><button type="button" class="icon-btn" data-drop-at="${i}" title="Bỏ"><i data-icon="x"></i></button></div>`).join('')}
      <div class="dz-actions"><button type="button" class="btn small" data-pick-dir><i data-icon="building"></i>${wd ? 'Đổi thư mục' : 'Chọn thư mục'}</button>
        <button type="button" class="btn small" data-new-dir title="Tạo thư mục mới để nhân viên lưu các tệp làm ra"><i data-icon="plus"></i>Tạo thư mục mới</button>
        <button type="button" class="btn small" data-pick-files><i data-icon="plus"></i>Thêm tệp</button>
        <label class="check"><input type="checkbox" data-allow ${files.allowEdit ? 'checked' : ''}> ${compact ? 'Cho phép sửa tệp' : 'Cho phép AI tạo/sửa tệp trong thư mục'}</label></div>
    </div>`;
}

function bindFiles(root, files) {
  $$('[data-drop-at]', root).forEach((x) => x.addEventListener('click', () => { files.attachments.splice(+x.dataset.dropAt, 1); files.render(); }));
  $('[data-drop-wd]', root)?.addEventListener('click', () => { files.workdir = ''; files.render(); });
  $('[data-pick-dir]', root)?.addEventListener('click', async () => { const p = await pickPaths(true); if (p[0]) { files.workdir = ''; await addPaths(files, p.slice(0, 1)); } });
  $('[data-pick-files]', root)?.addEventListener('click', async () => addPaths(files, await pickPaths(false)));
  $('[data-new-dir]', root)?.addEventListener('click', () => {
    if ($('.newdir-row', root)) return;
    const title = $('#f [name=title]')?.value?.trim() || $('#quick')?.value?.trim().slice(0, 40) || 'Dự án mới';
    const row = document.createElement('div');
    row.className = 'newdir-row';
    row.innerHTML = `<input value="${esc(title)}" placeholder="Tên thư mục"><button type="button" class="btn small" data-parent>Nơi tạo…</button><button type="button" class="btn small primary" data-mk>Tạo</button>
      <div class="hint-text" data-where>Tạo trong: ~/Documents/AI Office</div>`;
    $('.dropzone', root).appendChild(row);
    let parent = null;
    $('input', row).focus();
    $('[data-parent]', row).addEventListener('click', async () => {
      const p = await pickPaths(true);
      if (p[0]) { parent = p[0]; $('[data-where]', row).textContent = 'Tạo trong: ' + parent; }
    });
    $('[data-mk]', row).addEventListener('click', async () => {
      const name = $('input', row).value.trim();
      if (!name) return toast('Hãy đặt tên thư mục', 'err');
      const dir = await run(() => call('create_workdir', { name, parent }));
      files.workdir = dir;
      files.allowEdit = true; // thư mục mới tạo để làm việc: cho phép tạo/sửa tệp
      files.render();
      toast('Đã tạo thư mục làm việc: ' + dir, 'ok');
    });
  });
  $('[data-allow]', root)?.addEventListener('change', (e) => { files.allowEdit = e.target.checked; });
}

const filesPayload = (files) => ({ workdir: files?.workdir || '', attachments: (files?.attachments || []).map((a) => a.path), allow_edit: !!files?.allowEdit });

async function newMission(preset = {}) {
  if (!can('coordinator')) return;
  st.users = await call('list_users').catch(() => st.users);
  const coords = st.users.filter((u) => u.active && RANK[u.role] >= 2);
  const files = { workdir: '', attachments: [], allowEdit: false };
  await loadPipelines();
  const tpls = await call('list_doc_templates').catch(() => []);
  modal(`<h3>Giao nhiệm vụ mới</h3><p class="muted">Mô tả việc cần làm — điều phối viên AI sẽ phân rã và giao cho nhân viên phù hợp. Kéo thả tệp/thư mục để làm việc trên tài liệu thật.</p>
    <form class="form" id="f">
      <label>Tiêu đề<input name="title" required placeholder="VD: Rà soát và cập nhật báo cáo quý" value="${esc(preset.title || '')}"></label>
      <label>Mô tả chi tiết / yêu cầu đầu ra<textarea name="desc" rows="5" required placeholder="Bối cảnh, mục tiêu, yêu cầu, định dạng kết quả mong muốn…">${esc(preset.description || '')}</textarea></label>
      <div id="files"></div>
      <div class="row">
        <label>Mức ưu tiên<select name="prio">${Object.entries(PRIO).map(([k, v]) => `<option value="${k}" ${k === 'trung_binh' ? 'selected' : ''}>${v}</option>`).join('')}</select></label>
        <label>Hạn hoàn thành<input name="deadline" type="date"></label>
        <label>Người phân công<select name="coord"><option value="">Điều phối viên AI</option>${coords.map((u) => `<option value="${u.id}">${esc(u.display_name)} (${ROLE[u.role]})</option>`).join('')}</select></label>
      </div>
      <div><div class="hint-text" style="margin-bottom:6px">Cách phân công</div>
        <div class="mode-pick">
          <label><input type="radio" name="mode" value="auto" checked><b>Tự động</b><small>AI phân công và thực hiện ngay</small></label>
          <label><input type="radio" name="mode" value="review"><b>AI đề xuất, duyệt</b><small>Xem/sửa phân công rồi mới chạy</small></label>
          <label><input type="radio" name="mode" value="manual"><b>Phân công tay</b><small>Tự chọn đầu mối cho từng việc</small></label>
          <label><input type="radio" name="mode" value="pipeline"><b>Theo quy trình</b><small>Chuỗi nhiều AI (Gemini → Claude → GPT…)</small></label>
        </div></div>
      <label>Mẫu kết quả<select name="tpl"><option value="auto">Tự chọn mẫu phù hợp theo nội dung${tpls.length ? '' : ' (chưa có mẫu nào)'}</option><option value="none">Không dùng mẫu</option>${tpls.map((t) => `<option value="${t.id}" ${preset.tpl === t.id ? 'selected' : ''}>${esc(t.name)}${t.format ? ' → .' + t.format : ''}</option>`).join('')}</select></label>
      <div id="pipe" class="hidden"><div class="row"><label>Quy trình<select name="pipeline">${st.pipelines.map((p) => `<option value="${p.id}" ${p.id === preset.pipelineId ? 'selected' : ''}>${esc(p.name)}</option>`).join('')}</select></label>
        <label class="check" style="align-self:end"><input type="checkbox" name="prun" checked> Chạy ngay (bỏ chọn để xem/sửa từng bước trước)</label></div><div id="pipe-chain"></div></div>
      <div class="modal-actions"><button type="button" class="btn" data-close>Huỷ</button><button class="btn primary"><i data-icon="send"></i>Giao nhiệm vụ</button></div>
    </form>`, {
    wide: true,
    onMount: (b, close) => {
      const host = $('#files', b);
      files.render = () => { host.innerHTML = filesHtml(files); paintIcons(host); bindFiles(host, files); };
      files.render();
      st.files = files;
      if (preset.paths) addPaths(files, preset.paths);
      const syncMode = () => {
        const pipe = $('[name=mode]:checked', b).value === 'pipeline';
        $('#pipe', b).classList.toggle('hidden', !pipe);
        const p = st.pipelines.find((x) => x.id === +$('[name=pipeline]', b)?.value);
        $('#pipe-chain', b).innerHTML = pipe && p ? stageChain(p) : '';
      };
      $$('[name=mode]', b).forEach((r) => r.addEventListener('change', syncMode));
      $('[name=pipeline]', b)?.addEventListener('change', syncMode);
      if (preset.pipelineId) { $('[name=mode][value=pipeline]', b).checked = true; }
      syncMode();
      $('#f', b).addEventListener('submit', async (e) => {
        e.preventDefault();
        const f = new FormData(e.target);
        const id = await run(() => call('create_mission', { mission: { title: f.get('title'), description: f.get('desc'), priority: f.get('prio'), mode: f.get('mode'), coordinator_id: f.get('coord') ? +f.get('coord') : null, deadline: f.get('deadline') || '', ...filesPayload(files), pipeline_id: f.get('mode') === 'pipeline' ? +f.get('pipeline') : null, pipeline_run: !!f.get('prun'), template_mode: /^\d+$/.test(f.get('tpl')) ? 'pick' : f.get('tpl'), template_id: /^\d+$/.test(f.get('tpl')) ? +f.get('tpl') : null } }), 'Đã giao nhiệm vụ');
        st.files = null;
        close(true);
        await loadMissions();
        openMission(id);
        refreshStats();
      });
    },
  }).then(() => { if (st.files === files) st.files = null; });
}

// ───────────────────────────── Kéo thả tệp vào cửa sổ ─────────────────────────────
function setupDragDrop() {
  const T = window.__TAURI__;
  if (!T?.event) return;
  const overlay = $('#drop-overlay');
  T.event.listen('tauri://drag-enter', () => { if (st.user && can('coordinator')) overlay.classList.remove('hidden'); });
  T.event.listen('tauri://drag-leave', () => overlay.classList.add('hidden'));
  T.event.listen('tauri://drag-drop', async (e) => {
    overlay.classList.add('hidden');
    const paths = e.payload?.paths || [];
    if (!paths.length || !st.user || !can('coordinator')) return;
    if (st.dropHandler) return st.dropHandler(paths); // trình soạn mẫu đang mở
    if (st.files) return addPaths(st.files, paths); // hộp thoại / thẻ nhân viên đang nhận tệp
    newMission({ paths }); // chưa mở gì: tạo nhiệm vụ mới kèm tệp
  });
}

// ───────────────────────────── Phong cách văn phòng ─────────────────────────────
function styleCards(selected) {
  return `<div class="style-pick">${Object.entries(THEMES).map(([k, t]) => `
    <button type="button" class="style-card ${k === selected ? 'sel' : ''}" data-style="${k}">
      <span class="sw">${t.swatch.map((c) => `<i style="background:${c}"></i>`).join('')}</span>
      <b>${esc(t.name)}</b><small>${esc(t.desc)}</small></button>`).join('')}</div>`;
}
function bindStyleCards(root, onPick) {
  $$('[data-style]', root).forEach((x) => x.addEventListener('click', () => {
    $$('[data-style]', root).forEach((y) => y.classList.toggle('sel', y === x));
    onPick(x.dataset.style);
  }));
}

// ───────────────────────────── Nhân viên ─────────────────────────────
function renderStaff() {
  if (st.view !== 'staff') return;
  const busyOnly = st.staffFilter === 'working';
  const list = busyOnly ? st.agents.filter((a) => ['working', 'thinking', 'asking'].includes(a.status)) : st.agents;
  const b = body(`
    <div class="toolbar"><button class="btn small ${busyOnly ? '' : 'primary'}" data-sf="all">Tất cả (${st.agents.length})</button>
      <button class="btn small ${busyOnly ? 'primary' : ''}" data-sf="working">Đang làm (${st.agents.filter((a) => ['working', 'thinking', 'asking'].includes(a.status)).length})</button><span class="grow"></span>
      ${can('manager') ? `<button class="btn small" id="recruit"><i data-icon="sparkles"></i>Tuyển bằng AI</button><button class="btn small primary" id="add"><i data-icon="plus"></i>Thêm thủ công</button>` : ''}</div>
    <div class="list">${list.length ? '' : '<div class="empty"><b>Không có nhân viên nào đang làm</b>Giao nhiệm vụ để văn phòng bắt đầu làm việc.</div>'}${list.map((a) => `
      <div class="item" data-id="${a.id}"><div class="item-row">
        <div class="avatar" style="background:${a.color}"><img src="${avatarImg(a.avatar)}" alt="" style="width:34px;height:34px"></div>
        <div class="grow"><h4>${esc(a.name)} ${a.is_dispatcher ? '<span class="badge lv-chuyen_gia">Điều phối</span>' : ''}</h4>
          <p>${esc(a.title)}${a.department ? ' · ' + esc(a.department) : ''} · ${esc(aiLabel(a.provider_id, a.model, a.effort))}</p></div>
        <span class="badge lv-${a.level}">${LEVEL[a.level] || a.level}</span>
        <span class="badge ${a.status}">${ASTATUS[a.status] || a.status}</span>
      </div>
      ${a.skills.length ? `<div class="skills">${a.skills.map((s) => `<span class="skill">${esc(s)}</span>`).join('')}</div>` : ''}
      </div>`).join('')}</div>`);
  $$('[data-sf]', b).forEach((x) => x.addEventListener('click', () => { st.staffFilter = x.dataset.sf; renderStaff(); }));
  $('#add', b)?.addEventListener('click', () => editAgent(null));
  $('#recruit', b)?.addEventListener('click', () => openView('recruit'));
  $$('.item[data-id]', b).forEach((x) => x.addEventListener('click', () => { selectAgent(+x.dataset.id); office.focus(+x.dataset.id); }));
}

// ───────────────────────────── Việc đang làm, hỏi đáp ─────────────────────────────
st.live = {};
st.qaDraft = {};
st.questions = [];
const fmtElapsed = (from) => {
  if (!from) return '';
  const t = new Date(from.replace(' ', 'T'));
  const sec = Math.max(0, Math.round((Date.now() - t.getTime()) / 1000));
  return sec >= 3600 ? `${Math.floor(sec / 3600)} giờ ${Math.floor((sec % 3600) / 60)} phút` : sec >= 60 ? `${Math.floor(sec / 60)} phút ${sec % 60} giây` : `${sec} giây`;
};
setInterval(() => document.querySelectorAll('[data-since]').forEach((el) => { el.textContent = fmtElapsed(el.dataset.since); }), 1000);

/** Nhật ký: dòng trực tiếp (đang chạy) hoặc nhật ký đã lưu. */
function liveLines(sub, live) {
  if (st.live[sub.id]?.length) return st.live[sub.id];
  if (live?.length) { st.live[sub.id] = live.slice(); return live; }
  return sub.log ? sub.log.split('\n') : [];
}

function questionBox(subtaskId, q) {
  return `<div class="qbox"><div><b>❓ Câu hỏi cho bạn:</b> ${esc(q)}</div>
    ${can('coordinator') ? `<div class="qrow"><textarea rows="2" data-qa="${subtaskId}" placeholder="Nhập câu trả lời… (⌘/Ctrl + Enter để gửi)">${esc(st.qaDraft[subtaskId] || '')}</textarea><button class="btn small primary" data-qsend="${subtaskId}"><i data-icon="send"></i>Trả lời</button></div>` : ''}</div>`;
}

function bindQuestionBoxes(root) {
  paintIcons(root);
  $$('[data-qsend]', root).forEach((x) => x.addEventListener('click', async () => {
    const sid = +x.dataset.qsend;
    const text = $(`[data-qa="${sid}"]`, root).value.trim();
    if (!text) return toast('Hãy nhập câu trả lời', 'err');
    x.disabled = true;
    await run(() => call('answer_question', { subtaskId: sid, answer: text }), 'Đã trả lời — nhân viên làm tiếp');
  }));
  $$('[data-qa]', root).forEach((t) => t.addEventListener('input', () => { st.qaDraft[t.dataset.qa] = t.value; }));
  $$('[data-qa]', root).forEach((t) => t.addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) $(`[data-qsend="${t.dataset.qa}"]`, root).click(); }));
}

async function loadQuestions() {
  st.questions = await call('list_questions').catch(() => []);
  for (const q of st.questions) if (q.agent_id) office.ask(q.agent_id, q.question);
  renderQuestions();
}

function renderQuestions() {
  const el = $('#questions');
  if (!el) return;
  el.innerHTML = st.questions.map((q) => {
    const a = agentById(q.agent_id);
    return `<div class="qcard" data-qagent="${q.agent_id}"><div class="item-row">${a ? `<div class="avatar" style="background:${a.color};width:30px;height:30px"><img src="${avatarImg(a.avatar)}" style="width:26px;height:26px" alt=""></div>` : ''}
      <div class="grow"><b>${esc(a?.name || 'Nhân viên')}</b> hỏi <span class="hint-text">· ${esc(q.title || '')}</span></div></div><p>${esc(q.question)}</p>
      <div class="hint-text">Bấm để xem và trả lời</div></div>`;
  }).join('');
  $$('[data-qagent]', el).forEach((x) => x.addEventListener('click', () => onQuestion(+x.dataset.qagent)));
}

/** Bấm vào bóng câu hỏi trong 3D hoặc thẻ câu hỏi: mở thẻ nhân viên ở phần trả lời. */
function onQuestion(agentId) {
  selectAgent(agentId);
  office.focus(agentId);
  setTimeout(() => $('#agent-card [data-qa]')?.focus(), 400);
}
office.onQuestion = onQuestion;

async function loadWork(agentId) {
  const host = $('#agent-card #work');
  if (!host) return;
  const w = await call('agent_work', { agentId }).catch(() => null);
  if (!w || st.selected !== agentId || !$('#agent-card #work')) return;
  const items = w.items || [];
  const cur = items.find((x) => x.subtask.status === 'dang_lam');
  const asks = items.filter((x) => x.subtask.status === 'hoi');
  const queued = items.filter((x) => x.subtask.status === 'cho');
  const stepIcon = { xong: '✓', dang_lam: '●', loi: '✕', hoi: '?', huy: '–', cho: '○' };
  const steps = (it) => `<div class="steps">${it.steps.map((x) => `<span class="sstep ${x.status} ${x.id === it.subtask.id ? 'me' : ''}" title="${esc(x.agent_name)} · ${esc(SSTATUS[x.status] || x.status)}">${stepIcon[x.status] || '○'} B${x.step} ${esc(x.title)}</span>`).join('')}</div>`;
  host.innerHTML = `
    ${asks.map((it) => `<div class="section-title">Đang hỏi bạn</div>${questionBox(it.subtask.id, it.subtask.question)}<div class="hint-text">Nhiệm vụ: <a href="#" data-mission="${it.subtask.mission_id}">${esc(it.mission_title || '')}</a></div>`).join('')}
    ${cur ? `<div class="section-title">Đang làm</div>
      <div class="work-now">
        <div><b>${esc(cur.subtask.title)}</b></div>
        <div class="hint-text">Nhiệm vụ: <a href="#" data-mission="${cur.subtask.mission_id}">${esc(cur.mission_title || '')}</a> · bước ${cur.subtask.step}/${Math.max(...cur.steps.map((x) => x.step))} · đã làm <span data-since="${esc(cur.subtask.started_at)}">${fmtElapsed(cur.subtask.started_at)}</span></div>
        ${steps(cur)}
        <details><summary>Chỉ dẫn được giao</summary><div class="md">${md(cur.subtask.instruction)}</div></details>
        <div class="hint-text" style="margin-top:6px">Tiến trình trực tiếp</div>
        <div class="live-log" data-live="${cur.subtask.id}">${liveLines(cur.subtask, cur.live).map((l) => `<div>${esc(l)}</div>`).join('') || '<div class="muted">Đang chờ AI phản hồi…</div>'}</div>
        ${can('coordinator') ? `<div class="toolbar" style="margin:8px 0 0"><span class="grow"></span><button class="btn small danger" data-cancelm="${cur.subtask.mission_id}"><i data-icon="stop"></i>Huỷ nhiệm vụ</button></div>` : ''}
      </div>` : ''}
    ${queued.length ? `<div class="section-title">Đang xếp hàng (${queued.length})</div>${queued.map((it) => `<div class="hint-text">○ ${esc(it.subtask.title)} — <a href="#" data-mission="${it.subtask.mission_id}">${esc(it.mission_title || '')}</a></div>`).join('')}` : ''}
    ${!cur && !asks.length && !queued.length ? `<div class="hint-text" style="margin-top:10px">Hiện không có việc nào đang làm.</div>` : ''}
    ${w.recent?.length ? `<div class="section-title">Vừa hoàn thành</div>${w.recent.map((r) => `<div class="hint-text">✓ <a href="#" data-mission="${r.mission_id}">${esc(r.title)}</a> · ${esc((r.finished_at || '').slice(11, 16))}</div>`).join('')}` : ''}`;
  bindQuestionBoxes(host);
  $$('.live-log', host).forEach((el) => { el.scrollTop = el.scrollHeight; });
  $$('[data-mission]', host).forEach((x) => x.addEventListener('click', (e) => { e.preventDefault(); openMission(+x.dataset.mission); }));
  $$('[data-cancelm]', host).forEach((x) => x.addEventListener('click', () => run(() => call('cancel_mission', { id: +x.dataset.cancelm }), 'Đã gửi lệnh huỷ')));
}

function selectAgent(id) {
  st.selected = id;
  document.body.classList.toggle('card-open', !!id);
  office.highlight(id);
  if (!id) {
    $('#agent-card').classList.add('hidden');
    if (st.files && Object.values(st.quick).includes(st.files)) st.files = null;
    return;
  }
  renderAgentCard(id);
}

st.quick = {};
function renderAgentCard(id) {
  const a = agentById(id);
  const card = $('#agent-card');
  const typed = $('#quick', card)?.value || '';
  if (!a) { card.classList.add('hidden'); return; }
  card.classList.remove('hidden');
  card.innerHTML = `
    <div class="item-row"><div class="avatar" style="background:${a.color};width:52px;height:52px"><img src="${avatarImg(a.avatar)}" style="width:46px;height:46px" alt=""></div>
      <div class="grow"><h3>${esc(a.name)}</h3><div class="muted">${esc(a.title)}</div></div>
      <button class="icon-btn" id="cc"><i data-icon="x"></i></button></div>
    <div class="meta">
      <span>Cấp độ</span><span><span class="badge lv-${a.level}">${LEVEL[a.level] || a.level}</span></span>
      <span>Phòng ban</span><span>${esc(a.department || '—')}</span>
      <span>Trạng thái</span><span><span class="badge ${a.status}">${ASTATUS[a.status] || a.status}</span></span>
      <span>AI</span><span>${esc(aiLabel(a.provider_id, a.model, a.effort))}</span>
      <span>Đã hoàn thành</span><span>${a.tasks_done} đầu việc</span>
    </div>
    ${a.skills.length ? `<div class="skills">${a.skills.map((s) => `<span class="skill">${esc(s)}</span>`).join('')}</div>` : ''}
    <div id="work"><div class="muted hint-text" style="margin-top:10px">Đang tải công việc…</div></div>
    <div id="mem"></div>
    ${can('coordinator') ? `<div class="section-title">Giao việc nhanh</div>
      <textarea id="quick" rows="3" placeholder="Nhờ ${esc(a.name)} làm gì đó… (có thể kéo thả tệp/thư mục vào)"></textarea>
      <div id="qfiles" style="margin-top:8px"></div>
      <div class="toolbar" style="margin-top:8px"><span class="grow"></span><button class="btn small primary" id="send"><i data-icon="send"></i>Giao việc</button></div>` : ''}
    <div class="toolbar" style="margin-top:6px">
      <button class="btn small" id="focus"><i data-icon="camera"></i>Xem</button>
      ${can('manager') ? `<button class="btn small" id="edit"><i data-icon="edit"></i>Sửa hồ sơ</button>${a.is_dispatcher ? '' : `<button class="btn small danger" id="fire"><i data-icon="trash"></i>Cho thôi việc</button>`}` : ''}
    </div>`;
  paintIcons(card);
  loadWork(id);
  loadMemories(id);
  $('#cc', card).addEventListener('click', () => selectAgent(null));
  $('#focus', card).addEventListener('click', () => office.focus(id));
  $('#edit', card)?.addEventListener('click', () => editAgent(a));
  $('#fire', card)?.addEventListener('click', async () => {
    if (await confirmBox('Cho thôi việc?', `${a.name} (${a.title}) sẽ bị xoá khỏi văn phòng. Kết quả công việc cũ vẫn được giữ.`)) {
      await run(() => call('delete_agent', { id }), 'Đã cho thôi việc');
      selectAgent(null);
    }
  });
  const qf = (st.quick[id] ||= { workdir: '', attachments: [], allowEdit: false });
  const qhost = $('#qfiles', card);
  if (qhost) {
    $('#quick', card).value = typed;
    qf.render = () => { qhost.innerHTML = filesHtml(qf, true); paintIcons(qhost); bindFiles(qhost, qf); };
    qf.render();
    if ($('#modal').classList.contains('hidden')) st.files = qf;
  }
  $('#send', card)?.addEventListener('click', async () => {
    const msg = $('#quick', card).value.trim();
    if (!msg) return;
    const fp = filesPayload(qf);
    const mid = await run(() => call('ask_agent', { agentId: id, message: msg, workdir: fp.workdir, attachments: fp.attachments, allowEdit: fp.allow_edit }), `Đã giao việc cho ${a.name}`);
    $('#quick', card).value = '';
    st.quick[id] = { workdir: '', attachments: [], allowEdit: false };
    await loadMissions();
    openMission(mid);
  });
}

async function getTemplates() {
  if (!st.templates) st.templates = await call('list_templates');
  return st.templates;
}

function providerOptions(sel, onlyEnabled = false) {
  return st.providers.filter((p) => !onlyEnabled || p.enabled || p.id === sel).map((p) => `<option value="${p.id}" ${p.id === sel ? 'selected' : ''}>${esc(p.name)}${p.enabled ? '' : ' (tắt)'}</option>`).join('');
}

async function editAgent(a) {
  const t = await getTemplates();
  let avatar = a?.avatar || t.avatars[Math.floor(Math.random() * 13)];
  const def = st.agents.find((x) => x.is_dispatcher)?.provider_id || 'mock';
  modal(`<h3>${a ? 'Hồ sơ nhân viên' : 'Thêm nhân viên AI'}</h3>
    <form class="form" id="f">
      <div class="row"><label>Họ tên<input name="name" required value="${esc(a?.name || '')}"></label><label>Chức danh<input name="title" required value="${esc(a?.title || '')}"></label></div>
      <div class="row"><label>Phòng ban<input name="dept" value="${esc(a?.department || '')}"></label>
        <label>Cấp độ<select name="level">${t.levels.map((l) => `<option value="${l.key}" ${l.key === (a?.level || 'trung_cap') ? 'selected' : ''}>${l.label}</option>`).join('')}</select></label>
        <label>Màu nhận diện<input type="color" name="color" value="${a?.color || '#6366f1'}" style="height:40px;padding:3px"></label></div>
      <label>Kỹ năng (phân cách bằng dấu phẩy)<input name="skills" value="${esc((a?.skills || []).join(', '))}"></label>
      ${aiPickerHtml('ag', { provider_id: a?.provider_id || def, model: a?.model || '', effort: a?.effort || '' })}
      <label>Lời nhắc hệ thống (vai trò, phạm vi, cách trình bày)<textarea name="sys" rows="5">${esc(a?.system_prompt || '')}</textarea></label>
      <div><div class="hint-text" style="margin-bottom:6px">Nhân vật 3D</div><div class="avatars">${t.avatars.map((v) => `<button type="button" data-av="${v}" class="${v === avatar ? 'sel' : ''}"><img src="${avatarImg(v)}" alt="">${v.replace('character-', '')}</button>`).join('')}</div></div>
      <div class="modal-actions"><button type="button" class="btn" data-close>Huỷ</button><button class="btn primary">Lưu</button></div>
    </form>`, {
    wide: true,
    onMount: (b, close) => {
      bindAiPicker(b, 'ag');
      $$('[data-av]', b).forEach((x) => x.addEventListener('click', () => { avatar = x.dataset.av; $$('[data-av]', b).forEach((y) => y.classList.toggle('sel', y === x)); }));
      $('#f', b).addEventListener('submit', async (e) => {
        e.preventDefault();
        const f = new FormData(e.target);
        await run(() => call('save_agent', { agent: { id: a?.id ?? null, name: f.get('name'), title: f.get('title'), department: f.get('dept'), level: f.get('level'), skills: String(f.get('skills')).split(',').map((s) => s.trim()).filter(Boolean), system_prompt: f.get('sys'), ...(({ provider_id, model, effort }) => ({ provider_id, model, effort }))(aiPickerValue(e.target, 'ag')), avatar, color: f.get('color') } }), 'Đã lưu hồ sơ');
        close(true);
      });
    },
  });
}

// ───────────────────────────── Tuyển dụng ─────────────────────────────
let recruitState = { candidates: [], selected: new Set(), note: '', source: '' };
const PREFERRED = ['claude-cli', 'codex-cli', 'gemini-cli', 'anthropic-api', 'openai-api', 'gemini-api', 'qwen-cli', 'ollama'];
/** AI dùng để tuyển dụng: ưu tiên AI đang sẵn sàng trên máy. */
function recruiterProvider() {
  const ready = (st.scan?.providers || []).filter((p) => p.status === 'ready' && p.kind !== 'mock').map((p) => p.id);
  return PREFERRED.find((id) => ready.includes(id)) || ready[0] || st.agents.find((a) => a.is_dispatcher)?.provider_id || 'mock';
}

async function renderRecruit() {
  if (st.view !== 'recruit') return;
  const t = await getTemplates();
  if (!st.scan) await scanMachine().catch(() => null);
  const byLevel = Object.keys(LEVEL).map((lv) => [lv, t.templates.filter((x) => x.level === lv)]);
  const b = body(`
    <p class="muted" style="margin-top:0">Mô tả công việc của văn phòng — AI sẽ phân tích và đề xuất các ứng viên nhân viên phù hợp (chức danh, cấp độ, kỹ năng, lời nhắc hệ thống) và chọn AI/model hợp với từng vị trí. Bạn chọn người để tuyển.</p>
    <form class="form" id="f">
      <label>Mô tả công việc<textarea name="jd" rows="5" required placeholder="VD: Văn phòng cần xây dựng ứng dụng web quản lý bán hàng, viết nội dung quảng cáo trên mạng xã hội, phân tích số liệu doanh thu hàng tháng và trả lời khách hàng.">${esc(st.settings?.office_description || '')}</textarea></label>
      <div class="row">
        <label>Số ứng viên<select name="n">${[3, 4, 5, 6, 8].map((n) => `<option ${n === 4 ? 'selected' : ''}>${n}</option>`).join('')}</select></label>
        <label>AI tuyển dụng<select name="prov">${providerOptions(recruiterProvider(), true)}</select></label>
        <label style="align-self:end"><button class="btn primary wide" id="go"><i data-icon="sparkles"></i>AI đề xuất ứng viên</button></label>
      </div>
      <label class="check"><input type="checkbox" name="opt" checked> Tự chọn AI và model phù hợp nhất cho từng vị trí (trong số các AI đang sẵn sàng)</label>
    </form>
    <div id="cands"></div>
    <div class="section-title">Hoặc tuyển nhanh từ thư viện vị trí mẫu</div>
    ${byLevel.map(([lv, list]) => `<div class="hint-text" style="margin:10px 0 6px"><span class="badge lv-${lv}">${LEVEL[lv]}</span></div>
      <div class="cands">${list.map((x) => `<div class="cand"><h4>${esc(x.title)}</h4><p>${esc(x.department)} · ${x.skills.map(esc).join(', ')}</p>
        <div class="toolbar" style="margin:4px 0 0"><span class="grow"></span><button class="btn small" data-tpl="${x.key}"><i data-icon="plus"></i>Tuyển</button></div></div>`).join('')}</div>`).join('')}
  `);
  drawCandidates();
  $('#f', b).addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = new FormData(e.target);
    await doRecruit($('#go', b), f.get('jd'), +f.get('n'), f.get('prov'), !!f.get('opt'));
    drawCandidates();
  });
  $$('[data-tpl]', b).forEach((x) => x.addEventListener('click', async () => {
    const tp = t.templates.find((y) => y.key === x.dataset.tpl);
    const prov = $('[name=prov]', b).value;
    const cand = { name: '', title: tp.title, department: tp.department, level: tp.level, skills: tp.skills, system_prompt: tp.system_prompt, avatar: tp.avatar, color: tp.color, reason: '', provider_id: prov, model: '' };
    await run(() => call('hire', { candidates: [cand] }), `Đã tuyển ${tp.title}`);
  }));
}

async function doRecruit(btn, jd, n, prov, optimize) {
  btn.disabled = true;
  btn.innerHTML = `<i data-icon="sparkles">${icon('sparkles')}</i>AI đang sàng lọc…`;
  try {
    const r = await run(() => call('recruit', { jobDescription: jd, count: n, providerId: prov, optimize }));
    recruitState = { candidates: r.candidates, selected: new Set(r.candidates.map((_, i) => i)), note: r.note, source: r.source };
    return r;
  } finally {
    btn.disabled = false;
    btn.innerHTML = `<i data-icon="sparkles">${icon('sparkles')}</i>AI đề xuất ứng viên`;
  }
}

function drawCandidates(el = $('#cands'), onHired = null) {
  if (!el) return;
  const r = recruitState;
  if (!r.candidates.length) { el.innerHTML = ''; return; }
  el.innerHTML = `<div class="section-title">Ứng viên đề xuất · nguồn: ${esc(r.source)}</div>
    ${r.note ? `<p class="warn-text hint-text">${esc(r.note)}</p>` : ''}
    <div class="cands">${r.candidates.map((c, i) => `
      <div class="cand ${r.selected.has(i) ? 'sel' : ''}" data-i="${i}">
        <input type="checkbox" class="pick" ${r.selected.has(i) ? 'checked' : ''}>
        <div class="item-row"><div class="avatar" style="background:${c.color}"><img src="${avatarImg(c.avatar)}" style="width:34px;height:34px" alt=""></div>
          <div class="grow"><h4>${esc(c.name)}</h4><p>${esc(c.title)}</p></div></div>
        <div><span class="badge lv-${c.level}">${LEVEL[c.level] || c.level}</span> <span class="hint-text">${esc(c.department || '')}</span></div>
        <div class="skills">${c.skills.slice(0, 6).map((s) => `<span class="skill">${esc(s)}</span>`).join('')}</div>
        ${c.reason ? `<p>${esc(c.reason)}</p>` : ''}
        <div class="ai-pick"><select data-cp="${i}" title="AI cho nhân viên này">${providerOptions(c.provider_id)}</select><input data-cm="${i}" value="${esc(c.model || '')}" placeholder="model mặc định" title="Model"><select data-ce="${i}" title="Mức suy luận">${Object.entries(EFFORT).map(([k, l]) => `<option value="${k}" ${k === (c.effort || '') ? 'selected' : ''}>${l}</option>`).join('')}</select></div>
        ${c.ai_reason ? `<p class="ai-reason">${esc(c.ai_reason)}</p>` : ''}
        <details><summary>Lời nhắc hệ thống</summary><p>${esc(c.system_prompt)}</p></details>
      </div>`).join('')}</div>
    <div class="toolbar" style="margin-top:12px"><span class="grow"></span><button class="btn primary" id="hire"><i data-icon="check"></i>Tuyển ${r.selected.size} ứng viên đã chọn</button></div>`;
  paintIcons(el);
  $$('.cand[data-i]', el).forEach((x) => x.addEventListener('click', (e) => {
    if (e.target.closest('details, select, input:not(.pick)')) return;
    const i = +x.dataset.i;
    r.selected.has(i) ? r.selected.delete(i) : r.selected.add(i);
    drawCandidates(el, onHired);
  }));
  $$('[data-cp]', el).forEach((x) => x.addEventListener('change', () => {
    const c = r.candidates[+x.dataset.cp];
    c.provider_id = x.value;
    c.model = '';
    c.ai_reason = 'Bạn tự chọn AI cho vị trí này.';
    drawCandidates(el, onHired);
  }));
  $$('[data-cm]', el).forEach((x) => x.addEventListener('input', () => { r.candidates[+x.dataset.cm].model = x.value.trim(); }));
  $$('[data-ce]', el).forEach((x) => x.addEventListener('change', () => { r.candidates[+x.dataset.ce].effort = x.value; }));
  $('#hire', el).addEventListener('click', async () => {
    const list = r.candidates.filter((_, i) => r.selected.has(i));
    if (!list.length) return toast('Chưa chọn ứng viên nào', 'err');
    await run(() => call('hire', { candidates: list }), `Chào mừng ${list.length} nhân viên mới!`);
    recruitState = { candidates: [], selected: new Set(), note: '', source: '' };
    if (onHired) return onHired(list);
    drawCandidates(el);
    closePanel();
  });
}

// ───────────────────────────── Kết nối AI: quét máy, cài đặt, đăng nhập ─────────────────────────────
st.scan = null;
st.installLogs = {};
st.onMachineChange = null;
const SCAN_BADGE = { ready: ['ok-text', 'Sẵn sàng'], missing: ['warn-text', 'Chưa cài'], need_key: ['warn-text', 'Chưa có API key'], offline: ['warn-text', 'Chưa chạy'], disabled: ['muted', 'Đang tắt'] };

async function scanMachine() {
  st.scan = await call('scan_machine');
  return st.scan;
}
const readyAIs = () => (st.scan?.providers || []).filter((p) => p.status === 'ready' && p.kind !== 'mock');

function machineHtml(compact) {
  const s = st.scan;
  if (!s) return '<div class="muted">Đang quét máy…</div>';
  const t = s.tools;
  const used = (id) => st.agents.filter((a) => a.provider_id === id).length;
  const tools = `<div class="kv machine">
      <span class="badge ${t.node ? 'xong' : 'loi'}">Node.js ${t.node ? esc(t.node) : 'chưa có'}</span>
      <span class="badge ${t.npm ? 'xong' : 'loi'}">npm ${t.npm ? '✓' : '✗'}</span>
      <span class="badge ${t.brew ? 'xong' : 'cho'}">Homebrew ${t.brew ? '✓' : '—'}</span>
      <span class="badge ${readyAIs().length ? 'xong' : 'cho_duyet'}">${readyAIs().length} AI sẵn sàng</span>
      ${!t.npm && can('manager') ? (s.installing.includes('node') ? '<span class="badge working"><span class="mini-spin"></span>Đang cài Node.js…</span><button class="btn small danger" data-cancel="node">Huỷ</button>' : t.brew ? '<button class="btn small" data-install="node"><i data-icon="download"></i>Cài Node.js</button>' : '<span class="hint-text">Cài Node.js từ nodejs.org để tự cài được các CLI</span>') : ''}
    </div>${st.installLogs.node ? `<pre class="install-log" id="log-node">${esc(st.installLogs.node.slice(-14).join('\n'))}</pre>` : ''}`;
  const rows = s.providers.filter((p) => !compact || (p.kind !== 'mock' && (p.enabled || p.status === 'missing'))).map((p) => {
    const [cls, label] = SCAN_BADGE[p.status] || ['muted', p.status];
    const inst = s.installing.includes(p.id);
    let actions = '';
    if (can('manager')) {
      if (inst) actions += `<span class="badge working"><span class="mini-spin"></span>Đang cài…</span><button class="btn small danger" data-cancel="${p.id}"><i data-icon="stop"></i>Huỷ</button>`;
      else if (p.status === 'missing' && p.installable) actions += `<button class="btn small primary" data-install="${p.id}"><i data-icon="download"></i>Cài đặt</button>`;
      if (p.status === 'need_key') actions += `<button class="btn small primary" data-key="${p.id}"><i data-icon="key"></i>Nhập API key</button>`;
      if (p.kind === 'cli' && p.status !== 'missing' && !inst && p.can_login) actions += `<button class="btn small" data-login="${p.id}"><i data-icon="logout"></i>Đăng nhập</button>`;
      if (p.kind !== 'mock' && (p.status === 'ready' || p.status === 'disabled')) actions += `<button class="btn small" data-live="${p.id}"><i data-icon="bolt"></i>Thử gọi AI</button>`;
      if (!compact) actions += `<button class="btn small" data-editp="${p.id}"><i data-icon="edit"></i>Sửa</button>`;
    }
    const logs = st.installLogs[p.id];
    return `<div class="item static">
      <div class="item-row"><div class="grow"><h4>${esc(p.name)}</h4>
        <p>${KIND[p.kind] || p.kind} · <span class="${cls}">● ${label}</span>${p.version ? ' · ' + esc(p.version) : ''}${compact ? '' : ` · ${used(p.id)} nhân viên`}</p>
        <p>${esc(p.detail)}${p.path ? ` <code class="inline">${esc(p.path)}</code>` : ''}</p></div>
        <div class="prov-actions">${actions}</div></div>
      ${logs ? `<pre class="install-log" id="log-${p.id}">${esc(logs.slice(-14).join('\n'))}</pre>` : ''}
    </div>`;
  }).join('');
  return tools + `<div class="list">${rows}</div>`;
}

function bindMachine(root) {
  const refresh = () => scanMachine().then(() => st.onMachineChange?.());
  $$('[data-install]', root).forEach((x) => x.addEventListener('click', async () => {
    const id = x.dataset.install;
    st.installLogs[id] = ['Đang chuẩn bị cài đặt…'];
    await run(() => call('install_cli', { id }));
    st.scan.installing.push(id);
    refresh();
  }));
  $$('[data-cancel]', root).forEach((x) => x.addEventListener('click', async () => {
    x.disabled = true;
    await run(() => call('cancel_install', { id: x.dataset.cancel }), 'Đã gửi lệnh huỷ cài đặt');
  }));
  $$('[data-login]', root).forEach((x) => x.addEventListener('click', async () => {
    await run(() => call('open_cli_login', { id: x.dataset.login }), 'Đã mở cửa sổ dòng lệnh — đăng nhập xong hãy bấm "Thử gọi AI"');
  }));
  $$('[data-live]', root).forEach((x) => x.addEventListener('click', async () => {
    x.disabled = true;
    x.innerHTML = '<span class="mini-spin"></span>Đang gọi…';
    try {
      const m = await call('test_provider', { id: x.dataset.live, live: true });
      toast(m, 'ok');
    } catch (err) {
      toast('Chưa gọi được: ' + String(err?.message || err) + ' — có thể cần đăng nhập.', 'err');
    } finally { refresh(); }
  }));
  $$('[data-key]', root).forEach((x) => x.addEventListener('click', () => {
    const p = st.providers.find((y) => y.id === x.dataset.key);
    const item = x.closest('.item');
    if (!p || $('.key-row', item)) return;
    const row = document.createElement('form');
    row.className = 'key-row';
    row.innerHTML = `<input type="password" name="k" required autocomplete="off" placeholder="Dán API key của ${esc(p.name)}"><button class="btn small primary">Lưu</button>`;
    item.appendChild(row);
    $('input', row).focus();
    row.addEventListener('submit', async (e) => {
      e.preventDefault();
      const prov = { ...p, api_key: $('input', row).value.trim(), enabled: true };
      delete prov.has_key;
      await run(() => call('save_provider', { provider: prov }), 'Đã lưu API key');
      await loadProviders();
      refresh();
    });
  }));
  $$('[data-editp]', root).forEach((x) => x.addEventListener('click', () => editProvider(st.providers.find((p) => p.id === x.dataset.editp), refresh)));
}

function onInstallEvent(ev) {
  const id = ev.id;
  if (!st.installLogs[id]) st.installLogs[id] = [];
  if (ev.line) {
    st.installLogs[id].push(ev.line);
    const el = document.getElementById('log-' + id);
    if (el) { el.textContent = st.installLogs[id].slice(-14).join('\n'); el.scrollTop = el.scrollHeight; }
  }
  if (ev.done) {
    toast(ev.ok ? `${ev.message}${ev.path ? ' — ' + ev.path : ''}` : `Cài đặt chưa thành công: ${ev.message}`, ev.ok ? 'ok' : 'err');
    if (ev.ok) delete st.installLogs[id];
    scanMachine().then(() => st.onMachineChange?.());
  }
}

async function renderProviders() {
  if (st.view !== 'providers') return;
  st.onMachineChange = async () => { if (st.view === 'providers') { await loadProviders(); renderProvidersBody(); } };
  await Promise.all([loadProviders(), scanMachine()]);
  renderProvidersBody();
}

function renderProvidersBody() {
  if (st.view !== 'providers') return;
  const b = body(`
    <p class="muted" style="margin-top:0">Mỗi nhân viên dùng một AI. <b>CLI</b> (Claude Code, Codex, Gemini…) chạy trên máy với tài khoản bạn đã đăng nhập; <b>API</b> dùng khoá của bạn. Bấm <b>Cài đặt</b> để tự cài CLI còn thiếu, <b>Đăng nhập</b> để mở Terminal đăng nhập, <b>Thử gọi AI</b> để kiểm tra thật.</p>
    <div class="toolbar"><button class="btn small" id="scan"><i data-icon="refresh"></i>Quét máy</button><span class="grow"></span>
      ${can('manager') ? `<button class="btn small" id="opt"><i data-icon="brain"></i>Tối ưu AI cho nhân viên</button><button class="btn small" id="wiz"><i data-icon="sparkles"></i>Trợ lý thiết lập</button><button class="btn small primary" id="addp"><i data-icon="plus"></i>Thêm kết nối</button>` : ''}</div>
    ${machineHtml(false)}`);
  bindMachine(b);
  $('#scan', b).addEventListener('click', async () => {
    await run(() => call('refresh_path').catch(() => null));
    await scanMachine();
    renderProvidersBody();
    toast(`Đã quét máy: ${readyAIs().length} AI sẵn sàng`, 'ok');
  });
  $('#opt', b)?.addEventListener('click', () => optimizeModal());
  $('#wiz', b)?.addEventListener('click', () => setupWizard());
  $('#addp', b)?.addEventListener('click', () => editProvider(null, () => st.onMachineChange?.()));
}

// ───────────────────────────── Tối ưu AI cho nhân viên hiện có ─────────────────────────────
async function optimizeModal(onlyStale = false) {
  if (!st.scan) await scanMachine();
  const items = await run(() => call('optimize_preview'));
  if (!items.length) return toast('Chưa có AI nào sẵn sàng. Hãy cài/đăng nhập một CLI hoặc nhập API key trước.', 'err');
  const ready = new Set(readyAIs().map((p) => p.id));
  const pre = (it) => it.changed && (!onlyStale || !ready.has(it.current_provider));
  const names = readyAIs().map((p) => p.name).join(', ');
  modal(`<h3>Tối ưu AI cho từng nhân viên</h3>
    <p class="muted">AI đang sẵn sàng: <b>${esc(names)}</b>. Hệ thống ghép AI theo thế mạnh với chuyên môn của từng vị trí; việc chuyên sâu dùng model mạnh, việc cơ bản dùng model nhẹ để tiết kiệm, và chia đều khi các AI ngang nhau.</p>
    <table class="grid"><thead><tr><th></th><th>Nhân viên</th><th>Hiện tại</th><th>Đề xuất</th></tr></thead><tbody>
    ${items.map((it, i) => `<tr><td><input type="checkbox" data-o="${i}" ${pre(it) ? 'checked' : ''} ${it.changed ? '' : 'disabled'}></td>
      <td><b>${esc(it.name)}</b><div class="hint-text">${esc(it.title)} · ${LEVEL[it.level] || ''}</div></td>
      <td>${esc(aiLabel(it.current_provider, it.current_model, it.current_effort))}</td>
      <td>${it.changed ? '' : '<span class="ok-text">Đã tối ưu · </span>'}${esc(aiLabel(it.provider_id, it.model, it.effort))}<div class="hint-text">${esc(it.reason)}</div></td></tr>`).join('')}
    </tbody></table>
    <div class="modal-actions"><button class="btn" data-close>Để sau</button><button class="btn primary" id="apply"><i data-icon="check"></i>Áp dụng</button></div>`, {
    wide: true,
    onMount: (b, close) => $('#apply', b).addEventListener('click', async () => {
      const sel = $$('[data-o]:checked', b).map((x) => items[+x.dataset.o]).map((it) => ({ agent_id: it.agent_id, provider_id: it.provider_id, model: it.model, effort: it.effort || '' }));
      if (sel.length) await run(() => call('apply_optimization', { items: sel }), `Đã đổi AI cho ${sel.length} nhân viên`);
      close(true);
    }),
  });
}

// ───────────────────────────── Trợ lý thiết lập lần đầu ─────────────────────────────
async function setupWizard() {
  let step = 1;
  await Promise.all([loadProviders(), scanMachine()]);
  const prev = st.onMachineChange;
  modal('<div id="wiz"></div>', {
    wide: true,
    onMount: (box, close) => {
      const w = $('#wiz', box);
      const finish = async (hired) => {
        await call('set_onboarded', { done: true }).catch(() => {});
        st.onMachineChange = prev;
        close(true);
        if (hired) {
          toast(`Văn phòng đã có thêm ${hired.length} nhân viên. Bấm "Giao nhiệm vụ" để bắt đầu!`, 'ok');
          // nhân viên có sẵn đang dùng AI chưa sẵn sàng (VD mô phỏng) → đề xuất đổi
          const stale = st.agents.some((a) => !readyAIs().some((p) => p.id === a.provider_id));
          if (stale && readyAIs().length) setTimeout(() => optimizeModal(true), 600);
        }
      };
      st.onMachineChange = async () => { await loadProviders(); if (step === 1) draw(); };
      const head = () => `<div class="wiz-steps">${['Kết nối AI', 'Mô tả công việc', 'Chọn nhân viên'].map((s, i) => `<span class="${i + 1 === step ? 'on' : i + 1 < step ? 'done' : ''}"><b>${i + 1}</b>${s}</span>`).join('')}</div>`;
      const draw = () => {
        if (step === 1) {
          const n = readyAIs().length;
          w.innerHTML = `${head()}<h3>Kết nối AI cho văn phòng</h3>
            <p class="muted">Nhân viên AI cần ít nhất một AI để làm việc. Cài CLI còn thiếu bằng một cú bấm, đăng nhập qua Terminal, hoặc nhập API key. Có nhiều AI thì văn phòng sẽ tự phân bổ AI phù hợp cho từng vị trí.</p>
            ${machineHtml(true)}
            <div class="modal-actions"><button class="btn ghost" id="later">Để sau</button><span style="flex:1"></span>
              <button class="btn" id="rescan"><i data-icon="refresh"></i>Quét lại</button>
              ${n ? `<button class="btn primary" id="next">Tiếp tục với ${n} AI <i data-icon="send"></i></button>` : '<button class="btn" id="next">Dùng chế độ mô phỏng</button>'}</div>`;
          paintIcons(w);
          bindMachine(w);
          $('#rescan', w).addEventListener('click', async () => { await call('refresh_path').catch(() => null); await scanMachine(); draw(); });
          $('#later', w).addEventListener('click', () => {
            // "Để sau": không tự hỏi lại; mở lại ở Kết nối AI → Trợ lý thiết lập
            call('set_onboarded', { done: true }).catch(() => {});
            if (st.settings) st.settings.onboarded = true;
            st.onMachineChange = prev;
            close(null);
            toast('Có thể mở lại ở mục Kết nối AI → Trợ lý thiết lập');
          });
          $('#next', w).addEventListener('click', () => { step = 2; draw(); });
        } else if (step === 2) {
          w.innerHTML = `${head()}<h3>Văn phòng của bạn làm gì?</h3>
            <p class="muted">Mô tả công việc, lĩnh vực, sản phẩm đầu ra mong muốn. AI sẽ đề xuất đội ngũ nhân viên phù hợp từ cơ bản đến chuyên gia${readyAIs().length > 1 ? ` và phân bổ ${readyAIs().length} AI đang có cho từng vị trí` : ''}.</p>
            <form class="form" id="wf">
              <label>Tên văn phòng<input name="name" value="${esc(st.settings?.office_name || 'Văn phòng AI')}"></label>
              <label>Mô tả công việc<textarea name="jd" rows="6" required minlength="10" placeholder="VD: Phòng kế hoạch của bệnh viện: soạn báo cáo, kế hoạch, tờ trình; tổng hợp số liệu khám chữa bệnh hằng tháng; rà soát văn bản pháp quy; dịch tài liệu chuyên môn tiếng Anh.">${esc(st.settings?.office_description || '')}</textarea></label>
              <div class="row"><label>Số nhân viên đề xuất<select name="n">${[3, 4, 5, 6, 8].map((x) => `<option ${x === 5 ? 'selected' : ''}>${x}</option>`).join('')}</select></label>
                <label>AI dùng để tuyển<select name="prov">${providerOptions(recruiterProvider(), true)}</select></label></div>
              <label class="check"><input type="checkbox" name="opt" checked> Tự chọn AI và model phù hợp cho từng vị trí</label>
              <div class="modal-actions"><button type="button" class="btn" id="back">← Quay lại</button><span style="flex:1"></span><button class="btn primary" id="go"><i data-icon="sparkles"></i>AI đề xuất nhân viên</button></div>
            </form>`;
          paintIcons(w);
          $('#back', w).addEventListener('click', () => { step = 1; draw(); });
          $('#wf', w).addEventListener('submit', async (e) => {
            e.preventDefault();
            const f = new FormData(e.target);
            await call('save_settings', { officeName: f.get('name') || 'Văn phòng AI', officeDescription: f.get('jd') }).catch(() => {});
            await loadSettings();
            refreshStats();
            await doRecruit($('#go', w), f.get('jd'), +f.get('n'), f.get('prov'), !!f.get('opt'));
            step = 3;
            draw();
          });
        } else {
          w.innerHTML = `${head()}<h3>Chọn nhân viên cho văn phòng</h3>
            <p class="muted">Bấm vào thẻ để chọn/bỏ. Có thể đổi AI hoặc model cho từng người trước khi tuyển.</p>
            <div id="wc"></div>
            <div class="modal-actions"><button class="btn" id="back">← Mô tả lại</button></div>`;
          $('#back', w).addEventListener('click', () => { step = 2; draw(); });
          drawCandidates($('#wc', w), (list) => finish(list));
        }
      };
      draw();
    },
  });
}


function editProvider(p, onSaved = null) {
  const isNew = !p;
  p = p || { id: '', name: '', kind: 'cli', command: '', args: '{prompt}', model_args: '', use_stdin: false, base_url: '', default_model: '', timeout_secs: 900, enabled: true, builtin: false, notes: '', has_key: false };
  modal(`<h3>${isNew ? 'Thêm kết nối AI' : 'Sửa kết nối: ' + esc(p.name)}</h3>
    <form class="form" id="f">
      <div class="row"><label>Tên hiển thị<input name="name" required value="${esc(p.name)}"></label>
        <label>Loại<select name="kind" ${p.builtin ? 'disabled' : ''}>${Object.entries(KIND).map(([k, v]) => `<option value="${k}" ${k === p.kind ? 'selected' : ''}>${v}</option>`).join('')}</select></label></div>
      <div data-k="cli" class="form">
        <div class="row"><label>Lệnh (tên hoặc đường dẫn đầy đủ)<input name="command" value="${esc(p.command)}" placeholder="claude, codex, gemini, /usr/local/bin/aider…"></label></div>
        <label>Mẫu tham số<input name="args" value="${esc(p.args)}" style="font-family:var(--mono)"></label>
        <label>Tham số chọn model<input name="model_args" value="${esc(p.model_args)}" placeholder="--model {model}" style="font-family:var(--mono)"></label>
        <div class="hint-text">Biến thay thế: <code class="inline">{prompt}</code> nội dung yêu cầu · <code class="inline">{model}</code> · <code class="inline">{model_args}</code> chèn tham số model khi có · <code class="inline">{output_file}</code> tệp nhận kết quả · <code class="inline">{workdir}</code> thư mục làm việc riêng của nhân viên.</div>
        <label class="check"><input type="checkbox" name="stdin" ${p.use_stdin ? 'checked' : ''}> Gửi yêu cầu qua stdin thay vì tham số</label>
      </div>
      <div data-k="api" class="form">
        <label>Địa chỉ API (base URL)<input name="base_url" value="${esc(p.base_url)}" placeholder="https://api.openai.com/v1"></label>
        <label>API key ${p.has_key ? '<span class="ok-text">(đã lưu — bỏ trống để giữ nguyên)</span>' : ''}<input name="api_key" type="password" autocomplete="off" placeholder="${p.has_key ? '••••••••' : 'Dán khoá vào đây'}"></label>
      </div>
      <div class="row"><label>Model mặc định<input name="default_model" value="${esc(p.default_model)}"></label>
        <label>Thời gian chờ tối đa (giây)<input name="timeout" type="number" min="10" max="7200" value="${p.timeout_secs}"></label></div>
      <label>Ghi chú<input name="notes" value="${esc(p.notes)}"></label>
      <label class="check"><input type="checkbox" name="enabled" ${p.enabled ? 'checked' : ''}> Bật kết nối này</label>
      <div class="modal-actions">${!isNew && !p.builtin ? '<button type="button" class="btn danger" id="delp">Xoá</button><span style="flex:1"></span>' : ''}<button type="button" class="btn" data-close>Huỷ</button><button class="btn primary">Lưu</button></div>
    </form>`, {
    onMount: (b, close) => {
      const kindSel = $('[name=kind]', b);
      const sync = () => {
        const k = kindSel.value;
        $('[data-k=cli]', b).classList.toggle('hidden', k !== 'cli');
        $('[data-k=api]', b).classList.toggle('hidden', !['anthropic', 'openai', 'gemini'].includes(k));
      };
      kindSel.addEventListener('change', sync);
      sync();
      $('#delp', b)?.addEventListener('click', async () => {
        await run(() => call('delete_provider', { id: p.id }), 'Đã xoá kết nối');
        close(true);
        loadAgents();
        onSaved ? onSaved() : renderProviders();
      });
      $('#f', b).addEventListener('submit', async (e) => {
        e.preventDefault();
        const f = new FormData(e.target);
        const prov = { ...p, name: f.get('name'), kind: kindSel.value, command: f.get('command') || '', args: f.get('args') || '', model_args: f.get('model_args') || '', use_stdin: !!f.get('stdin'), base_url: f.get('base_url') || '', api_key: f.get('api_key') || '', default_model: f.get('default_model') || '', timeout_secs: +f.get('timeout') || 900, enabled: !!f.get('enabled'), notes: f.get('notes') || '' };
        delete prov.has_key;
        await run(() => call('save_provider', { provider: prov }), 'Đã lưu kết nối');
        close(true);
        onSaved ? onSaved() : renderProviders();
      });
    },
  });
}

// ───────────────────────────── Người dùng ─────────────────────────────
async function renderUsers() {
  if (st.view !== 'users') return;
  st.users = await call('list_users');
  const b = body(`
    <p class="muted" style="margin-top:0"><b>Quản lý</b>: toàn quyền (tuyển dụng, kết nối AI, người dùng). <b>Điều phối</b>: giao nhiệm vụ, phân công đầu việc cho nhân viên cụ thể, duyệt và chạy. <b>Quan sát</b>: chỉ xem.</p>
    <div class="toolbar"><span class="grow"></span><button class="btn small primary" id="addu"><i data-icon="plus"></i>Thêm người dùng</button></div>
    <table class="grid"><thead><tr><th>Họ tên</th><th>Tên đăng nhập</th><th>Vai trò</th><th>Trạng thái</th><th></th></tr></thead><tbody>
    ${st.users.map((u) => `<tr><td>${esc(u.display_name)}${u.id === st.user.id ? ' <span class="muted">(bạn)</span>' : ''}</td><td><code class="inline">${esc(u.username)}</code></td><td>${ROLE[u.role]}</td>
      <td>${u.active ? '<span class="ok-text">Hoạt động</span>' : '<span class="warn-text">Đã khoá</span>'}</td>
      <td style="text-align:right"><button class="icon-btn" data-u="${u.id}"><i data-icon="edit"></i></button>${u.id !== st.user.id ? `<button class="icon-btn" data-du="${u.id}"><i data-icon="trash"></i></button>` : ''}</td></tr>`).join('')}
    </tbody></table>`);
  $('#addu', b).addEventListener('click', () => editUser(null));
  $$('[data-u]', b).forEach((x) => x.addEventListener('click', () => editUser(st.users.find((u) => u.id === +x.dataset.u))));
  $$('[data-du]', b).forEach((x) => x.addEventListener('click', async () => {
    const u = st.users.find((y) => y.id === +x.dataset.du);
    if (await confirmBox('Xoá người dùng?', `Xoá tài khoản ${u.display_name}?`)) {
      await run(() => call('delete_user', { id: u.id }), 'Đã xoá');
      renderUsers();
    }
  }));
}

function editUser(u) {
  modal(`<h3>${u ? 'Sửa người dùng' : 'Thêm người dùng'}</h3><form class="form" id="f">
    <div class="row"><label>Họ tên<input name="display" required value="${esc(u?.display_name || '')}"></label>
      <label>Tên đăng nhập<input name="username" required ${u ? 'disabled' : ''} value="${esc(u?.username || '')}"></label></div>
    <div class="row"><label>Vai trò<select name="role">${Object.entries(ROLE).map(([k, v]) => `<option value="${k}" ${k === (u?.role || 'coordinator') ? 'selected' : ''}>${v}</option>`).join('')}</select></label>
      <label>${u ? 'Mật khẩu mới (bỏ trống nếu giữ nguyên)' : 'Mật khẩu'}<input type="password" name="pw" ${u ? '' : 'required minlength="6"'}></label></div>
    <label class="check"><input type="checkbox" name="active" ${!u || u.active ? 'checked' : ''}> Cho phép đăng nhập</label>
    <div class="modal-actions"><button type="button" class="btn" data-close>Huỷ</button><button class="btn primary">Lưu</button></div></form>`, {
    onMount: (b, close) => $('#f', b).addEventListener('submit', async (e) => {
      e.preventDefault();
      const f = new FormData(e.target);
      await run(() => call('save_user', { user: { id: u?.id ?? null, username: u?.username || f.get('username'), display_name: f.get('display'), role: f.get('role'), active: !!f.get('active'), password: f.get('pw') || '' } }), 'Đã lưu người dùng');
      close(true);
      renderUsers();
    }),
  });
}

// ───────────────────────────── Nhật ký & cài đặt ─────────────────────────────
async function renderActivity() {
  if (st.view !== 'activity') return;
  const items = await call('list_activity', { limit: 200 });
  body(`<div class="list">${items.map((i) => `<div class="item static" ${i.mission_id ? `data-m="${i.mission_id}" style="cursor:pointer"` : ''}>
    <div class="item-row"><div class="grow"><h4 style="white-space:normal;font-weight:500"><b>${esc(i.actor)}</b> — ${esc(i.message)}</h4><p>${esc(i.ts)}</p></div>
    ${i.mission_id ? `<span class="badge">#${i.mission_id}</span>` : ''}</div></div>`).join('') || '<div class="empty">Chưa có hoạt động</div>'}</div>`);
  $$('[data-m]').forEach((x) => x.addEventListener('click', () => openMission(+x.dataset.m)));
}

async function renderSettings() {
  if (st.view !== 'settings') return;
  const info = await call('app_info');
  const s = await call('get_settings');
  const b = body(`<form class="form" id="f">
      <label>Tên văn phòng<input name="name" value="${esc(s.office_name)}" required></label>
      <label>Mô tả văn phòng (được gửi kèm làm bối cảnh cho mọi nhân viên AI)<textarea name="desc" rows="6">${esc(s.office_description)}</textarea></label>
      <div class="toolbar"><span class="grow"></span><button class="btn primary">Lưu cài đặt</button></div></form>
    <div class="section-title">Phong cách văn phòng</div>
    <div id="styles">${styleCards(st.style || 'hien_dai')}</div>
    <div class="section-title">Thông tin</div>
    <div class="box"><p style="margin:0">Phiên bản: <b>${esc(info.version)}</b> ${isDesktop ? '' : '(bản xem thử trên trình duyệt)'}</p>
      <p style="margin:6px 0 0">Thư mục dữ liệu: <code class="inline">${esc(info.data_dir)}</code></p>
      <p class="hint-text" style="margin:6px 0 0">Cơ sở dữ liệu <code class="inline">office.db</code> và thư mục làm việc riêng của từng nhân viên (<code class="inline">workspaces/</code>) nằm ở đây. Sao lưu thư mục này để giữ toàn bộ văn phòng.</p>
      <div class="toolbar" style="margin:10px 0 0"><button class="btn small" id="upd-check"><i data-icon="download"></i>Kiểm tra cập nhật</button><span class="hint-text" id="upd-status"></span></div>
      <p class="hint-text" style="margin:6px 0 0">Mô hình 3D: Kenney Furniture Kit, Mini Characters & Blocky Characters (giấy phép CC0, kenney.nl).</p></div>`);
  bindStyleCards($('#styles', b), async (k) => {
    await run(() => call('set_office_style', { style: k }), 'Đã đổi phong cách văn phòng');
    st.style = k;
    office.setTheme(k);
  });
  $('#upd-check', b).addEventListener('click', () => checkForUpdates(true));
  $('#f', b).addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = new FormData(e.target);
    await run(() => call('save_settings', { officeName: f.get('name'), officeDescription: f.get('desc') }), 'Đã lưu cài đặt');
    loadSettings().then(refreshStats);
  });
}

// ───────────────────────────── Tự cập nhật ─────────────────────────────
/** Kiểm tra bản mới trên GitHub Releases. manual=true: báo kết quả ra màn hình; false: chỉ hỏi khi có bản mới. */
async function checkForUpdates(manual = false) {
  const status = $('#upd-status');
  const setStatus = (t) => { if (status) status.textContent = t; };
  const U = window.__TAURI__?.updater;
  if (!U) { if (manual) toast('Cập nhật chỉ hoạt động trong ứng dụng cài đặt', 'err'); return; }
  try {
    setStatus('Đang kiểm tra…');
    const up = await U.check();
    if (!up) { setStatus('Bạn đang dùng bản mới nhất.'); if (manual) toast('Bạn đang dùng bản mới nhất', 'ok'); return; }
    setStatus(`Có bản mới ${up.version}`);
    const ok = await confirmBox('Có bản cập nhật mới', `Phiên bản ${up.version} đã sẵn sàng${up.body ? ':\n' + up.body.slice(0, 300) : '.'}\nTải và cài ngay? Ứng dụng sẽ khởi động lại.`, false);
    if (!ok) return;
    setStatus('Đang tải bản cập nhật…');
    await up.downloadAndInstall((ev) => {
      if (ev.event === 'Progress') setStatus(`Đang tải… ${Math.round((ev.data.chunkLength || 0) / 1024)} KB`);
    });
    await window.__TAURI__.process.relaunch();
  } catch (e) {
    setStatus('');
    if (manual) toast('Không kiểm tra được cập nhật: ' + (e?.message || e), 'err');
    else console.warn('update check', e);
  }
}

setInterval(refreshStats, 30000);
setTimeout(() => checkForUpdates(false), 8000);
boot();
