// Kimi 月之暗面（Moonshot）：GET /v1/users/me/balance（Bearer API Key）
// 返回：{ code, status, data: { available_balance, voucher_balance, cash_balance } }
// 官方字段名以文档为准，这里对常见命名做宽松读取，取不到时明确提示而不是猜一个数。
(function () {
  const { fmtNum, num } = window.AFP_UTIL;

  window.AFP_SOURCES = window.AFP_SOURCES || {};
  window.AFP_SOURCES.kimi = {
    render(data) {
      const d = data?.data || {};
      const total =
        num(d.available_balance) ?? num(d.availableBalance) ?? num(d.balance) ?? null;
      const voucher = num(d.voucher_balance) ?? num(d.voucherBalance) ?? null;
      const cash = num(d.cash_balance) ?? num(d.cashBalance) ?? null;
      const currency = "CNY";

      if (total === null) {
        return {
          badge: "—",
          summary: "已连通，但没解析出余额字段",
          html: `<div class="empty">已连通，但返回里没有可解析的余额字段</div>`,
          float: null,
        };
      }

      const subs = [];
      if (cash !== null) subs.push(`现金 ${fmtNum(cash)}`);
      if (voucher !== null) subs.push(`代金券 ${fmtNum(voucher)}`);

      return {
        badge: currency,
        summary: `可用余额 ${fmtNum(total)} ${currency}`,
        html: `
          <div class="balance-card">
            <div class="balance-title">Kimi 账户余额</div>
            <div class="balance-value"><span>${fmtNum(total)}</span><em>${currency}</em></div>
            <div class="balance-rows">
              ${cash !== null ? `<div class="balance-row"><span>现金余额</span><b>${fmtNum(cash)}</b></div>` : ""}
              ${voucher !== null ? `<div class="balance-row"><span>代金券余额</span><b>${fmtNum(voucher)}</b></div>` : ""}
            </div>
          </div>`,
        float: {
          label: "账户余额",
          remain: fmtNum(total),
          unit: currency,
          sub: subs.join(" · "),
          reset: "",
          pct: null,
          // 悬停提示窗：余额类场景按「余额 ÷ 日均消耗」估算可用天数
          title: "Kimi 余额",
          remainNum: total,
          tip: [
            ...(cash !== null ? [["现金余额", `${fmtNum(cash)} ${currency}`]] : []),
            ...(voucher !== null ? [["代金券余额", `${fmtNum(voucher)} ${currency}`]] : []),
          ],
        },
      };
    },
  };
})();
