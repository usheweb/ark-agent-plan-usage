# ark-agent-plan-usage

**火山方舟 Agent Plan 余量查看器** —— 一个轻量的桌面小工具，实时显示你的**火山方舟 Agent Plan 剩余用量**。

支持**悬浮窗显示**，定住后窗口置顶、常驻桌面，随手瞄一眼就知道还剩多少量，很方便。
**只需要配置一次方舟的 Key**（Access Key / Secret Key）即可使用。

---

## 功能

- 实时显示火山方舟 Agent Plan 余量
- **每 60 秒自动刷新**，也可以点按钮手动刷新
- **悬浮窗模式**：定住后一直置顶显示，不挡事、随时可见
- 只需配置一次方舟 Key，之后打开即用
- 只在本地读取用量，不会上传你的凭证

## 配置方舟 Key（只需一次）

本应用**只需要配置火山方舟的 Access Key**，无需改任何文件。

1. 打开应用 → 点右上角「设置」
2. 填入 **Access Key** 与 **Secret Key**
3. 点「测试连接」确认可用，再点「保存」

> Key 从哪里拿：[火山引擎 IAM 密钥管理](https://console.volcengine.com/iam/keymanage)
>
> 注意：要的是 **Access Key**，不是 API Key。

保存后凭证存放在本机（不会进仓库、不会上传）：

| 平台 | 路径 |
|---|---|
| macOS | `~/Library/Application Support/com.usheai.afpusage/credentials.json` |
| Windows | `%APPDATA%\com.usheai.afpusage\credentials.json` |

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
- **macOS**：`*.dmg`

## 开发 / 构建

```bash
npm install          # 安装 Tauri CLI
npm run tauri:dev    # 开发调试
npm run tauri:build  # 打包本机安装包
```

技术栈：Tauri 2（Rust 后端 + 原生 HTML/CSS/JS 前端），界面资源在编译期内嵌，改 UI 后需重新编译。

## 说明

- 用量通过火山引擎 `GetAFPUsage` 接口获取（V4 签名，区域 `cn-beijing`）。
- 本工具仅做只读查询，不修改任何账号配置。
