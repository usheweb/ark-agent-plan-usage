// 硅基流动 SiliconFlow：GET /v1/user/info（Bearer API Key）
// 返回：{ code, message, status, data: { name, balance, charge_balance, total_balance, status } }
(function () {
  const { fmtNum, num } = window.AFP_UTIL;

  window.AFP_SOURCES = window.AFP_SOURCES || {};
  window.AFP_SOURCES.siliconflow = {
    render(data) {
      const d = data?.data || {};
      const total = num(d.balance) ?? num(d.total_balance) ?? null;
      const charge = num(d.charge_balance) ?? null;
      const grant = num(d.total_balance) !== null && total !== null
        ? Math.max(0, num(d.total_balance) - (charge ?? 0))
        : null;
      const currency = "CNY";
      const accountName = d.name || "";
      const frozen = String(d.status || "").toLowerCase() !== "normal" && d.status;

      if (total === null) {
        return {
          badge: "—",
          summary: "已连通，但没解析出余额字段",
          html: `<div class="empty">已连通，但返回里没有可解析的余额字段</div>`,
          float: null,
        };
      }

      const subs = [];
      if (charge !== null) subs.push(`充值 ${fmtNum(charge)}`);
      if (grant !== null) subs.push(`赠送 ${fmtNum(grant)}`);

      return {
        badge: currency,
        summary: `可用余额 ${fmtNum(total)} ${currency}`,
        html: `
          <div class="balance-card">
            <div class="balance-title">硅基流动账户余额${accountName ? ` · ${accountName}` : ""}</div>
            <div class="balance-value"><span>${fmtNum(total)}</span><em>${currency}</em></div>
            <div class="balance-rows">
              ${charge !== null ? `<div class="balance-row"><span>充值余额</span><b>${fmtNum(charge)}</b></div>` : ""}
              ${grant !== null ? `<div class="balance-row"><span>赠送金额</span><b>${fmtNum(grant)}</b></div>` : ""}
            </div>
            ${frozen ? `<div class="balance-warn">账户状态：${d.status}</div>` : ""}
          </div>`,
        float: {
          label: "账户余额",
          remain: fmtNum(total),
          unit: currency,
          sub: subs.join(" · "),
          reset: frozen ? `状态 ${d.status}` : "",
          pct: null,
          // 悬停提示窗：余额类场景按「余额 ÷ 日均消耗」估算可用天数
          title: "硅基流动余额",
          remainNum: total,
          tip: [
            ...(charge !== null ? [["充值余额", `${fmtNum(charge)} ${currency}`]] : []),
            ...(grant !== null ? [["赠送金额", `${fmtNum(grant)} ${currency}`]] : []),
          ],
        },
      };
    },
  };
})();
