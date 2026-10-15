// Trae Code 积分：本机 Trae CN 登录态自动解密取 token，POST /trae/api/v2/pay/ide_user_ent_usage
// 后端下发的 data = { raw: 接口原始响应, identity: 套餐身份, expiredAt: 登录态到期, file: 登录态文件 }
(function () {
  const { fmtNum, num, ringColor } = window.AFP_UTIL;

  const R = 32;
  const CIRC = 2 * Math.PI * R;

  /// 复刻官方聚合口径：遍历 user_entitlement_pack_list
  /// credits_limit = -1 视为不限量；> 0 累加总额，usage.credits_amount 累加已用
  function aggregate(raw) {
    const packs = Array.isArray(raw?.user_entitlement_pack_list)
      ? raw.user_entitlement_pack_list
      : [];
    let used = 0;
    let total = 0;
    let unlimited = false;
    for (const pk of packs) {
      const quota = pk?.entitlement_base_info?.quota || {};
      const limit = quota.credits_limit;
      const amount = num(pk?.usage?.credits_amount) ?? 0;
      if (limit === -1) {
        unlimited = true;
        continue;
      }
      const l = num(limit);
      if (l !== null && l > 0) {
        total += l;
        used += amount;
      }
    }
    const remaining = unlimited ? null : Math.max(total - used, 0);
    return { used, total, unlimited, remaining, hasPacks: packs.length > 0 };
  }

  /// 登录态到期时间（ISO）-> MM-DD HH:MM
  function fmtDate(iso) {
    if (!iso) return "";
    const dt = new Date(iso);
    if (isNaN(dt.getTime())) return "";
    const mm = String(dt.getMonth() + 1).padStart(2, "0");
    const dd = String(dt.getDate()).padStart(2, "0");
    const hm = dt.toTimeString().slice(0, 5);
    return `${mm}-${dd} ${hm}`;
  }

  window.AFP_SOURCES = window.AFP_SOURCES || {};
  window.AFP_SOURCES.trae = {
    render(data) {
      const raw = data?.raw ?? data ?? {};
      const identity = data?.identity || "";
      const expiredAt = data?.expiredAt || "";
      const agg = aggregate(raw);
      const badge = identity || "Trae";

      if (!agg.hasPacks) {
        return {
          badge,
          summary: "已连通，但返回里没有套餐信息",
          html: `<div class="empty">已连通，但没有可解析的积分字段</div>`,
          float: null,
        };
      }

      if (agg.unlimited) {
        return {
          badge,
          summary: "不限量套餐",
          html: `
            <div class="card">
              <div class="card-info">
                <div class="card-title">Trae Code 积分</div>
                <div class="card-sub">剩余 <span class="remain">∞</span> 积分 · 不限量套餐</div>
                <div class="card-reset">${identity ? `套餐 ${identity}` : "重置 —"}</div>
              </div>
            </div>`,
          float: {
            label: "Trae Code",
            remain: "∞",
            unit: "",
            sub: identity ? `${identity} · 不限量` : "不限量套餐",
            reset: "",
            pct: null,
            color: ringColor(0),
          },
        };
      }

      const pct = agg.total > 0 ? (agg.used / agg.total) * 100 : 0;
      const clamped = Math.max(0, Math.min(100, pct));
      const color = ringColor(pct);
      const offset = CIRC * (1 - clamped / 100);
      const remainTxt = fmtNum(agg.remaining);
      const sub = identity || `已用 ${pct.toFixed(1)}%`;

      return {
        badge,
        summary: `剩余积分 ${remainTxt}`,
        html: `
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
              <div class="card-title">Trae Code 积分</div>
              <div class="card-sub">剩余 <span class="remain">${remainTxt}</span> 积分 · 已用 ${fmtNum(agg.used)} / 共 ${fmtNum(agg.total)}</div>
              <div class="card-reset">重置 —${expiredAt ? ` · 登录态 ${fmtDate(expiredAt)} 到期` : ""}</div>
            </div>
          </div>`,
        float: {
          label: "Trae Code",
          remain: remainTxt,
          unit: "积分",
          sub,
          reset: "",
          pct,
          color,
        },
      };
    },
  };
})();
