// 各数据源渲染模块共用的格式化工具
window.AFP_UTIL = (function () {
  function fmtNum(v) {
    const n = Number(v);
    if (!isFinite(n)) return "—";
    const isInt = Math.abs(n - Math.round(n)) < 1e-9;
    return n.toLocaleString("en-US", {
      minimumFractionDigits: isInt ? 0 : 2,
      maximumFractionDigits: isInt ? 0 : 2,
    });
  }

  /// 把重置时间戳格式化成「08:00 / 明 08:00 / MM-DD 08:00」
  function fmtReset(ms, compact = false) {
    if (ms === undefined || ms === null) return "—";
    const dt = new Date(Number(ms));
    if (isNaN(dt.getTime())) return "—";
    const now = new Date();
    const hm = dt.toTimeString().slice(0, 5).padStart(5, "0");
    const sameDay = dt.toDateString() === now.toDateString();
    const tomorrow = new Date(now);
    tomorrow.setDate(now.getDate() + 1);
    if (sameDay) return compact ? hm : `今天 ${hm}`;
    if (dt.toDateString() === tomorrow.toDateString()) return compact ? `明 ${hm}` : `明天 ${hm}`;
    const mm = String(dt.getMonth() + 1).padStart(2, "0");
    const dd = String(dt.getDate()).padStart(2, "0");
    return `${mm}-${dd} ${hm}`;
  }

  /// 已用比例 -> 颜色：< 30% 绿、30~60% 蓝、60~85% 橙、≥ 85% 红
  function ringColor(pct) {
    if (pct >= 85) return "#f87171";
    if (pct >= 60) return "#fbbf24";
    if (pct >= 30) return "#22d3ee";
    return "#34d399";
  }

  /// 宽松取数：任意能转成正数的值
  function num(v) {
    const n = parseFloat(v);
    return isFinite(n) ? n : null;
  }

  return { fmtNum, fmtReset, ringColor, num };
})();
