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

/// 当前展示的场景是否为「未解锁」的钥匙串源（决定悬浮窗是否显示待授权入口）
let lockedScene = false;

/// 某数据源本次运行是否可立即取数：auto 源看「本机登录态可用」，钥匙串源看「本次是否已连接」
function sourceReady(id) {
  const s = catalog.find((x) => x.id === id);
  return !!s && s.ready === true;
}

/// 当前场景是否可立即取数（未连接时既不取数、也不启动自动刷新）
function activeReady() {
  return !!activeSource && sourceReady(activeSource);
}

/* ---------- 通用 ---------- */

function setStatus(kind, text) {
  el.statusDot.className = "status-dot" + (kind ? " " + kind : "");
  el.statusText.textContent = text;
}

function showError(msg) {
  el.error.textContent = msg;
  el.error.classList.remove("hidden");
  fitWindowHeight();
}

function hideError() {
  el.error.classList.add("hidden");
  fitWindowHeight();
}

/* ---------- 悬浮窗 ---------- */

function updateFloat() {
  renderFloat();
  // 悬浮卡片随内容收缩宽度，短内容（如只有几位积分的 Trae）就不会在右侧空出一大块
  fitFloatWidth();
}

function renderFloat() {
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

/// 上次应用到窗口的悬浮卡宽度（逻辑像素），用来抑制数字位数变化带来的反复抖动
let floatWidth = 0;

/// 量出卡片「贴合内容」时的宽度：临时把卡片设为脱离文档流 + max-content，
/// 读完立刻还原样式——同一个任务里量完就还原，浏览器来不及重绘，用户看不到这一下。
function measureFloatWidth() {
  const card = el.floatCard;
  if (!card) return 0;
  const prev = card.getAttribute("style");
  card.style.position = "absolute";
  card.style.width = "max-content";
  const w = card.offsetWidth;
  if (prev === null) card.removeAttribute("style");
  else card.setAttribute("style", prev);
  return w;
}

/// 定住态下让窗口宽度贴合内容；只有宽度变化超过 6px 才真正改窗口，
/// 避免刷新时数字从 36,212.47 变成 9,999.99 就抖一下。
async function fitFloatWidth(force = false) {
  if (!document.body.classList.contains("pinned")) return;
  const w = measureFloatWidth();
  if (!w) return;
  if (!force && Math.abs(w - floatWidth) < 6) return;
  floatWidth = w;
  try {
    await invoke("set_float_width", { width: w });
  } catch (_) {
    /* 改不动尺寸不影响内容展示，保持原宽度即可 */
  }
}

/* ---------- 窗口高度贴合内容 ---------- */

/// 量出主视图内容的「贴合高度」（逻辑像素）：顶栏 + Tab 条 + 内容区实际内容。
/// 内容区取最后一个可见子元素的位置 + 高度再加下内边距，元素间的外边距
/// 天然包含在 offsetTop 里，比逐项累加更稳。
function measureContentHeight() {
  const topbar = document.querySelector(".topbar");
  const tabsH = el.tabs.classList.contains("hidden") ? 0 : el.tabs.offsetHeight;
  const content = document.querySelector(".content");
  const kids = [...content.children].filter((c) => !c.classList.contains("hidden"));
  const last = kids[kids.length - 1];
  if (!last) return topbar.offsetHeight + tabsH + content.offsetHeight;
  const padBottom = parseFloat(getComputedStyle(content).paddingBottom) || 0;
  return topbar.offsetHeight + tabsH + last.offsetTop + last.offsetHeight + padBottom;
}

/// 上次应用到窗口的高度（逻辑像素），用来抑制数字位数 / 换行变化带来的反复抖动
let lastFitH = 0;
/// 启动后是否已贴合过：首次直接落位不播动画，避免窗口刚显示就「缩一下」
let bootFitted = false;

/// 把窗口高度调到当前内容贴合的高度（切换 tab / 内容变化时调用），带过渡动画。
/// 定住态与设置弹窗打开期间不贴合；floor 用于抬高下限（设置弹窗需要更多空间）。
async function fitWindowHeight(force = false, floor = 0) {
  if (document.body.classList.contains("pinned")) return;
  if (settingsOpen()) return;
  // 定住 / 取消定住的动画期间窗口尺寸正在被动画接管，别去抢
  if (pinAnimating) return;
  let h = measureContentHeight();
  if (!h) return;
  if (floor) h = Math.max(h, floor);
  if (!force && Math.abs(h - lastFitH) < 6) return;
  lastFitH = h;
  const animate = bootFitted;
  bootFitted = true;
  try {
    await invoke("fit_window_height", { height: h, animate });
  } catch (_) {
    /* 改不动尺寸不影响内容展示，保持当前高度即可 */
  }
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
  const nm = catalog.find((s) => s.id === activeSource)?.name || "该平台";
  el.cards.innerHTML = `
    <div class="connect-card">
      <div class="connect-title">连接 ${nm}</div>
      <p class="connect-desc">「${nm}」的凭证存在系统钥匙串里。点击后 macOS 会请求钥匙串授权，一次性读取所有已保存平台的密钥并加载数据；此后切换其他 tab 不再重复请求。授权仅在本次运行内有效。</p>
      <button type="button" class="primary-btn connect-btn" id="connect-btn">连接账号</button>
    </div>`;
  el.cards
    .querySelector("#connect-btn")
    .addEventListener("click", () => connectSource(activeSource));
  lastFloat = null;
  updateFloat();
}

/// 按需连接某个数据源：请求钥匙串授权 → 读取该源密钥 → 重新加载清单与数据 → 启动刷新
async function connectSource(id) {
  const sid = id || activeSource;
  if (!sid) return;
  const btn = document.getElementById("connect-btn");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "连接中…";
  }
  try {
    await invoke("unlock_source", { sourceId: sid });
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
      lastUpdatedAt = now;
      setStatus("live", `更新于 ${now}`);
    }
  } catch (e) {
    lockedScene = false;
    setStatus("err", "获取失败");
    showError(String(e));
    // 刷新失败不清空已有数据：悬浮卡与主视图保留上一次成功的结果，
    // 错误交给顶部状态与错误横幅表达，避免网络抖一下悬浮窗就变成空「—」卡；
    // 提示窗里的「更新于」停留在上次成功时间，可据此判断数据新旧
  } finally {
    loading = false;
    el.refreshBtn.classList.remove("spinning");
    el.refreshText.textContent = "刷新";
    if (floatBusy) updateFloat();
    // 提示窗开着时取数完成：顺手把里面的数字刷成新的
    if (tipShown) showTip();
    // 取数完成即重置倒计时与自动刷新定时器（场景或间隔可能刚被改写）
    applyRefreshInterval();
    // 内容高度可能随场景 / 错误横幅变化：贴合一次（切换 tab 的动画过渡就走这里）
    fitWindowHeight();
  }
}

function updateCountdown() {
  el.countdown.textContent = `${secondsLeft}s 后自动刷新`;
}

el.refreshBtn.addEventListener("click", () => refresh(true));
el.connectSm.addEventListener("click", () => connectSource(activeSource));

/* ---------- 场景 Tab（M-05）+ 每场景刷新间隔（E-02） ---------- */

/// 只给「已配置」的平台建 Tab：未配置的平台在设置里录入后才出现
function renderTabs() {
  const configured = catalog.filter((s) => s.configured);
  el.tabs.innerHTML = "";

  // 只有一个已配置平台时 Tab 没有切换意义，收起把纵向空间还给内容
  if (configured.length < 2) {
    el.tabs.classList.add("hidden");
    fitWindowHeight();
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
  fitWindowHeight();
}

/// 取当前场景配置的刷新间隔，并重建自动刷新定时器。
/// 当前场景未连接（钥匙串源未解锁）时不启动定时器，倒计时位置显示「待连接」（no-op，不刷出错误）。
function applyRefreshInterval() {
  if (autoTimer) clearInterval(autoTimer);
  autoTimer = null;

  if (!activeReady()) {
    secondsLeft = refreshInterval;
    el.countdown.textContent = "待连接";
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
  // 切到需要凭证的 tab：此刻才读钥匙串（懒加载）。未连接才请求授权，已连接直接取数。
  const def = catalog.find((s) => s.id === id);
  if (def && !def.autoCreds && !def.ready) {
    await connectSource(id);
    return;
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
      // 已保存但未连接：不能显示成「未配置」，否则会诱导用户重录
      stateText = "已保存 · 待连接";
    } else {
      stateText = "未配置";
    }
    state.className =
      "source-row-state" + (ok ? " ok" : s.hasCreds && !s.unlocked ? " warn" : "");
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
  // 弹窗需要更多纵向空间：先把窗口抬到至少 460，关闭时再贴合回内容
  await fitWindowHeight(true, 460);
  el.settingsModal.classList.remove("hidden");
}

function closeSettings() {
  el.settingsModal.classList.add("hidden");
  setSettingsMsg("", "");
  currentSource = null;
  fitWindowHeight(true);
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
  // 已保存凭证但本次运行尚未连接：不能在这一态读取钥匙串，也不能诱导用户重录覆盖旧密钥
  const lockedHasCreds = !isAuto && !def.unlocked && def.hasCreds === true;
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
      el.detailDesc.textContent = `${def.subtitle} · 该平台已保存凭证，但本次运行尚未连接。请先切到「${def.name}」的 tab 点击「连接账号」读取后再查看或修改。`;
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

/// 是否正处于定住 / 取消定住的过渡动画期间。
/// 收缩动画里窗口会从鼠标底下扫过，若不抑制，mouseenter 会触发 tip 弹出造成抖动；
/// 高度贴合这时也在跟动画抢窗口尺寸，一并停掉。
let pinAnimating = false;
let pinAnimatingTimer = 0;
function beginPinAnimation(ms = 300) {
  pinAnimating = true;
  clearTimeout(pinAnimatingTimer);
  // 定住方向 Rust 侧动画约 180ms，留点余量；取消定住方向 Rust 要先等标题栏恢复
  // （最长约 600ms）再播动画，抑制窗要盖到整段过渡结束，见 setPinned(false)
  pinAnimatingTimer = setTimeout(() => {
    pinAnimating = false;
  }, ms);
}

async function setPinned(on) {
  if (pinBusy) return;
  pinBusy = true;
  setTimeout(() => {
    pinBusy = false;
  }, 500);
  beginPinAnimation();
  try {
    if (on) {
      closeSettings();
      // 先切到悬浮布局：一是量出内容宽度，好让窗口一次缩到位（动画结束就不再跳一下）；
      // 二是让收缩过程里展示的就是悬浮卡片（Rust 随后还会补发一次 pin-state）
      document.body.classList.add("pinned");
      updateFloat();
      const w = measureFloatWidth();
      floatWidth = w;
      await invoke("set_pinned", { on, width: w || null });
    } else {
      // 退出悬浮态时一并收起提示窗（Rust 侧也会收，这里先收掉避免闪一下）
      hideTip();
      // 取消定住的整段过渡（等标题栏 + 动画）最长约 800ms，这期间窗口还是窄尺寸：
      // 抑制 tip 弹出，也抑制高度贴合（贴合会保持当前窄宽度，把窄宽度固化下来）
      beginPinAnimation(1000);
      // 趁还在定住态先量主视图贴合高度（摘 class → 量 → 还原，同一任务内完成不会重绘），
      // 让 Rust 的取消定住动画直接落到这个高度，落地后不再二次调高。
      // 注意量之前把 body 临时固定到普通窗宽 420px：此刻窗口还是悬浮态的窄宽度，
      // 按窄宽度量会让卡片文字换行更多、高度偏大，回到 420 宽后底部就多出空白
      document.body.classList.remove("pinned");
      const prevBodyW = document.body.style.width;
      document.body.style.width = "420px";
      const h = measureContentHeight();
      document.body.style.width = prevBodyW;
      document.body.classList.add("pinned");
      lastFitH = h;
      bootFitted = true;
      await invoke("set_pinned", { on, height: h || null });
      // 兜底：等 Rust 的取消定住动画完全落地后（含等标题栏的时间）再用最终布局量一次，
      // 偏差超过阈值就纠正（量准则无动作）。太早量会量到过渡中的窄宽度
      setTimeout(() => fitWindowHeight(true), 1000);
    }
  } catch (e) {
    if (on) document.body.classList.remove("pinned");
    showError(String(e));
  }
}

// 由 Rust 在动画开始前发出，让内容布局与窗口缩放同步切换
window.__TAURI__.event.listen("pin-state", (e) => {
  const on = !!e.payload;
  document.body.classList.toggle("pinned", on);
  if (on) updateFloat();
  else hideTip();
});

el.pinBtn.addEventListener("click", () => setPinned(true));
el.floatClose.addEventListener("click", () => setPinned(false));

// 悬浮窗整卡可点：点一下立刻重取数据（锁定态且需钥匙串则先走解锁）。
// 卡片同时是原生拖拽区，拖完系统会补一个 click，所以要按位移把拖拽排掉。
// 这里必须用屏幕绝对坐标 e.screenX/e.screenY：拖拽时窗口跟着指针一起走，
// 指针相对窗口的 clientX/clientY 几乎不变，只有屏幕坐标才反映真实位移。
let floatDown = null;
el.floatCard.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  floatDown = { x: e.screenX, y: e.screenY };
  // 一按下去（可能拖拽）就收起提示窗，别让提示跟着窗口一起拖
  tipHold = true;
  clearTimeout(tipTimer);
  hideTip();
});
el.floatCard.addEventListener("pointercancel", () => {
  floatDown = null;
});
el.floatCard.addEventListener("click", (e) => {
  if (e.target.closest(".float-close")) return;
  const moved =
    floatDown && Math.hypot(e.screenX - floatDown.x, e.screenY - floatDown.y) > 6;
  floatDown = null;
  if (moved) return;
  if (lockedScene) connectSource(activeSource);
  else refresh(true);
});

// 悬浮卡片右键菜单：由 Rust 弹系统原生菜单（网页里自绘会被这么小的窗口裁掉），
// 菜单项「退出悬浮窗」走 menu-unpin 事件回到 setPinned，「退出软件」由 Rust 直接退出。
// 这里 preventDefault 才不会再弹出 WebView 自带的 Reload 菜单。
el.floatCard.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  invoke("show_card_menu").catch(() => {});
});

window.__TAURI__.event.listen("menu-unpin", () => setPinned(false));

// 主窗口关闭按钮 = 定住：Rust 拦下关闭后发这个事件，这里走与「定住」按钮同一条路径
window.__TAURI__.event.listen("close-to-pin", () => setPinned(true));

/* ---------- 悬浮卡片悬停提示窗 ---------- */

/// 提示窗内容比卡片窗口还高，所以它落在另一个小窗口里（见 Rust 的 show_float_tip），
/// 这里只负责算内容和决定什么时候让它出现 / 收起。
/// 各行内容由数据源自己给（`float.tip`）——「预计可用天数」的口径各平台不同，
/// 比如 Trae 的月配额型按「订阅包已用 ÷ 已过天数」当场算，不走本地采样。
let lastUpdatedAt = "";
let tipShown = false;
/// 指针按下过（拖拽 / 点击）之后先别弹，等指针重新进来或松手后再弹
let tipHold = false;
let tipTimer = null;

/// 组装提示窗内容：标题 → 剩余 → 各平台自己的行 → 页脚
function buildTipPayload() {
  const f = lastFloat;
  if (!f) return null;

  const rows = [[f.remainLabel || "剩余", `${f.remain ?? "—"}${f.unit ? ` ${f.unit}` : ""}`]];
  for (const [k, v, tone] of f.tip || []) rows.push(tone ? [k, v, tone] : [k, v]);

  return {
    title: f.title || el.subtitle.textContent || "用量",
    rows,
    foot: [`更新于 ${lastUpdatedAt || "—"}`, "点击面板可刷新"],
  };
}

async function showTip() {
  if (!document.body.classList.contains("pinned")) return;
  // 过渡动画期间窗口在鼠标底下扫过，此时弹 tip 会造成抖动，跳过
  if (pinAnimating) return;
  const payload = buildTipPayload();
  if (!payload) return;
  tipShown = true;
  try {
    await invoke("show_float_tip", { payload });
  } catch (_) {
    tipShown = false;
  }
}

async function hideTip() {
  if (!tipShown) return;
  tipShown = false;
  try {
    await invoke("hide_float_tip");
  } catch (_) {
    /* 收起失败也不影响使用，下次进入还会重设 */
  }
}

// 停在卡片上：略等一下再弹（只是划过就不弹了）；离开就收。
// 松手后再等一拍重弹，这样「点一下刷新」之后提示还会回来。
el.floatCard.addEventListener("mouseenter", () => {
  if (tipHold) return;
  clearTimeout(tipTimer);
  tipTimer = setTimeout(showTip, 150);
});
el.floatCard.addEventListener("mouseleave", () => {
  clearTimeout(tipTimer);
  tipHold = false;
  hideTip();
});
el.floatCard.addEventListener("pointerup", () => {
  tipHold = false;
  clearTimeout(tipTimer);
  tipTimer = setTimeout(showTip, 400);
});

// 倒计时每秒走一格；自动刷新定时器由 applyRefreshInterval 按当前场景的间隔维护
setInterval(() => {
  // 当前场景未连接：倒计时冻结（显示「待连接」），不做任何刷新
  if (!activeReady()) return;
  secondsLeft -= 1;
  if (secondsLeft <= 0) {
    secondsLeft = refreshInterval;
  }
  updateCountdown();
}, 1000);

async function bootstrap() {
  // 先拉清单（不触碰钥匙串）：让 tab / 列表 / 待连接卡片都能拿到平台名，再取数
  try {
    await reloadCatalog();
    const cur = catalog.find((s) => s.id === activeSource);
    // 已保存凭证但本次尚未连接的钥匙串源不弹设置（主界面已给「连接账号」入口）；
    // trae 不可用 / 确实未配置仍按原逻辑引导去设置页录入。
    if (!cur?.configured) openSettings();
  } catch (_) {
    /* 忽略：清单读取失败不影响主流程 */
  }
  await refresh(false);
}

// 启动流程：窗口以 visible:false 创建（否则会先闪一下主界面大窗、悬浮态很穿帮）。
// 先问 Rust 上次是不是定住态，在窗口还隐藏着的时候就把布局切成对应的形态，再显示，最后才开始取数。
(async () => {
  let pinnedAtStart = false;
  try {
    const st = await invoke("startup_state");
    pinnedAtStart = !!st.pinned;
    document.body.classList.toggle("pinned", pinnedAtStart);
    if (pinnedAtStart) updateFloat();
  } catch (_) {
    /* 拿不到状态就按主界面显示 */
  }
  try {
    await invoke("show_window");
  } catch (_) {
    /* 显示失败也有 Rust 侧 3 秒兜底 */
  }
  bootstrap();
})();
