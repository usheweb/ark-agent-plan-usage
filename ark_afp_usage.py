#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""查询火山方舟 Agent Plan 套餐的 AFP（Agent Fuel Point）额度用量。

对接接口: GetAFPUsage（仅个人版 Agent Plan 套餐）
  POST https://ark.cn-beijing.volcengineapi.com/?Action=GetAFPUsage&Version=2024-01-01

鉴权: 使用火山引擎 Access Key（AK/SK）签名。AK/SK 从环境变量读取:
  export VOLC_ACCESSKEY=你的AK
  export VOLC_SECRETKEY=你的SK
在火山引擎「Access Key 管理」页获取: https://console.volcengine.com/iam/keymanage

用法:
  python3 ark_afp_usage.py              # 查询一次并打印
  python3 ark_afp_usage.py -w 60        # 每 60 秒刷新一次（Ctrl-C 退出）
  python3 ark_afp_usage.py --json       # 输出原始 JSON
"""

import os
import sys
import json
import time
import hmac
import hashlib
import argparse
import datetime
import urllib.request
import urllib.error

HOST = "ark.cn-beijing.volcengineapi.com"
REGION = "cn-beijing"
SERVICE = "ark"
VERSION = "2024-01-01"
ACTION = "GetAFPUsage"

# 四个滚动窗口的展示顺序与中文名（接口返回的字段名 -> 展示名）
WINDOWS = [
    ("AFPFiveHour", "近 5 小时"),
    ("AFPDaily", "近 1 天"),
    ("AFPWeekly", "近 1 周"),
    ("AFPMonthly", "近 1 月"),
]


def _sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _hmac_sha256(key: bytes, msg: str) -> bytes:
    return hmac.new(key, msg.encode("utf-8"), hashlib.sha256).digest()


def build_signed_request(ak: str, sk: str):
    """按火山引擎 V4 签名算法构造请求（签名流程与 AWS SigV4 一致）。"""
    body = b"{}"
    now = datetime.datetime.now(datetime.timezone.utc)
    x_date = now.strftime("%Y%m%dT%H%M%SZ")     # 例如 20261014T053002Z
    date_short = now.strftime("%Y%m%d")          # 例如 20261014

    content_sha256 = _sha256_hex(body)
    signed_headers = "content-type;host;x-content-sha256;x-date"
    canonical_headers = (
        "content-type:application/json\n"
        "host:%s\n"
        "x-content-sha256:%s\n"
        "x-date:%s\n" % (HOST, content_sha256, x_date)
    )
    canonical_query = "Action=%s&Version=%s" % (ACTION, VERSION)
    canonical_request = "\n".join([
        "POST",
        "/",
        canonical_query,
        canonical_headers,       # 末尾自带 \n，与公式叠加形成空行，符合规范
        signed_headers,
        content_sha256,
    ])

    credential_scope = "%s/%s/%s/request" % (date_short, REGION, SERVICE)
    string_to_sign = "\n".join([
        "HMAC-SHA256",
        x_date,
        credential_scope,
        _sha256_hex(canonical_request.encode("utf-8")),
    ])

    signing_key = _hmac_sha256(sk.encode("utf-8"), date_short)
    signing_key = _hmac_sha256(signing_key, REGION)
    signing_key = _hmac_sha256(signing_key, SERVICE)
    signing_key = _hmac_sha256(signing_key, "request")
    signature = hmac.new(
        signing_key, string_to_sign.encode("utf-8"), hashlib.sha256
    ).hexdigest()

    authorization = (
        "HMAC-SHA256 Credential=%s/%s, SignedHeaders=%s, Signature=%s"
        % (ak, credential_scope, signed_headers, signature)
    )
    headers = {
        "Content-Type": "application/json",
        "X-Date": x_date,
        "X-Content-Sha256": content_sha256,
        "Authorization": authorization,
        "Host": HOST,
    }
    url = "https://%s/?%s" % (HOST, canonical_query)
    return url, headers, body


def fetch_afp_usage(ak: str, sk: str) -> dict:
    url, headers, body = build_signed_request(ak, sk)
    req = urllib.request.Request(url, data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=20) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "replace")
        raise RuntimeError("接口返回 HTTP %s: %s" % (e.code, detail))
    if "Result" not in payload:
        raise RuntimeError("返回异常: %s" % json.dumps(payload, ensure_ascii=False))
    return payload["Result"]


def _bar(pct: float, width: int = 24) -> str:
    filled = max(0, min(width, int(round(pct / 100.0 * width))))
    return "█" * filled + "░" * (width - filled)


def _fmt_reset(ms) -> str:
    try:
        dt = datetime.datetime.fromtimestamp(int(ms) / 1000.0)
    except (TypeError, ValueError):
        return "-"
    now = datetime.datetime.now()
    if dt.date() == now.date():
        return dt.strftime("今天 %H:%M")
    if dt.date() == (now + datetime.timedelta(days=1)).date():
        return dt.strftime("明天 %H:%M")
    return dt.strftime("%m-%d %H:%M")


def render(result: dict) -> str:
    lines = []
    lines.append("")
    lines.append("  火山方舟 AFP 用量  ·  套餐 %s  ·  %s" % (
        result.get("PlanType", "-"),
        datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
    ))
    lines.append("  " + "─" * 66)
    for field, label in WINDOWS:
        win = result.get(field)
        if not win:
            continue
        try:
            quota = float(win.get("Quota", 0) or 0)
            used = float(win.get("Used", 0) or 0)
        except ValueError:
            quota, used = 0.0, 0.0
        pct = (used / quota * 100.0) if quota > 0 else 0.0
        lines.append("  %-8s %s %6.2f%%   %s / %s AFP    重置 %s" % (
            label, _bar(pct), pct,
            _fmt_num(used), _fmt_num(quota),
            _fmt_reset(win.get("ResetTime")),
        ))
    lines.append("")
    return "\n".join(lines)


def _fmt_num(v: float) -> str:
    # 去掉多余小数：整数就不显示小数点
    if v == int(v):
        return "{:,}".format(int(v))
    return "{:,.2f}".format(v)


def _load_dotenv(path: str = ".env") -> None:
    """若脚本目录下存在 .env，则载入其中尚未设置的环境变量（不覆盖已有值）。"""
    here = os.path.dirname(os.path.abspath(__file__))
    env_path = os.path.join(here, path)
    if not os.path.isfile(env_path):
        return
    with open(env_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, val = line.partition("=")
            key, val = key.strip(), val.strip().strip('"').strip("'")
            if key and key not in os.environ:
                os.environ[key] = val


def main() -> int:
    _load_dotenv()
    parser = argparse.ArgumentParser(description="查询火山方舟 Agent Plan 的 AFP 用量")
    parser.add_argument("-w", "--watch", type=int, metavar="SEC",
                        help="每隔 SEC 秒刷新一次，持续运行")
    parser.add_argument("--json", action="store_true", help="输出原始 JSON")
    args = parser.parse_args()

    ak = os.environ.get("VOLC_ACCESSKEY") or os.environ.get("VOLC_AK")
    sk = os.environ.get("VOLC_SECRETKEY") or os.environ.get("VOLC_SK")
    if not ak or not sk:
        sys.stderr.write(
            "未设置凭证。请先执行（AK/SK 在 https://console.volcengine.com/iam/keymanage 获取）：\n"
            "  export VOLC_ACCESSKEY=你的AK\n"
            "  export VOLC_SECRETKEY=你的SK\n"
        )
        return 2

    while True:
        try:
            result = fetch_afp_usage(ak, sk)
            if args.json:
                print(json.dumps(result, ensure_ascii=False, indent=2))
            else:
                print(render(result))
        except Exception as e:  # noqa: BLE001  面向用户的脚本，统一提示
            sys.stderr.write("查询失败: %s\n" % e)
            if not args.watch:
                return 1
        if not args.watch:
            return 0
        try:
            time.sleep(args.watch)
        except KeyboardInterrupt:
            return 0


if __name__ == "__main__":
    raise SystemExit(main())
