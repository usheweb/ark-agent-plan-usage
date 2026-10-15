# 唐三浪悬浮窗

**唐三浪悬浮窗** —— 一个轻量的桌面小工具，把各个 AI 服务商的**余额 / 用量**聚合在一个悬浮窗里，随手瞄一眼就知道还剩多少，避免余额欠费导致业务停摆。

支持**悬浮窗显示**，定住后窗口置顶、常驻桌面；悬浮窗整块可点，点一下即重新取数。

---

## 支持的数据源

| 数据源 | 取值口径 | 凭证来源 |
|---|---|---|
| **Trae 积分** | Trae Code 积分额度 | 自动读取本机 Trae CN 登录态，**无需填写** |
| **火山方舟** | Agent Plan 套餐额度 | Access Key / Secret Key |
| **DeepSeek** | 账户余额 | API Key |
| **Kimi 月之暗面** | 账户余额 | API Key |
| **硅基流动** | 账户余额 | API Key |

已配置 **2 个及以上**平台时，顶栏下方会出现场景 Tab，点击切换当前展示的平台；
每个场景可单独设置自动刷新间隔（5 ~ 3600 秒）。

## 功能

- 多平台余额 / 用量聚合展示，每个平台独立对接、独立展示
- 顶栏 Tab 切换场景，切换即刷新
- **每个场景独立的自动刷新间隔**，也可以点刷新按钮或点悬浮窗手动刷新
- **悬浮窗模式**：定住后一直置顶显示，不挡事、随时可见
- **启动不弹钥匙串授权**：界面上的「连接账号」按钮由你主动点击后才请求 macOS 授权并读取密钥
- 凭证只存本机、不落明文（系统钥匙串优先），不会上传

## 配置凭证（每个平台一次）

1. 打开应用 → 点右上角「设置」
2. 选择平台，按提示填入它的凭证（字段随平台不同）
3. 点「测试连接」确认可用，再点「保存并切换」

平台字段旁的指引链接可以直接跳到对应的密钥管理页，例如：
[火山引擎 IAM 密钥管理](https://console.volcengine.com/iam/keymanage)（要的是 **Access Key**，不是 API Key）。

**Trae 积分**不需要填写任何凭证：它从本机 `Trae CN` 桌面端的登录态文件里现场解密取 token，
所以使用前需要本机安装并登录过 Trae CN。

凭证存放在本机（不会进仓库、不会上传）：

| 平台 | 存放位置 |
|---|---|
| macOS | 系统钥匙串（service `com.tangsanlang.floating`）；钥匙串不可用时退回 `~/Library/Application Support/com.tangsanlang.floating/credentials.json` |
| Windows | Windows 凭据管理器（service `com.tangsanlang.floating`）；不可用时退回 `%APPDATA%\com.tangsanlang.floating\credentials.json` |

### 也支持 .env / 环境变量（开发或高级用法）

在项目根目录放一份 `.env`：

```env
VOLC_ACCESSKEY=你的 Access Key
VOLC_SECRETKEY=你的 Secret Key
```

也接受别名 `VOLC_AK` / `VOLC_SK`。

**优先级：应用内保存的凭证 > `.env` / 环境变量。**

## 下载安装

到 [Releases](../../releases) 页面下载对应平台的安装包：

- **Windows**：`*_x64-setup.exe`
- **macOS**：`*.dmg`（通用包，同时支持 Intel 与 Apple 芯片）

## 开发 / 构建

```bash
npm install          # 安装 Tauri CLI
npm run tauri:dev    # 开发调试
npm run tauri:build  # 打包本机安装包
```

技术栈：Tauri 2（Rust 后端 + 原生 HTML/CSS/JS 前端），界面资源在编译期内嵌，改 UI 后需重新编译。

## 说明

- 火山方舟用量通过 `GetAFPUsage` 接口获取（V4 签名，区域 `cn-beijing`）；其余平台走各自的余额 / 额度接口。
- Trae 积分通过本机登录态解密后调用 IDE 私有接口获取，官方改版可能失效。
- 本工具仅做只读查询，不修改任何账号配置。
