// 火山方舟：GetAFPUsage —— 三档套餐额度（近 5 小时 / 近 1 周 / 近 1 月）
(function () {
  const { fmtNum, fmtReset, ringColor } = window.AFP_UTIL;

  const R = 32;
  const CIRC = 2 * Math.PI * R;

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

  function card(win, label) {
    if (!win) return "";
    const quota = parseFloat(win.Quota ?? 0) || 0;
    const used = parseFloat(win.Used ?? 0) || 0;
    const pct = quota > 0 ? (used / quota) * 100 : 0;
    const clamped = Math.max(0, Math.min(100, pct));
    const remain = Math.max(0, quota - used);
    const offset = CIRC * (1 - clamped / 100);
    const color = ringColor(pct);

    return `
      <div class="card">
        <div class="ring">
          <svg width="78" height="78" viewBox="0 0 78 78">
            <circle class="ring-bg" cx="39" cy="39" r="${R}" fill="none" stroke-width="7" />
            <circle class="ring-fg" cx="39" cy="39" r="${R}" fill="none" stroke-width="7"
              stroke="${color}" stroke-dasharray="${CIRC.toFixed(2)}"
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
          <div class="card-reset">重置 ${fmtReset(win.ResetTime)}</div>
        </div>
      </div>`;
  }

  window.AFP_SOURCES = window.AFP_SOURCES || {};
  window.AFP_SOURCES.volcengine = {
    render(data) {
      const fiveHour = data?.AFPFiveHour ?? null;
      const cards = WINDOWS.map((w) => card(data?.[w.field], w.label)).join("");
      const badge = planLabel(data?.PlanType);

      if (!fiveHour) {
        return {
          badge,
          summary: "已连通，但暂无用量数据",
          html: `<div class="empty">暂无用量数据</div>`,
          float: null,
        };
      }

      const quota = parseFloat(fiveHour.Quota ?? 0) || 0;
      const used = parseFloat(fiveHour.Used ?? 0) || 0;
      const pct = quota > 0 ? (used / quota) * 100 : 0;
      const remain = Math.max(0, quota - used);
      const color = ringColor(pct);

      return {
        badge,
        summary: `近 5 小时剩余 ${fmtNum(remain)} AFP`,
        html: cards || `<div class="empty">暂无用量数据</div>`,
        float: {
          label: "近 5 小时剩余",
          remain: fmtNum(remain),
          unit: "AFP",
          sub: `已用 ${pct.toFixed(1)}% · 共 ${fmtNum(quota)}`,
          reset: `${fmtReset(fiveHour.ResetTime, true)} 重置`,
          pct,
          color,
        },
      };
    },
  };
})();
