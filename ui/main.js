const { invoke } = window.__TAURI__.core;

// Windows 使用系统原生标题栏，去掉为 macOS 红绿灯预留的左侧内边距
if (/Windows/i.test(navigator.userAgent)) {
  document.body.classList.add("is-windows");
}

// 自动刷新间隔按场景各自配置（E-02）：后端清单下发前先用默认值兜底
let defaultInterval = 60;
let minInterval = 5;
let maxInterval = 3600;
let refreshInterval = defaultInterval;
let autoTimer = null;

// 悬浮窗圆环
const FR = 20;
const FCIRC = 2 * Math.PI * FR;

const el = {
  cards: document.getElementById("cards"),
  badge: document.getElementById("plan-badge"),
  statusDot: document.getElementById("status-dot"),
  statusText: document.getElementById("status-text"),
  countdown: document.getElementById("countdown"),
  connectSm: document.getElementById("connect-sm"),
  error: document.getElementById("error-banner"),
  refreshBtn: document.getElementById("refresh-btn"),
  refreshText: document.getElementById("refresh-text"),
  subtitle: document.getElementById("subtitle"),
  pinBtn: document.getElementById("pin-btn"),
  tabs: document.getElementById("scenario-tabs"),

  floatRing: document.getElementById("float-ring"),
  floatCard: document.getElementById("float-card"),
  floatLabel: document.getElementById("float-label"),
  floatClose: document.getElementById("float-close"),
  floatRemain: document.getElementById("float-remain"),
  floatUnit: document.getElementById("float-unit"),
  floatSub: document.getElementById("float-sub"),
  floatReset: document.getElementById("float-reset"),
  floatRingFg: document.getElementById("float-ring-fg"),
  floatRingPct: document.getElementById("float-ring-pct"),

  settingsBtn: document.getElementById("settings-btn"),
  settingsModal: document.getElementById("settings-modal"),
  settingsBackdrop: document.getElementById("settings-backdrop"),
  settingsTitle: document.getElementById("settings-title"),
  settingsClose: document.getElementById("settings-close"),
  settingsMsg: document.getElementById("settings-msg"),
  storageDesc: document.getElementById("storage-desc"),
  listView: document.getElementById("settings-list-view"),
  detailView: document.getElementById("settings-detail-view"),
  sourceList: document.getElementById("source-list"),
  detailDesc: document.getElementById("detail-desc"),
  detailFields: document.getElementById("detail-fields"),
  detailInterval: document.getElementById("detail-interval"),
  detailBack: document.getElementById("detail-back"),
  detailClear: document.getElementById("detail-clear"),
  detailTest: document.getElementById("detail-test"),
  detailSave: document.getElementById("detail-save"),
};

let loading = false;
/// 当前场景交给悬浮窗的文案；为 null 表示该场景没有可展示的悬浮信息
let lastFloat = null;

/// 后端下发的数据源清单，以及当前正在编辑 / 正在展示的平台
let catalog = [];
let activeSource = null;
/// 凭证存放方式（后端下发）：keychain = 系统钥匙串，file = 明文文件兜底
let credStorage = "keychain";
let credPath = "";
let currentSource = null;

/// 会话解锁闸门状态（后端下发，不持久化）：未解锁前钥匙串源不取数、不弹授权框
let credsUnlocked = false;
/// 当前展示的场景是否为「锁定态」的钥匙串源（决定悬浮窗是否显示待授权入口）
let lockedScene = false;

/* ---------- 通用 ---------- */

function setStatus(kind, text) {
  el.statusDot.className = "status-dot" + (kind ? " " + kind : "");
  el.statusText.textContent = text;
}

function showError(msg) {
  el.error.textContent = msg;
  el.error.classList.remove("hidden");
}

function hideError() {
  el.error.classList.add("hidden");
}

/* ---------- 悬浮窗 ---------- */

function updateFloat() {
  const fg = el.floatRingFg;
  if (!fg) return;

  // 锁定态钥匙串源：悬浮窗显示「待授权」，并把副标题做成可点击的解锁入口
  if (lockedScene) {
    el.floatRing.classList.add("hidden");
    el.floatLabel.textContent = "连接账号";
    el.floatReset.textContent = "";
    el.floatRemain.textContent = "待授权";
    el.floatRemain.style.color = "";
    el.floatUnit.textContent = "";
    el.floatSub.textContent = "点击此处解锁";
    el.floatSub.classList.add("float-action");
    return;
  }
  el.floatSub.classList.remove("float-action");

  const f = lastFloat;
  if (!f) {
    el.floatRing.classList.add("hidden");
    el.floatLabel.textContent = "—";
    el.floatReset.textContent = "—";
    el.floatRemain.textContent = "—";
    el.floatRemain.style.color = "";
    el.floatUnit.textContent = "";
    el.floatSub.textContent = "暂无数据";
    return;
  }

  el.floatLabel.textContent = f.label || "";
  el.floatReset.textContent = f.reset || "";
  el.floatRemain.textContent = f.remain ?? "—";
  el.floatRemain.style.color = f.color || "";
  el.floatUnit.textContent = f.unit || "";
  el.floatSub.textContent = f.sub || "";

  // 余额类场景没有「已用比例」这个概念，就不画圆环，把空间让给数值
  const pct = typeof f.pct === "number" ? Math.max(0, Math.min(100, f.pct)) : null;
  if (pct === null) {
    el.floatRing.classList.add("hidden");
    return;
  }
  el.floatRing.classList.remove("hidden");
  fg.setAttribute("stroke", f.color || "#22d3ee");
  fg.setAttribute("stroke-dasharray", FCIRC.toFixed(2));
  fg.setAttribute("stroke-dashoffset", (FCIRC * (1 - pct / 100)).toFixed(2));
  // 环里补上百分比，口径与展开视图一致（已用占比）；取整避免 100% 撑出环内
  el.floatRingPct.textContent = `${Math.round(pct)}%`;
  el.floatRingPct.style.color = f.color || "#22d3ee";
}

/* ---------- 内容区 ---------- */

/// 数据源的展示交给它自己的渲染模块：ui/sources/<source>.js
function renderResult(res) {
  lockedScene = false;
  const mod = (window.AFP_SOURCES || {})[res?.source];
  let view = null;
  try {
    view = mod ? mod.render(res?.data) : null;
  } catch (e) {
    view = null;
    console.error(e);
  }

  if (!view) {
    el.cards.innerHTML = `<div class="empty">这个数据源还没有对应的展示</div>`;
    el.badge.textContent = "—";
    lastFloat = null;
    updateFloat();
    return;
  }

  el.cards.innerHTML = view.html || `<div class="empty">暂无数据</div>`;
  el.badge.textContent = view.badge || "—";
  lastFloat = view.float || null;
  updateFloat();
}

/// 锁定态主视图：显示「连接账号」按钮 + 说明，点击后才请求钥匙串授权并加载数据
function renderLocked() {
  lockedScene = true;
  el.badge.textContent = "待授权";
  el.cards.innerHTML = `
    <div class="connect-card">
      <div class="connect-title">连接账号</div>
      <p class="connect-desc">点击后 macOS 会请求钥匙串授权，随后读取各平台已保存的密钥并加载数据。授权仅在本次运行内有效，下次启动仍需重新连接。</p>
      <button type="button" class="primary-btn connect-btn" id="connect-btn">连接账号</button>
    </div>`;
  el.cards.querySelector("#connect-btn").addEventListener("click", connectAccount);
  lastFloat = null;
  updateFloat();
}

/// 解锁流程：请求钥匙串授权 → 读取各平台密钥 → 重新加载清单与数据 → 启动刷新
async function connectAccount() {
  const btn = document.getElementById("connect-btn");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "连接中…";
  }
  try {
    const r = await invoke("unlock_credentials");
    credsUnlocked = r?.unlocked === true;
    await reloadCatalog();
    await refresh(true);
  } catch (e) {
    showError(String(e));
    if (btn) {
      btn.disabled = false;
      btn.textContent = "连接账号";
    }
  }
}

async function refresh(manual = false) {
  if (loading) return;
  loading = true;
  el.refreshBtn.classList.add("spinning");
  el.refreshText.textContent = "刷新中";
  if (manual) hideError();
  // 定住态看不到主窗口状态行，把「刷新中」的反馈落在悬浮窗副标题上
  const floatBusy = manual && document.body.classList.contains("pinned");
  if (floatBusy) el.floatSub.textContent = "刷新中…";

  try {
    const res = await invoke("get_source_data");
    el.subtitle.textContent = res?.subtitle || res?.name || "—";
    if (res?.state === "locked") {
      // 待授权：不是错误，主视图给「连接账号」入口
      setStatus("", "待授权");
      renderLocked();
    } else {
      renderResult(res);
      hideError();
      const now = new Date().toLocaleTimeString("zh-CN", { hour12: false });
      setStatus("live", `更新于 ${now}`);
    }
  } catch (e) {
    lockedScene = false;
    setStatus("err", "获取失败");
    showError(String(e));
    el.cards.innerHTML = `<div class="empty">暂无数据</div>`;
    lastFloat = null;
    updateFloat();
  } finally {
    loading = false;
    el.refreshBtn.classList.remove("spinning");
    el.refreshText.textContent = "刷新";
    if (floatBusy) updateFloat();
    // 取数完成即重置倒计时与自动刷新定时器（场景或间隔可能刚被改写）
    applyRefreshInterval();
  }
}

function updateCountdown() {
  el.countdown.textContent = `${secondsLeft}s 后自动刷新`;
}

el.refreshBtn.addEventListener("click", () => refresh(true));
el.connectSm.addEventListener("click", () => connectAccount());

/* ---------- 场景 Tab（M-05）+ 每场景刷新间隔（E-02） ---------- */

/// 只给「已配置」的平台建 Tab：未配置的平台在设置里录入后才出现
function renderTabs() {
  const configured = catalog.filter((s) => s.configured);
  el.tabs.innerHTML = "";

  // 只有一个已配置平台时 Tab 没有切换意义，收起把纵向空间还给内容
  if (configured.length < 2) {
    el.tabs.classList.add("hidden");
    return;
  }
  el.tabs.classList.remove("hidden");

  for (const s of configured) {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = "tab" + (s.id === activeSource ? " active" : "");
    tab.textContent = s.name;
    tab.addEventListener("click", () => switchSource(s.id));
    el.tabs.appendChild(tab);
  }
}

/// 取当前场景配置的刷新间隔，并重建自动刷新定时器。
/// 锁定态下不启动定时器，倒计时位置显示「待授权」（no-op，不刷出错误）。
function applyRefreshInterval() {
  if (autoTimer) clearInterval(autoTimer);
  autoTimer = null;

  if (!credsUnlocked) {
    secondsLeft = refreshInterval;
    el.countdown.textContent = "待授权";
    el.connectSm.classList.remove("hidden");
    return;
  }

  el.connectSm.classList.add("hidden");

  const cur = catalog.find((s) => s.id === activeSource);
  refreshInterval = cur?.interval || defaultInterval;
  secondsLeft = refreshInterval;
  updateCountdown();

  autoTimer = setInterval(() => refresh(false), refreshInterval * 1000);
}

async function switchSource(id) {
  if (id === activeSource) return;
  activeSource = id;
  renderTabs();
  applyRefreshInterval();
  try {
    await invoke("set_active_source", { sourceId: id });
  } catch (e) {
    showError(String(e));
  }
  await refresh(true);
}

/* ---------- 设置：数据源列表 + 按平台动态表单 ---------- */

function setSettingsMsg(kind, text) {
  const box = el.settingsMsg;
  if (!text) {
    box.className = "settings-msg hidden";
    box.textContent = "";
    return;
  }
  box.className = "settings-msg" + (kind ? " " + kind : "");
  box.textContent = text;
}

function settingsOpen() {
  return !el.settingsModal.classList.contains("hidden");
}

function showView(name) {
  el.listView.classList.toggle("hidden", name !== "list");
  el.detailView.classList.toggle("hidden", name !== "detail");
  el.settingsTitle.textContent = name === "detail" ? currentSource?.name || "数据源" : "数据源";
}

/// 凭证放在哪儿，由后端下发的 storage 决定（E-01）
function renderStorageDesc() {
  el.storageDesc.textContent =
    credStorage === "keychain"
      ? "凭证保存在系统钥匙串（macOS Keychain / Windows 凭据管理器），不落到明文文件。点击平台录入它自己的密钥，保存后即切换为该场景。"
      : `凭证以明文保存在本机（${credPath}），因为当前系统钥匙串不可用。点击平台录入它自己的密钥，保存后即切换为该场景。`;
}

function renderSourceList() {
  el.sourceList.innerHTML = "";
  if (!catalog.length) {
    el.sourceList.innerHTML = `<div class="empty">暂时没有可用的数据源</div>`;
    return;
  }

  for (const s of catalog) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "source-row" + (s.id === activeSource ? " current" : "");

    const main = document.createElement("span");
    main.className = "source-row-main";
    const name = document.createElement("span");
    name.className = "source-row-name";
    name.textContent = s.name;
    const sub = document.createElement("span");
    sub.className = "source-row-sub";
    sub.textContent = s.subtitle;
    main.append(name, sub);

    const state = document.createElement("span");
    let stateText;
    let ok = false;
    if (s.autoCreds) {
      stateText = s.configured ? (s.id === activeSource ? "当前 · 可用" : "可用") : "不可用";
      ok = !!s.configured;
    } else if (s.ready) {
      stateText = s.id === activeSource ? "当前 · 已配置" : "已配置";
      ok = true;
    } else if (s.hasCreds) {
      // 已保存但未解锁：不能显示成「未配置」，否则会诱导用户重录
      stateText = "已保存 · 待解锁";
    } else {
      stateText = "未配置";
    }
    state.className =
      "source-row-state" + (ok ? " ok" : s.hasCreds && !credsUnlocked ? " warn" : "");
    state.textContent = stateText;

    row.append(main, state);
    row.addEventListener("click", () => openDetail(s.id));
    el.sourceList.appendChild(row);
  }
}

async function reloadCatalog() {
  const res = await invoke("list_sources");
  catalog = res?.sources || [];
  activeSource = res?.active || null;
  credsUnlocked = res?.unlocked === true;
  credStorage = res?.storage === "file" ? "file" : "keychain";
  credPath = res?.path || "";
  renderStorageDesc();
  if (res?.defaultInterval) defaultInterval = res.defaultInterval;
  if (res?.minInterval) minInterval = res.minInterval;
  if (res?.maxInterval) maxInterval = res.maxInterval;
  renderSourceList();
  renderTabs();
  applyRefreshInterval();
}

async function openSettings() {
  setSettingsMsg("", "");
  try {
    await reloadCatalog();
  } catch (e) {
    catalog = [];
    activeSource = null;
    renderSourceList();
    el.sourceList.innerHTML = `<div class="empty">读取数据源失败</div>`;
    console.error(e);
  }
  showView("list");
  el.settingsModal.classList.remove("hidden");
}

function closeSettings() {
  el.settingsModal.classList.add("hidden");
  setSettingsMsg("", "");
  currentSource = null;
}

/// 首个凭证字段标题后面的指引链接：点它去对应的密钥管理页
function helpLink(def) {
  const a = document.createElement("a");
  a.className = "field-help";
  a.href = "#";
  a.title = def.helpText || "";
  a.textContent = `${def.helpText} →`;
  a.addEventListener("click", (e) => {
    e.preventDefault();
    if (def.helpUrl) invoke("open_url", { url: def.helpUrl }).catch(() => {});
  });
  return a;
}

/// 按当前平台的字段 schema 生成表单（含密文字段的显示 / 隐藏切换）
function openDetail(id) {
  const def = catalog.find((s) => s.id === id);
  if (!def) return;
  currentSource = def;
  setSettingsMsg("", "");

  const isAuto = def.autoCreds === true;
  // 已保存凭证但当前锁定：不能在这一态读取钥匙串，也不能诱导用户重录覆盖旧密钥
  const lockedHasCreds = !isAuto && !credsUnlocked && def.hasCreds === true;
  el.detailFields.innerHTML = "";

  if (isAuto) {
    // 无用户可填字段：只展示后端现场检测到的登录态信息
    const info = def.autoInfo || {};
    el.detailDesc.textContent = `${def.subtitle} · 自动读取本机 Trae CN 登录态，无需填写凭证。`;
    const rows = [
      ["登录态文件", info.file || "—"],
      ["token 过期时间", info.expiredAt || "—"],
      ["套餐身份", info.identity || "—"],
      ["检测结果", info.available ? "正常" : `不可用：${info.unavailable || "未知原因"}`],
    ];
    for (const [label, value] of rows) {
      const wrap = document.createElement("div");
      wrap.className = "field";
      const lb = document.createElement("span");
      lb.className = "field-label";
      lb.textContent = label;
      const val = document.createElement("div");
      val.className = "ro";
      val.textContent = value;
      wrap.append(lb, val);
      el.detailFields.appendChild(wrap);
    }
  } else {
    if (lockedHasCreds) {
      el.detailDesc.textContent = `${def.subtitle} · 该平台已保存凭证，但当前处于锁定态。请先返回主界面点击「连接账号」解锁后再查看或修改。`;
    } else {
      el.detailDesc.textContent = `${def.subtitle} · 凭证只存本机，用于调用 ${def.name} 的接口。`;
    }
    for (const [i, f] of def.fields.entries()) {
      const wrap = document.createElement("div");
      wrap.className = "field";

      // 标题行：字段名 + （仅首个字段）指引链接，两者同一行不额外占纵向空间
      const head = document.createElement("span");
      head.className = "field-head";
      const label = document.createElement("span");
      label.className = "field-label";
      label.textContent = f.label;
      head.appendChild(label);
      if (i === 0 && def.helpText) head.appendChild(helpLink(def));
      wrap.appendChild(head);

      const input = document.createElement("input");
      input.type = f.secret ? "password" : "text";
      input.value = f.value || "";
      input.placeholder = f.placeholder || "";
      input.spellcheck = false;
      input.autocomplete = "off";
      input.dataset.key = f.key;

      if (f.secret) {
        const row = document.createElement("span");
        row.className = "field-row";
        const toggle = document.createElement("button");
        toggle.type = "button";
        toggle.className = "ghost-btn";
        toggle.textContent = "显示";
        toggle.addEventListener("click", () => {
          const isPw = input.type === "password";
          input.type = isPw ? "text" : "password";
          toggle.textContent = isPw ? "隐藏" : "显示";
        });
        row.append(input, toggle);
        wrap.appendChild(row);
      } else {
        wrap.appendChild(input);
      }

      el.detailFields.appendChild(wrap);
    }
  }

  el.detailInterval.value = String(def.interval || defaultInterval);
  el.detailInterval.min = String(minInterval);
  el.detailInterval.max = String(maxInterval);

  // 自动凭证的源没有表单：隐藏保存 / 清除，把「测试连接」改成「重新检测」
  // 锁定态且已保存凭证时：隐藏保存 / 测试，避免用空表单覆盖钥匙串里的旧密钥
  el.detailSave.classList.toggle("hidden", isAuto || lockedHasCreds);
  el.detailClear.classList.toggle("hidden", isAuto || !def.configured);
  el.detailTest.classList.toggle("hidden", lockedHasCreds);
  el.detailTest.textContent = isAuto ? "重新检测" : "测试连接";

  showView("detail");
  el.detailFields.querySelector("input")?.focus();
}

/// 读表单里的刷新间隔；空值 / 非法值回落到默认间隔（越界由后端夹紧）
function readIntervalValue() {
  const raw = Number(el.detailInterval.value);
  if (!Number.isFinite(raw) || raw <= 0) return defaultInterval;
  return Math.round(raw);
}

function readDetailValues() {
  const values = {};
  el.detailFields.querySelectorAll("input").forEach((i) => {
    values[i.dataset.key] = i.value.trim();
  });
  return values;
}

async function saveDetail() {
  if (!currentSource) return;
  const id = currentSource.id;
  el.detailSave.disabled = true;
  el.detailSave.textContent = "保存中…";
  try {
    await invoke("save_source_credentials", {
      sourceId: id,
      values: readDetailValues(),
    });
    await invoke("set_refresh_interval", {
      sourceId: id,
      seconds: readIntervalValue(),
    });
    // 保存后后端会把该平台设为当前场景，重新拉清单让 Tab / 间隔都跟上
    await reloadCatalog();
    await refresh(true);
    closeSettings();
  } catch (e) {
    setSettingsMsg("err", String(e));
  } finally {
    el.detailSave.disabled = false;
    el.detailSave.textContent = "保存并切换";
  }
}

async function testDetail() {
  if (!currentSource) return;

  // 自动凭证的源：重新检测 = 重新拉清单（后端现场重读登录态文件）
  if (currentSource.autoCreds) {
    el.detailTest.disabled = true;
    el.detailTest.textContent = "检测中…";
    try {
      await reloadCatalog();
      openDetail(currentSource.id);
      setSettingsMsg("ok", "已重新检测本机登录态");
    } catch (e) {
      setSettingsMsg("err", String(e));
    } finally {
      el.detailTest.disabled = false;
      el.detailTest.textContent = "重新检测";
    }
    return;
  }

  el.detailTest.disabled = true;
  el.detailTest.textContent = "测试中…";
  try {
    const r = await invoke("test_source", {
      sourceId: currentSource.id,
      values: readDetailValues(),
    });
    let text = "连接成功";
    try {
      const view = (window.AFP_SOURCES || {})[currentSource.id]?.render(r?.data);
      if (view?.summary) text = `连接成功 · ${view.summary}`;
    } catch (_) {
      /* 展示解析失败不影响「已连通」这个结论 */
    }
    setSettingsMsg("ok", text);
  } catch (e) {
    setSettingsMsg("err", String(e));
  } finally {
    el.detailTest.disabled = false;
    el.detailTest.textContent = "测试连接";
  }
}

async function clearDetail() {
  if (!currentSource) return;
  const id = currentSource.id;
  try {
    await invoke("clear_source_credentials", { sourceId: id });
    await reloadCatalog();
    openDetail(id);
    setSettingsMsg("ok", "已清除本机的该平台凭证");
  } catch (e) {
    setSettingsMsg("err", String(e));
  }
}

el.settingsBtn.addEventListener("click", openSettings);
el.settingsClose.addEventListener("click", closeSettings);
el.settingsBackdrop.addEventListener("click", closeSettings);
el.detailBack.addEventListener("click", () => {
  setSettingsMsg("", "");
  showView("list");
});
el.detailSave.addEventListener("click", saveDetail);
el.detailTest.addEventListener("click", testDetail);
el.detailClear.addEventListener("click", clearDetail);

document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !settingsOpen()) return;
  // 详情页先退回列表，再按一次才关闭弹窗
  if (!el.detailView.classList.contains("hidden")) {
    setSettingsMsg("", "");
    showView("list");
    return;
  }
  closeSettings();
});

/* ---------- 定住 / 取消定住 ---------- */

// 窗口会播放约 180ms 的缩放动画，期间忽略重复点击，避免状态错乱
let pinBusy = false;

async function setPinned(on) {
  if (pinBusy) return;
  pinBusy = true;
  setTimeout(() => {
    pinBusy = false;
  }, 500);
  try {
    if (on) closeSettings();
    await invoke("set_pinned", { on });
  } catch (e) {
    showError(String(e));
  }
}

// 由 Rust 在动画开始前发出，让内容布局与窗口缩放同步切换
window.__TAURI__.event.listen("pin-state", (e) => {
  const on = !!e.payload;
  document.body.classList.toggle("pinned", on);
  if (on) updateFloat();
});

el.pinBtn.addEventListener("click", () => setPinned(true));
el.floatClose.addEventListener("click", () => setPinned(false));

// 悬浮窗整卡可点：点一下立刻重取数据（锁定态且需钥匙串则先走解锁）。
// 卡片同时是拖拽区，拖动时浏览器可能补一个 click，所以按下位置位移超过 6px 就当成拖拽，不刷新。
let floatDown = null;
el.floatCard.addEventListener("pointerdown", (e) => {
  floatDown = { x: e.clientX, y: e.clientY };
});
el.floatCard.addEventListener("click", (e) => {
  if (e.target.closest(".float-close")) return;
  if (floatDown && Math.hypot(e.clientX - floatDown.x, e.clientY - floatDown.y) > 6) {
    floatDown = null;
    return;
  }
  floatDown = null;
  if (lockedScene) connectAccount();
  else refresh(true);
});

// 倒计时每秒走一格；自动刷新定时器由 applyRefreshInterval 按当前场景的间隔维护
setInterval(() => {
  // 锁定态：倒计时冻结（显示「待授权」），不做任何刷新
  if (!credsUnlocked) return;
  secondsLeft -= 1;
  if (secondsLeft <= 0) {
    secondsLeft = refreshInterval;
  }
  updateCountdown();
}, 1000);

async function bootstrap() {
  await refresh(false);
  try {
    await reloadCatalog();
    const cur = catalog.find((s) => s.id === activeSource);
    // 锁定态下钥匙串源「已保存与否」暂不可判定，不弹设置（主界面已给「连接账号」入口）；
    // 其余情况（trae 不可用 / 解锁后确实未配置）仍按原逻辑引导去设置页录入。
    const keychainLocked = !credsUnlocked && !cur?.autoCreds;
    if (!cur?.configured && !keychainLocked) openSettings();
  } catch (_) {
    /* 忽略：清单读取失败不影响主流程 */
  }
}

bootstrap();
