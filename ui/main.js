const { invoke } = window.__TAURI__.core;

// Windows 使用系统原生标题栏，去掉为 macOS 红绿灯预留的左侧内边距
if (/Windows/i.test(navigator.userAgent)) {
  document.body.classList.add("is-windows");
}

const REFRESH_SECONDS = 60;

const WINDOWS = [
  { field: "AFPFiveHour", label: "近 5 小时" },
  { field: "AFPWeekly", label: "近 1 周" },
  { field: "AFPMonthly", label: "近 1 月" },
];

const PLAN_NAMES = {
  small: "Small 档",
  medium: "Medium 档",
  large: "Large 档",
  max: "Max 档",
};

function planLabel(plan) {
  if (!plan) return "—";
  return PLAN_NAMES[String(plan).toLowerCase()] || String(plan);
}

const R = 32;
const CIRC = 2 * Math.PI * R;
const FR = 20;
const FCIRC = 2 * Math.PI * FR;

const el = {
  cards: document.getElementById("cards"),
  badge: document.getElementById("plan-badge"),
  statusDot: document.getElementById("status-dot"),
  statusText: document.getElementById("status-text"),
  countdown: document.getElementById("countdown"),
  error: document.getElementById("error-banner"),
  refreshBtn: document.getElementById("refresh-btn"),
  refreshText: document.getElementById("refresh-text"),
  subtitle: document.getElementById("subtitle"),
  pinBtn: document.getElementById("pin-btn"),
  floatClose: document.getElementById("float-close"),
  floatRemain: document.getElementById("float-remain"),
  floatSub: document.getElementById("float-sub"),
  floatRingFg: document.getElementById("float-ring-fg"),
  settingsBtn: document.getElementById("settings-btn"),
  settingsModal: document.getElementById("settings-modal"),
  settingsBackdrop: document.getElementById("settings-backdrop"),
  settingsClose: document.getElementById("settings-close"),
  settingsCancel: document.getElementById("settings-cancel"),
  settingsSave: document.getElementById("settings-save"),
  settingsTest: document.getElementById("settings-test"),
  settingsClear: document.getElementById("settings-clear"),
  settingsMsg: document.getElementById("settings-msg"),
  akInput: document.getElementById("ak-input"),
  skInput: document.getElementById("sk-input"),
  skToggle: document.getElementById("sk-toggle"),
  credsPath: document.getElementById("creds-path"),
  keyHelp: document.getElementById("key-help"),
};

const KEY_URL = "https://console.volcengine.com/iam/keymanage";

let secondsLeft = REFRESH_SECONDS;
let loading = false;
let lastFiveHour = null;

function fmtNum(v) {
  if (!isFinite(v)) return "-";
  const isInt = Math.abs(v - Math.round(v)) < 1e-9;
  return v.toLocaleString("en-US", {
    minimumFractionDigits: isInt ? 0 : 2,
    maximumFractionDigits: isInt ? 0 : 2,
  });
}

function fmtReset(ms) {
  if (ms === undefined || ms === null) return "—";
  const dt = new Date(Number(ms));
  if (isNaN(dt.getTime())) return "—";
  const now = new Date();
  const hm = dt.toTimeString().slice(0, 5).padStart(5, "0");
  const sameDay = dt.toDateString() === now.toDateString();
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  if (sameDay) return `今天 ${hm}`;
  if (dt.toDateString() === tomorrow.toDateString()) return `明天 ${hm}`;
  const mm = String(dt.getMonth() + 1).padStart(2, "0");
  const dd = String(dt.getDate()).padStart(2, "0");
  return `${mm}-${dd} ${hm}`;
}

function ringColor(pct) {
  if (pct >= 85) return "#f87171";
  if (pct >= 60) return "#fbbf24";
  return "#22d3ee";
}

function buildCard(win, label) {
  const quota = parseFloat(win?.Quota ?? 0) || 0;
  const used = parseFloat(win?.Used ?? 0) || 0;
  const pct = quota > 0 ? (used / quota) * 100 : 0;
  const clamped = Math.max(0, Math.min(100, pct));
  const remain = Math.max(0, quota - used);
  const offset = CIRC * (1 - clamped / 100);
  const color = ringColor(pct);

  const card = document.createElement("div");
  card.className = "card";
  card.innerHTML = `
    <div class="ring">
      <svg width="78" height="78" viewBox="0 0 78 78">
        <circle class="ring-bg" cx="39" cy="39" r="${R}" fill="none" stroke-width="7" />
        <circle class="ring-fg" cx="39" cy="39" r="${R}" fill="none" stroke-width="7"
          stroke="${color}"
          stroke-dasharray="${CIRC.toFixed(2)}"
          stroke-dashoffset="${offset.toFixed(2)}" />
      </svg>
      <div class="ring-label">
        <div class="ring-pct" style="color:${color}">${pct.toFixed(1)}%</div>
        <div class="ring-unit">已用</div>
      </div>
    </div>
    <div class="card-info">
      <div class="card-title">${label}</div>
      <div class="card-sub">剩余 <span class="remain">${fmtNum(remain)}</span> AFP · 共 ${fmtNum(quota)}</div>
      <div class="card-reset">重置 ${fmtReset(win?.ResetTime)}</div>
    </div>
  `;
  return card;
}

function updateFloat() {
  const fg = el.floatRingFg;
  if (!fg) return;
  const w = lastFiveHour;
  if (!w) {
    el.floatRemain.textContent = "—";
    el.floatSub.textContent = "暂无数据";
    fg.setAttribute("stroke-dashoffset", FCIRC.toFixed(2));
    return;
  }
  const quota = parseFloat(w.Quota ?? 0) || 0;
  const used = parseFloat(w.Used ?? 0) || 0;
  const pct = quota > 0 ? (used / quota) * 100 : 0;
  const clamped = Math.max(0, Math.min(100, pct));
  const remain = Math.max(0, quota - used);
  const color = ringColor(pct);

  fg.setAttribute("stroke", color);
  fg.setAttribute("stroke-dasharray", FCIRC.toFixed(2));
  fg.setAttribute("stroke-dashoffset", (FCIRC * (1 - clamped / 100)).toFixed(2));

  el.floatRemain.textContent = fmtNum(remain);
  el.floatRemain.style.color = color;
  el.floatSub.textContent = `已用 ${pct.toFixed(1)}% · 共 ${fmtNum(quota)}`;
}

function render(result) {
  el.badge.textContent = planLabel(result?.PlanType);

  lastFiveHour = result?.AFPFiveHour ?? null;
  updateFloat();

  el.cards.innerHTML = "";
  let painted = 0;
  for (const { field, label } of WINDOWS) {
    const win = result?.[field];
    if (!win) continue;
    el.cards.appendChild(buildCard(win, label));
    painted++;
  }
  if (painted === 0) {
    el.cards.innerHTML = `<div class="empty">暂无用量数据</div>`;
  }
}

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

async function refresh(manual = false) {
  if (loading) return;
  loading = true;
  el.refreshBtn.classList.add("spinning");
  el.refreshText.textContent = "刷新中";
  if (manual) hideError();

  try {
    const result = await invoke("get_afp_usage");
    render(result);
    hideError();
    const now = new Date().toLocaleTimeString("zh-CN", { hour12: false });
    setStatus("live", `更新于 ${now}`);
    el.subtitle.textContent = "Agent Plan 套餐额度";
  } catch (e) {
    setStatus("err", "获取失败");
    showError(String(e));
  } finally {
    loading = false;
    el.refreshBtn.classList.remove("spinning");
    el.refreshText.textContent = "刷新";
    secondsLeft = REFRESH_SECONDS;
    updateCountdown();
  }
}

function updateCountdown() {
  el.countdown.textContent = `${secondsLeft}s 后自动刷新`;
}

el.refreshBtn.addEventListener("click", () => refresh(true));

/* ---------- 设置：火山引擎凭证 ---------- */

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

async function openSettings() {
  setSettingsMsg("", "");
  try {
    const c = await invoke("get_credentials");
    if (c.path) el.credsPath.textContent = c.path;
    el.akInput.value = c.accessKey || "";
    el.skInput.value = c.secretKey || "";
    if (c.source === "env") {
      setSettingsMsg("", "当前生效的是 .env / 环境变量中的凭证，保存后将以本机设置为准。");
    }
  } catch (e) {
    setSettingsMsg("err", String(e));
  }
  el.settingsModal.classList.remove("hidden");
  el.akInput.focus();
}

function closeSettings() {
  el.settingsModal.classList.add("hidden");
  setSettingsMsg("", "");
}

function readCreds() {
  return { ak: el.akInput.value.trim(), sk: el.skInput.value.trim() };
}

async function saveSettings() {
  const { ak, sk } = readCreds();
  if (!ak || !sk) {
    setSettingsMsg("err", "请填写完整的 Access Key 与 Secret Key");
    return;
  }
  el.settingsSave.disabled = true;
  el.settingsSave.textContent = "保存中…";
  try {
    await invoke("save_credentials", { accessKey: ak, secretKey: sk });
    closeSettings();
    await refresh(true);
  } catch (e) {
    setSettingsMsg("err", String(e));
  } finally {
    el.settingsSave.disabled = false;
    el.settingsSave.textContent = "保存";
  }
}

async function testSettings() {
  const { ak, sk } = readCreds();
  if (!ak || !sk) {
    setSettingsMsg("err", "请先填写 Access Key 与 Secret Key");
    return;
  }
  el.settingsTest.disabled = true;
  el.settingsTest.textContent = "测试中…";
  try {
    const r = await invoke("test_credentials", { accessKey: ak, secretKey: sk });
    setSettingsMsg("ok", `连接成功 · ${planLabel(r?.PlanType)}`);
  } catch (e) {
    setSettingsMsg("err", String(e));
  } finally {
    el.settingsTest.disabled = false;
    el.settingsTest.textContent = "测试连接";
  }
}

async function clearSettings() {
  try {
    await invoke("clear_credentials");
    el.akInput.value = "";
    el.skInput.value = "";
    setSettingsMsg("ok", "已清除本机保存的凭证");
  } catch (e) {
    setSettingsMsg("err", String(e));
  }
}

el.settingsBtn.addEventListener("click", openSettings);
el.settingsClose.addEventListener("click", closeSettings);
el.settingsCancel.addEventListener("click", closeSettings);
el.settingsBackdrop.addEventListener("click", closeSettings);
el.settingsSave.addEventListener("click", saveSettings);
el.settingsTest.addEventListener("click", testSettings);
el.settingsClear.addEventListener("click", clearSettings);

el.skToggle.addEventListener("click", () => {
  const isPw = el.skInput.type === "password";
  el.skInput.type = isPw ? "text" : "password";
  el.skToggle.textContent = isPw ? "隐藏" : "显示";
});

el.keyHelp.addEventListener("click", (e) => {
  e.preventDefault();
  invoke("open_url", { url: KEY_URL }).catch(() => {});
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && settingsOpen()) closeSettings();
});

async function setPinned(on) {
  try {
    if (on) closeSettings();
    await invoke("set_pinned", { on });
    document.body.classList.toggle("pinned", on);
    if (on) updateFloat();
  } catch (e) {
    showError(String(e));
  }
}

el.pinBtn.addEventListener("click", () => setPinned(true));
el.floatClose.addEventListener("click", () => setPinned(false));

setInterval(() => {
  secondsLeft -= 1;
  if (secondsLeft <= 0) {
    secondsLeft = REFRESH_SECONDS;
  }
  updateCountdown();
}, 1000);

setInterval(() => refresh(false), REFRESH_SECONDS * 1000);

async function bootstrap() {
  await refresh(false);
  try {
    const c = await invoke("get_credentials");
    if (!c.configured) openSettings();
  } catch (_) {
    /* 忽略：get_credentials 失败不影响主流程 */
  }
}

bootstrap();
