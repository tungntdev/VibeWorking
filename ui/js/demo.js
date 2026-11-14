// Bản giả lập backend để xem trước giao diện trong trình duyệt (không dùng trong ứng dụng desktop).

const listeners = [];
const emit = (e) => listeners.forEach((f) => f(e));
export const demoListen = (f) => listeners.push(f);
const now = () => { const d = new Date(); return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().replace('T', ' ').slice(0, 19); };
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const S = {
  users: [],
  settings: { office_name: 'Văn phòng AI (bản xem thử)', office_description: '' },
  providers: [
    { id: 'mock', name: 'Mô phỏng (không cần AI)', kind: 'mock', command: '', args: '', model_args: '', use_stdin: false, base_url: '', default_model: '', timeout_secs: 60, enabled: true, builtin: true, notes: 'Trả lời mẫu', has_key: false },
    { id: 'claude-cli', name: 'Claude Code CLI', kind: 'cli', command: 'claude', args: '-p {prompt} --output-format text {model_args}', model_args: '--model {model}', use_stdin: false, base_url: '', default_model: '', timeout_secs: 900, enabled: true, builtin: true, notes: '', has_key: false },
    { id: 'codex-cli', name: 'OpenAI Codex CLI', kind: 'cli', command: 'codex', args: 'exec --skip-git-repo-check {prompt}', model_args: '-m {model}', use_stdin: false, base_url: '', default_model: '', timeout_secs: 900, enabled: true, builtin: true, notes: '', has_key: false },
    { id: 'gemini-cli', name: 'Google Gemini CLI', kind: 'cli', command: 'gemini', args: '-p {prompt}', model_args: '-m {model}', use_stdin: false, base_url: '', default_model: '', timeout_secs: 900, enabled: true, builtin: true, notes: '', has_key: false },
    { id: 'anthropic-api', name: 'Claude API (Anthropic)', kind: 'anthropic', command: '', args: '', model_args: '', use_stdin: false, base_url: 'https://api.anthropic.com', default_model: 'claude-opus-5-5', timeout_secs: 600, enabled: true, builtin: true, notes: '', has_key: false },
  ],
  agents: [],
  missions: [],
  subtasks: [],
  activity: [],
  seq: 100,
};
const id = () => ++S.seq;
// Tải lại trang trong bản xem thử: giữ phiên đăng nhập giả lập.
try { if (sessionStorage.getItem('aio_token') === 'demo') S.users.push({ id: 1, username: 'quanly', display_name: 'Quản lý Demo', role: 'manager', active: true, created_at: now() }); } catch {}
const A = (name, title, dept, level, skills, avatar, color, disp = false) => ({
  id: id(), name, title, department: dept, level, skills, system_prompt: `Bạn là ${title}.`, provider_id: 'mock', model: '',
  avatar, color, is_dispatcher: disp, desk: disp ? -1 : S.agents.filter((a) => !a.is_dispatcher).length, status: 'idle', tasks_done: 0, created_at: now(),
});
S.agents.push(A('Điều phối viên Minh An', 'Điều phối viên', 'Ban điều hành', 'chuyen_gia', ['Phân rã công việc', 'Phân công'], 'character-robot', '#f59e0b', true));
for (const x of [
  ['Trần Thu Hà', 'Thư ký hành chính', 'Hành chính', 'co_ban', ['Soạn email', 'Lập lịch'], 'character-female-a', '#ec4899'],
  ['Lê Đức Huy', 'Lập trình viên Full-stack', 'Công nghệ', 'chuyen_nghiep', ['Rust', 'TypeScript'], 'character-male-e', '#6366f1'],
  ['Phạm Ngọc Linh', 'Chuyên viên phân tích dữ liệu', 'Phân tích', 'trung_cap', ['SQL', 'Python'], 'character-female-c', '#3b82f6'],
  ['Vũ Quang Nam', 'Kỹ sư kiểm thử (QA)', 'Công nghệ', 'trung_cap', ['Test case'], 'character-male-b', '#84cc16'],
  ['Đỗ Minh Thảo', 'Chuyên viên nội dung & marketing', 'Truyền thông', 'trung_cap', ['Viết bài', 'SEO'], 'character-female-d', '#f97316'],
]) S.agents.push(A(...x));

function log(kind, actor, message, mission_id = null, agent_id = null) {
  const item = { id: id(), ts: now(), kind, actor, message, mission_id, agent_id };
  S.activity.unshift(item);
  emit({ type: 'activity', item });
}
const setAgent = (aid, status, mission_id, note = '') => {
  const a = S.agents.find((x) => x.id === aid); if (a) a.status = status;
  emit({ type: 'agent', id: aid, status, mission_id, note });
};
const setMission = (m, status) => { m.status = status; m.updated_at = now(); emit({ type: 'mission', id: m.id, status }); };

let runMission = async function (m, plan, exec) {
  const disp = S.agents.find((a) => a.is_dispatcher);
  if (plan) {
    setMission(m, 'dang_lap_ke_hoach');
    setAgent(disp.id, 'thinking', m.id, 'Đang lập kế hoạch');
    await wait(1800);
    const staff = S.agents.filter((a) => !a.is_dispatcher);
    const pick = staff.sort(() => Math.random() - 0.5).slice(0, Math.min(3, staff.length));
    S.subtasks = S.subtasks.filter((s) => s.mission_id !== m.id);
    pick.forEach((a, i) => {
      const s = { id: id(), mission_id: m.id, agent_id: a.id, agent_name: a.name, step: i < 2 ? 1 : 2, title: `${m.title} — phần của ${a.title}`, instruction: m.description, status: 'cho', output: '', error: '', started_at: '', finished_at: '', duration_ms: 0 };
      S.subtasks.push(s);
      emit({ type: 'assign', from: disp.id, to: a.id, mission_id: m.id, title: s.title });
    });
    m.analysis = 'Phân công thử nghiệm (bản xem trước trong trình duyệt).';
    log('phan_cong', disp.name, `Đã phân công ${pick.length} đầu việc`, m.id, disp.id);
    setAgent(disp.id, 'idle', m.id);
    setMission(m, 'cho_duyet');
  }
  if (!exec) return;
  setMission(m, 'dang_thuc_hien');
  const steps = [...new Set(S.subtasks.filter((s) => s.mission_id === m.id).map((s) => s.step))].sort();
  for (const st of steps) {
    try { await Promise.all(S.subtasks.filter((s) => s.mission_id === m.id && s.step === st && s.status !== 'xong').map(async (s) => {
      await wait(3500);
      const prev = S.subtasks.find((x) => x.mission_id === m.id && x.step < st && x.agent_id !== s.agent_id);
      if (prev) emit({ type: 'exchange', from: s.agent_id, to: prev.agent_id, mission_id: m.id, title: s.title });
      s.status = 'dang_lam'; s.started_at = now();
      emit({ type: 'subtask', id: s.id, mission_id: m.id, status: 'dang_lam' });
      setAgent(s.agent_id, 'working', m.id, s.title);
      const say = (line) => { const l = new Date().toTimeString().slice(0, 8) + ' ' + line; (LIVE[s.id] ||= []).push(l); emit({ type: 'progress', subtask_id: s.id, agent_id: s.agent_id, mission_id: m.id, line: l }); };
      say('▶️ Chạy ' + (s.provider_id || 'AI của nhân viên'));
      for (const step of ['🔧 Đọc tệp: yeu-cau.md', '🔧 Tìm kiếm web: thị trường 2026', '✍️ Đang viết phần mở đầu…', '✍️ Đang lập bảng số liệu…']) { await wait(900 + Math.random() * 700); say(step); }
      if ((s.instruction + m.description).includes('(cần hỏi)') && !s.instruction.includes('[Quản lý trả lời')) {
        s.status = 'hoi'; s.question = 'Báo cáo dành cho ai đọc và dài khoảng bao nhiêu trang?'; s.log = LIVE[s.id].join('\n');
        emit({ type: 'subtask', id: s.id, mission_id: m.id, status: 'hoi' });
        setAgent(s.agent_id, 'asking', m.id, s.question);
        emit({ type: 'question', agent_id: s.agent_id, subtask_id: s.id, mission_id: m.id, question: s.question, title: s.title });
        m.status = 'cho_tra_loi'; emit({ type: 'mission', id: m.id, status: 'cho_tra_loi' });
        throw 'hoi';
      }
      await wait(1200);
      say('✅ Hoàn tất'); s.log = LIVE[s.id].join('\n'); delete LIVE[s.id];
      if (/xuất|file|tệp/i.test(s.instruction + s.title)) { s.files = [{ path: '/Users/demo/AI Office/ket-qua/Bao-cao.docx', name: 'Bao-cao.docx', size: 24576, kind: 'xuat' }, { path: '/Users/demo/AI Office/ket-qua/So-lieu.xlsx', name: 'So-lieu.xlsx', size: 9216, kind: 'xuat' }]; emit({ type: 'files', subtask_id: s.id, mission_id: m.id, files: s.files }); }
      s.status = 'xong'; s.output = `## Kết quả\n- Đã hoàn thành **${s.title}** (giả lập).\n- Đề xuất bước tiếp theo.`; s.duration_ms = 4200;
      const a = S.agents.find((x) => x.id === s.agent_id); if (a) a.tasks_done++;
      emit({ type: 'subtask', id: s.id, mission_id: m.id, status: 'xong' });
      setAgent(s.agent_id, 'done', m.id); setAgent(s.agent_id, 'idle', m.id);
      log('hoan_thanh', s.agent_name, `Hoàn thành: ${s.title}`, m.id, s.agent_id);
    })); } catch (e) { if (e === 'hoi') return; throw e; }
  }
  m.summary = `# Báo cáo tổng hợp: ${m.title}\n\nTất cả đầu việc đã hoàn thành (giả lập).`;
  setMission(m, 'hoan_thanh');
  emit({ type: 'mission', id: m.id, done: true });
}

const H = {
  app_info: () => ({ needs_setup: S.users.length === 0, version: 'demo', data_dir: '(trình duyệt)', office_name: S.settings.office_name, office_style: S.settings.office_style || 'hien_dai' }),
  set_office_style: ({ style }) => { S.settings.office_style = style; },
  path_info: ({ paths }) => paths.map((p) => ({ path: p, name: p.split('/').pop(), is_dir: !/\.[a-z0-9]+$/i.test(p), size: 2048 })),
  mission_folder: ({ id }) => '/demo/missions/nhiem-vu-' + id,
  open_path: () => null,
  setup_manager: ({ username, displayName: display_name, officeStyle }) => { if (officeStyle) S.settings.office_style = officeStyle; return (() => { const u = { id: 1, username, display_name: display_name || username, role: 'manager', active: true, created_at: now() }; S.users.push(u); return { token: 'demo', user: u }; })(); },
  login: ({ username }) => { const u = S.users.find((x) => x.username === username) || S.users[0]; if (!u) throw 'Sai tên đăng nhập hoặc mật khẩu'; return { token: 'demo', user: u }; },
  logout: () => null,
  me: () => { if (!S.users[0]) throw 'Phiên đăng nhập đã hết hạn'; return S.users[0]; },
  change_password: () => null,
  list_users: () => S.users,
  save_user: ({ user }) => { if (user.id) Object.assign(S.users.find((u) => u.id === user.id), user); else S.users.push({ ...user, id: id(), created_at: now() }); return 1; },
  delete_user: ({ id: uid }) => { S.users = S.users.filter((u) => u.id !== uid); },
  get_settings: () => S.settings,
  save_settings: (a) => { S.settings.office_name = a.officeName; S.settings.office_description = a.officeDescription; },
  list_providers: () => S.providers,
  save_provider: ({ provider }) => { const i = S.providers.findIndex((p) => p.id === provider.id); if (i >= 0) S.providers[i] = { ...S.providers[i], ...provider }; else S.providers.push({ ...provider, id: 'tuy-chinh-' + id() }); return provider.id; },
  delete_provider: ({ id: pid }) => { S.providers = S.providers.filter((p) => p.id !== pid); },
  test_provider: () => 'Bản xem trước: không kiểm tra được trong trình duyệt',
  detect_clis: () => ({ 'claude-cli': '/usr/local/bin/claude', 'codex-cli': null, 'gemini-cli': null }),
  refresh_path: () => null,
  list_agents: () => S.agents,
  list_templates: () => ({ templates: [], avatars: ['character-female-a', 'character-female-b', 'character-female-c', 'character-female-d', 'character-female-e', 'character-female-f', 'character-male-a', 'character-male-b', 'character-male-c', 'character-male-d', 'character-male-e', 'character-male-f', 'character-male-g', 'character-robot', 'character-worker', 'character-soldier', 'character-knight', 'character-alien'], levels: [{ key: 'co_ban', label: 'Cơ bản' }, { key: 'trung_cap', label: 'Trung cấp' }, { key: 'chuyen_nghiep', label: 'Chuyên nghiệp' }, { key: 'chuyen_gia', label: 'Chuyên gia' }] }),
  save_agent: ({ agent }) => { if (agent.id) Object.assign(S.agents.find((a) => a.id === agent.id), agent); else S.agents.push({ ...A(agent.name, agent.title, agent.department, agent.level, agent.skills, agent.avatar, agent.color), ...agent, id: id() }); emit({ type: 'agents_changed' }); return 1; },
  delete_agent: ({ id: aid }) => { S.agents = S.agents.filter((a) => a.id !== aid); emit({ type: 'agents_changed' }); },
  recruit: async ({ jobDescription, count }) => { await wait(1200); return { source: 'Giả lập', note: '', candidates: Array.from({ length: count }, (_, i) => ({ name: ['Ngô Bảo Long', 'Hoàng Thu Trang', 'Bùi Văn Sơn', 'Đặng Ngọc Yến'][i % 4], title: ['Chuyên viên soạn thảo', 'Nhà thiết kế UI/UX', 'Kỹ sư DevOps', 'Biên dịch viên'][i % 4], department: 'Văn phòng', level: ['trung_cap', 'chuyen_nghiep', 'chuyen_gia', 'co_ban'][i % 4], skills: ['Kỹ năng A', 'Kỹ năng B'], system_prompt: '...', avatar: ['character-male-g', 'character-female-f', 'character-worker', 'character-female-e'][i % 4], color: '#14b8a6', reason: 'Phù hợp mô tả: ' + jobDescription.slice(0, 40), provider_id: 'mock', model: '' })) }; },
  hire: ({ candidates }) => { candidates.forEach((c) => S.agents.push({ ...A(c.name, c.title, c.department, c.level, c.skills, c.avatar, c.color), provider_id: c.provider_id || 'mock', model: c.model || '' })); emit({ type: 'agents_changed' }); return []; },
  list_missions: () => S.missions.map((m) => { const subs = S.subtasks.filter((s) => s.mission_id === m.id); return { ...m, total: subs.length, done: subs.filter((s) => s.status === 'xong').length, agent_ids: subs.map((s) => s.agent_id), running: ['dang_lap_ke_hoach', 'dang_thuc_hien'].includes(m.status) }; }),
  get_mission: ({ id: mid }) => ({ mission: S.missions.find((m) => m.id === mid), subtasks: S.subtasks.filter((s) => s.mission_id === mid), running: false }),
  create_mission: ({ mission }) => { const m = { ...mission, id: id(), status: 'moi', analysis: '', summary: '', created_by: 1, created_at: now(), updated_at: now() }; S.missions.unshift(m); log('giao_viec', 'Quản lý', `Giao nhiệm vụ "${m.title}"`, m.id); if (m.mode !== 'manual') runMission(m, true, m.mode === 'auto'); return m.id; },
  update_mission: () => null,
  delete_mission: ({ id: mid }) => { S.missions = S.missions.filter((m) => m.id !== mid); },
  plan_mission: ({ id: mid }) => { runMission(S.missions.find((m) => m.id === mid), true, false); },
  run_mission: ({ id: mid }) => { const m = S.missions.find((x) => x.id === mid); runMission(m, !S.subtasks.some((s) => s.mission_id === mid), true); },
  cancel_mission: () => false,
  save_subtask: ({ subtask }) => { if (subtask.id) Object.assign(S.subtasks.find((s) => s.id === subtask.id), subtask, { status: 'cho' }); else S.subtasks.push({ ...subtask, id: id(), status: 'cho', output: '', error: '', agent_name: S.agents.find((a) => a.id === subtask.agentId)?.name || '' }); return 1; },
  delete_subtask: ({ id: sid }) => { S.subtasks = S.subtasks.filter((s) => s.id !== sid); },
  ask_agent: ({ agentId, message }) => { const m = { id: id(), title: message.slice(0, 60), description: message, priority: 'trung_binh', mode: 'manual', status: 'moi', analysis: '', summary: '', deadline: '', created_at: now(), updated_at: now() }; S.missions.unshift(m); S.subtasks.push({ id: id(), mission_id: m.id, agent_id: agentId, agent_name: S.agents.find((a) => a.id === agentId)?.name, step: 1, title: 'Việc giao trực tiếp', instruction: message, status: 'cho', output: '', error: '' }); runMission(m, false, true); return m.id; },
  list_activity: () => S.activity.slice(0, 100),
  dashboard: () => ({ agents: S.agents.length, working: S.agents.filter((a) => a.status === 'working' || a.status === 'thinking').length, missions: S.missions.length, missions_done: S.missions.filter((m) => m.status === 'hoan_thanh').length, missions_active: S.missions.filter((m) => ['dang_lap_ke_hoach', 'dang_thuc_hien'].includes(m.status)).length, missions_waiting: S.missions.filter((m) => ['moi', 'cho_duyet'].includes(m.status)).length, subtasks_done: S.subtasks.filter((s) => s.status === 'xong').length }),
};

// Giả lập quét máy / cài đặt CLI
const M = { installed: { 'claude-cli': '/Users/demo/.local/bin/claude' }, installing: [] };
const NPM = { 'codex-cli': '@openai/codex', 'gemini-cli': '@google/gemini-cli', 'qwen-cli': '@qwen-code/qwen-code', 'claude-cli': '@anthropic-ai/claude-code' };
Object.assign(H, {
  scan_machine: () => ({
    os: 'macos', installing: [...M.installing],
    tools: { node: 'v24.16.0', npm: '/usr/local/bin/npm', brew: '/opt/homebrew/bin/brew', curl: true },
    providers: S.providers.map((p) => {
      const base = { id: p.id, name: p.name, kind: p.kind, enabled: p.enabled, path: null, version: '', installable: false, can_login: false };
      if (p.kind === 'cli') return M.installed[p.id] ? { ...base, status: 'ready', path: M.installed[p.id], version: '1.0.0', can_login: true, detail: 'Đã cài trên máy' } : { ...base, status: 'missing', installable: !!NPM[p.id], detail: `Chưa có lệnh "${p.command}"` };
      if (p.kind === 'mock') return { ...base, status: 'ready', detail: 'Luôn sẵn sàng' };
      return p.has_key ? { ...base, status: 'ready', detail: 'Đã có API key' } : { ...base, status: 'need_key', detail: 'Chưa nhập API key' };
    }),
  }),
  install_cli: ({ id }) => {
    M.installing.push(id);
    const lines = [`$ npm install -g ${NPM[id] || id}`, 'npm http fetch GET 200 https://registry.npmjs.org/...', 'added 12 packages in 3s'];
    lines.forEach((line, i) => setTimeout(() => emit({ type: 'install', id, line }), 600 * (i + 1)));
    setTimeout(() => {
      M.installing = M.installing.filter((x) => x !== id);
      M.installed[id] = `/usr/local/bin/${id.replace('-cli', '')}`;
      emit({ type: 'install', id, done: true, ok: true, message: `Đã cài ${NPM[id]}`, path: M.installed[id] });
    }, 2600);
  },
  open_cli_login: () => null,
  cancel_install: () => true,
  set_onboarded: ({ done }) => { S.settings.onboarded = done; },
  optimize_preview: () => {
    const ready = S.providers.filter((p) => p.kind !== 'mock' && (M.installed[p.id] || p.has_key));
    if (!ready.length) return [];
    const lvModel = { co_ban: 'haiku', trung_cap: 'sonnet', chuyen_nghiep: 'sonnet', chuyen_gia: 'opus' };
    return S.agents.map((a, i) => {
      const p = ready[i % ready.length];
      const model = p.id === 'claude-cli' ? lvModel[a.is_dispatcher ? 'chuyen_gia' : a.level] : '';
      return { agent_id: a.id, name: a.name, title: a.title, level: a.level, is_dispatcher: a.is_dispatcher, current_provider: a.provider_id, current_model: a.model, provider_id: p.id, model, reason: `${p.name}${model ? ' · ' + model : ''} — phù hợp chuyên môn; cân đối chi phí.`, changed: a.provider_id !== p.id || a.model !== model };
    });
  },
  apply_optimization: ({ items }) => { items.forEach((it) => Object.assign(S.agents.find((a) => a.id === it.agent_id), { provider_id: it.provider_id, model: it.model })); emit({ type: 'agents_changed' }); return items.length; },
});
const _recruit = H.recruit;
H.recruit = async (a) => {
  const r = await _recruit(a);
  if (a.optimize) r.candidates.forEach((c, i) => { const ids = Object.keys(M.installed); c.provider_id = ids[i % ids.length]; c.model = c.provider_id === 'claude-cli' ? { co_ban: 'haiku', trung_cap: 'sonnet', chuyen_nghiep: 'sonnet', chuyen_gia: 'opus' }[c.level] : ''; c.ai_reason = `${c.provider_id}${c.model ? ' · ' + c.model : ''} — mạnh về chuyên môn của vị trí; cân đối chi phí.`; });
  return r;
};


// Quy trình nhiều AI & danh sách model (giả lập)
S.pipelines = [
  { id: 1, builtin: true, name: 'Nghiên cứu → Tổng hợp → Kiểm chứng', description: 'Gemini tra cứu thông tin, Claude viết tổng hợp, GPT (Codex) kiểm tra chéo lại thông tin.', stages: [
    { step: 1, title: 'Tra cứu thông tin', instruction: 'Tìm kiếm thông tin mới nhất, ghi nguồn.', agent_id: null, role: 'nghiên cứu', provider_id: 'gemini-cli', model: 'gemini-2.5-pro', effort: 'high', web: true },
    { step: 2, title: 'Viết báo cáo tổng hợp', instruction: 'Viết báo cáo tổng hợp.', agent_id: null, role: 'soạn thảo', provider_id: 'claude-cli', model: 'opus', effort: 'high', web: false },
    { step: 3, title: 'Kiểm tra lại thông tin', instruction: 'Kiểm tra chéo số liệu.', agent_id: null, role: 'kiểm thử', provider_id: 'codex-cli', model: '', effort: 'xhigh', web: false } ] },
  { id: 2, builtin: true, name: 'Dịch → Hiệu đính', description: 'Gemini dịch nhanh, Claude hiệu đính.', stages: [
    { step: 1, title: 'Dịch tài liệu', instruction: '', agent_id: null, role: 'dịch', provider_id: 'gemini-cli', model: 'gemini-3.5-flash', effort: 'medium', web: false },
    { step: 2, title: 'Hiệu đính', instruction: '', agent_id: null, role: 'soạn thảo', provider_id: 'claude-cli', model: 'sonnet', effort: 'high', web: false } ] },
];
Object.assign(H, {
  list_pipelines: () => S.pipelines,
  save_pipeline: ({ pipeline }) => { if (pipeline.id) Object.assign(S.pipelines.find((p) => p.id === pipeline.id), pipeline); else S.pipelines.push({ ...pipeline, id: id() }); return 1; },
  delete_pipeline: ({ id: pid }) => { S.pipelines = S.pipelines.filter((p) => p.id !== pid); },
  list_models: ({ providerId }) => ({ live: providerId === 'gemini-cli', default_model: '', models: { 'claude-cli': ['opus', 'sonnet', 'haiku', 'fable'], 'gemini-cli': ['gemini-2.5-pro', 'gemini-3.5-flash', 'gemini-3.1-flash-lite'], 'anthropic-api': ['claude-opus-5-5', 'claude-sonnet-5-5', 'claude-haiku-5-5'] }[providerId] || [] }),
});
const _create = H.create_mission;
H.create_mission = (a) => {
  const m = a.mission;
  if (m.mode !== 'pipeline') return _create(a);
  const mid = _create({ mission: { ...m, mode: 'manual' } });
  const pl = S.pipelines.find((p) => p.id === m.pipeline_id);
  const staff = S.agents.filter((x) => !x.is_dispatcher);
  pl.stages.forEach((stg, i) => { const ag = staff[i % staff.length]; S.subtasks.push({ id: id(), mission_id: mid, agent_id: ag.id, agent_name: ag.name, step: stg.step, title: stg.title, instruction: stg.instruction, status: 'cho', output: '', error: '', provider_id: stg.provider_id, model: stg.model, effort: stg.effort, web: stg.web }); });
  const mm = S.missions.find((x) => x.id === mid);
  mm.mode = 'pipeline';
  mm.analysis = 'Chạy theo quy trình **' + pl.name + '**';
  if (m.pipeline_run) runMission(mm, false, true); else mm.status = 'cho_duyet';
  return mid;
};

// Việc đang làm, hỏi đáp, nhắn tiếp (giả lập)
const LIVE = {};
Object.assign(H, {
  list_questions: () => S.subtasks.filter((s) => s.status === 'hoi').map((s) => ({ subtask_id: s.id, agent_id: s.agent_id, mission_id: s.mission_id, question: s.question, title: s.title })),
  answer_question: ({ subtaskId, answer }) => {
    const s = S.subtasks.find((x) => x.id === subtaskId);
    s.instruction += `\n\n[Quản lý trả lời câu hỏi “${s.question}”]: ${answer}`; s.question = ''; s.status = 'cho';
    setAgent(s.agent_id, 'idle', s.mission_id);
    emit({ type: 'answered', subtask_id: s.id, agent_id: s.agent_id, mission_id: s.mission_id });
    runMission(S.missions.find((m) => m.id === s.mission_id), false, true);
    return s.mission_id;
  },
  followup_mission: ({ id: mid, message, agentId }) => {
    const subs = S.subtasks.filter((s) => s.mission_id === mid);
    const step = Math.max(0, ...subs.map((s) => s.step)) + 1;
    const ag = S.agents.find((a) => a.id === agentId);
    S.subtasks.push({ id: id(), mission_id: mid, agent_id: agentId, agent_name: ag?.name, step, title: '↪ ' + message.slice(0, 60), instruction: message, status: 'cho', output: '', error: '', provider_id: '', model: '', effort: '', web: false, log: '', question: '' });
    emit({ type: 'assign', from: S.agents[0].id, to: agentId, mission_id: mid, title: 'Yêu cầu tiếp' });
    runMission(S.missions.find((m) => m.id === mid), false, true);
    return 1;
  },
  agent_work: ({ agentId }) => {
    const mine = S.subtasks.filter((s) => s.agent_id === agentId);
    const items = mine.filter((s) => ['dang_lam', 'hoi'].includes(s.status) || (s.status === 'cho' && ['dang_thuc_hien', 'cho_duyet', 'cho_tra_loi'].includes(S.missions.find((m) => m.id === s.mission_id)?.status)))
      .map((s) => ({ subtask: s, mission_title: S.missions.find((m) => m.id === s.mission_id)?.title, steps: S.subtasks.filter((x) => x.mission_id === s.mission_id).map((x) => ({ id: x.id, step: x.step, title: x.title, agent_name: x.agent_name, status: x.status })), live: LIVE[s.id] || [] }));
    const recent = mine.filter((s) => s.status === 'xong').slice(-5).reverse().map((s) => ({ id: s.id, title: s.title, finished_at: now(), mission_id: s.mission_id }));
    return { items, recent };
  },
});

Object.assign(H, {
  os_auth_info: () => ({ available: true, method: 'Touch ID hoặc mật khẩu đăng nhập máy tính' }),
  os_login: async ({ username }) => { await wait(800); const u = S.users.find((x) => x.username === username); if (!u) throw 'Không tìm thấy tài khoản này'; return { token: 'demo', user: u }; },
  os_reset_password: async ({ username }) => { await wait(800); const u = S.users.find((x) => x.username === username); if (!u) throw 'Không tìm thấy tài khoản này'; return { token: 'demo', user: u }; },
});

Object.assign(H, {
  export_formats: () => [['docx', 'Word (.docx)'], ['pdf', 'PDF (.pdf)'], ['xlsx', 'Excel (.xlsx)'], ['csv', 'Bảng CSV (.csv)'], ['html', 'Trang web (.html)'], ['md', 'Markdown (.md)'], ['txt', 'Văn bản (.txt)']].map(([ext, label]) => ({ ext, label })),
  export_content: ({ missionId, format }) => { const m = S.missions.find((x) => x.id === missionId); const f = { path: `/Users/demo/AI Office/nhiem-vu-${missionId}/ket-qua/${m.title}.${format}`, name: `${m.title}.${format}`, size: 18432, kind: 'xuat' }; (S.exports ||= []).push({ mid: missionId, ...f }); return f.path; },
  mission_files: ({ id: mid }) => ({ folder: `/Users/demo/AI Office/nhiem-vu-${mid}/ket-qua`, files: [...S.subtasks.filter((s) => s.mission_id === mid).flatMap((s) => (s.files || []).map((f) => ({ ...f, by: s.agent_name }))), ...(S.exports || []).filter((e) => e.mid === mid)] }),
  create_workdir: ({ name, parent }) => `${parent || '/Users/demo/Documents/AI Office'}/${name}`,
});

// Trí nhớ, mẫu tài liệu, thông báo hoàn thành (giả lập)
S.memories = [{ id: 1, agent_id: null, content: 'Đơn vị tiền tệ dùng triệu đồng', source: 'quan_ly', mission_id: null, active: true, created_at: now() }];
S.tpls = [{ id: 1, name: 'Báo cáo tuần', description: 'Báo cáo kết quả công tác hằng tuần', keywords: 'báo cáo tuần, tuần qua', content: '# BÁO CÁO CÔNG TÁC TUẦN\n\n## 1. Kết quả\n\n| Việc | Kết quả |\n|---|---|\n|  |  |\n\n## 2. Kế hoạch tuần tới', format: 'docx', auto: true, created_at: now() }];
Object.assign(H, {
  list_memories: ({ agentId }) => S.memories.filter((m) => m.agent_id == null || m.agent_id === agentId).slice().reverse(),
  save_memory: ({ memory }) => { if (memory.id) Object.assign(S.memories.find((m) => m.id === memory.id), memory); else S.memories.push({ ...memory, id: id(), source: 'quan_ly', created_at: now() }); },
  delete_memory: ({ id: mid }) => { S.memories = S.memories.filter((m) => m.id !== mid); },
  list_doc_templates: () => S.tpls,
  save_doc_template: ({ template }) => { if (template.id) Object.assign(S.tpls.find((t) => t.id === template.id), template); else S.tpls.push({ ...template, id: id() }); return 1; },
  delete_doc_template: ({ id: tid }) => { S.tpls = S.tpls.filter((t) => t.id !== tid); },
  import_template_file: ({ path }) => ({ name: path.split('/').pop().replace(/\.[^.]+$/, ''), content: '# Mẫu nhập từ tệp\n\n## 1. Nội dung' }),
});
const _rm = runMission;
runMission = async (m, plan, exec) => {
  await _rm(m, plan, exec);
  if (exec && m.status === 'hoan_thanh') {
    const subs = S.subtasks.filter((s) => s.mission_id === m.id);
    const ag = [...new Set(subs.map((s) => s.agent_id))];
    const presenter = ag.length > 1 ? S.agents.find((a) => a.is_dispatcher).id : subs.at(-1)?.agent_id;
    emit({ type: 'mission_done', mission_id: m.id, title: m.title, presenter, excerpt: m.summary, files: subs.reduce((n, s) => n + (s.files?.length || 0), 0), exported: null, template: null, steps: subs.length });
    if (subs.length === 1) { const a = S.agents.find((x) => x.id === subs[0].agent_id); const mem = { id: id(), agent_id: a.id, content: 'Quản lý thích báo cáo ngắn gọn, có bảng số liệu', source: 'ai', mission_id: m.id, active: true, created_at: now() }; S.memories.push(mem); emit({ type: 'memory', agent_id: a.id, content: mem.content, office: false }); }
  }
};

export async function demoCall(cmd, args) {
  const f = H[cmd];
  if (!f) throw `Bản xem trước chưa hỗ trợ: ${cmd}`;
  await wait(60);
  return f(args);
}
