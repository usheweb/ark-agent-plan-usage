// DeepSeek 开放平台：GET /user/balance（Bearer API Key）
// 返回：{ is_available, balance_infos: [{ currency, total_balance, granted_balance, topped_up_balance }] }
(function () {
  const { fmtNum, num } = window.AFP_UTIL;

  window.AFP_SOURCES = window.AFP_SOURCES || {};
  window.AFP_SOURCES.deepseek = {
    render(data) {
      const info = (Array.isArray(data?.balance_infos) ? data.balance_infos[0] : null) || {};
      const total = num(info.total_balance);
      const granted = num(info.granted_balance);
      const topped = num(info.topped_up_balance);
      const currency = info.currency || "CNY";
      const available = data?.is_available !== false;

      if (total === null) {
        return {
          badge: "—",
          summary: "已连通，但没解析出余额字段",
          html: `<div class="empty">已连通，但返回里没有可解析的余额字段</div>`,
          float: null,
        };
      }

      const subs = [];
      if (topped !== null) subs.push(`充值 ${fmtNum(topped)}`);
      if (granted !== null) subs.push(`赠送 ${fmtNum(granted)}`);

      return {
        badge: currency,
        summary: `可用余额 ${fmtNum(total)} ${currency}`,
        html: `
          <div class="balance-card">
            <div class="balance-title">DeepSeek 账户余额</div>
            <div class="balance-value"><span>${fmtNum(total)}</span><em>${currency}</em></div>
            <div class="balance-rows">
              ${topped !== null ? `<div class="balance-row"><span>充值余额</span><b>${fmtNum(topped)}</b></div>` : ""}
              ${granted !== null ? `<div class="balance-row"><span>赠送余额</span><b>${fmtNum(granted)}</b></div>` : ""}
            </div>
            ${available ? "" : `<div class="balance-warn">该账户当前不可用</div>`}
          </div>`,
        float: {
          label: "账户余额",
          remain: fmtNum(total),
          unit: currency,
          sub: subs.join(" · "),
          reset: available ? "" : "账户不可用",
          pct: null,
        },
      };
    },
  };
})();
