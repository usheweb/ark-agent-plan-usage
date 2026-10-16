// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use tauri::{Emitter, Manager};

const HOST: &str = "ark.cn-beijing.volcengineapi.com";
const REGION: &str = "cn-beijing";
const SERVICE: &str = "ark";
const VERSION: &str = "2024-01-01";
const ACTION: &str = "GetAFPUsage";

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

/// 候选的 .env 路径，按优先级查找：环境变量 -> 当前目录 -> 可执行文件旁 -> 源码工程目录。
fn env_file_candidates() -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("AFP_ENV_FILE") {
        if !p.trim().is_empty() {
            list.push(PathBuf::from(p));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        list.push(cwd.join(".env"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            list.push(dir.join(".env"));
            // .app 包内: Contents/MacOS/ -> 上溯三级即工程根目录
            if let Some(root) = dir.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
                list.push(root.join(".env"));
            }
        }
    }
    // 编译期工程根目录（src-tauri 的上级）
    list.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env")));
    list
}

fn parse_env_file(path: &PathBuf) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return map,
    };
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let (k, v) = line.split_once('=').unwrap();
        let key = k.trim().to_string();
        let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
        if !key.is_empty() {
            map.insert(key, val);
        }
    }
    map
}

/// 应用配置目录（macOS 下为 ~/Library/Application Support/<identifier>/）
fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("获取配置目录失败: {}", e))
}

/// 凭证文件路径
fn credentials_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("credentials.json"))
}

/// 偏好设置文件路径（刷新间隔等，不含任何密钥）
fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("settings.json"))
}

/* ---------- 数据源注册表 ---------- */

/// 一个数据源需要的单个凭证字段（前端据此动态渲染表单，不用为每个平台手写设置页）
struct FieldDef {
    key: &'static str,
    label: &'static str,
    secret: bool,
    placeholder: &'static str,
}

/// 一个数据源：标识、展示名、凭证字段、密钥页地址
struct SourceDef {
    id: &'static str,
    name: &'static str,
    /// 取值口径（写在前端副标题上，如「账户余额」「套餐额度」）
    subtitle: &'static str,
    help_url: &'static str,
    help_text: &'static str,
    fields: &'static [FieldDef],
    /// 凭证由程序自动获取（用户无法填写）：前端隐藏录入表单，只展示只读检测结果。
    /// 目前仅 trae 使用——它从本机 Trae CN 登录态里现场解密，不写钥匙串、不落盘。
    auto_creds: bool,
}

const VOLC_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "accessKey",
        label: "Access Key",
        secret: false,
        placeholder: "AKLT…",
    },
    FieldDef {
        key: "secretKey",
        label: "Secret Key",
        secret: true,
        placeholder: "请填写 Secret Key",
    },
];

const BEARER_FIELDS: &[FieldDef] = &[FieldDef {
    key: "apiKey",
    label: "API Key",
    secret: true,
    placeholder: "sk-…",
}];

const SOURCES: &[SourceDef] = &[
    SourceDef {
        id: "trae",
        name: "Trae 积分",
        subtitle: "Code 积分额度",
        help_url: "",
        help_text: "",
        // 无用户可填字段：凭证从本机 Trae CN 登录态自动解密，见 trae_probe
        fields: &[],
        auto_creds: true,
    },
    SourceDef {
        id: "volcengine",
        name: "火山方舟",
        subtitle: "Agent Plan 套餐额度",
        help_url: "https://console.volcengine.com/iam/keymanage",
        help_text: "打开火山引擎密钥管理页",
        fields: VOLC_FIELDS,
        auto_creds: false,
    },
    SourceDef {
        id: "deepseek",
        name: "DeepSeek",
        subtitle: "账户余额",
        help_url: "https://platform.deepseek.com/api_keys",
        help_text: "打开 DeepSeek 密钥页",
        fields: BEARER_FIELDS,
        auto_creds: false,
    },
    SourceDef {
        id: "kimi",
        name: "Kimi 月之暗面",
        subtitle: "账户余额",
        help_url: "https://platform.moonshot.cn/console/api-keys",
        help_text: "打开 Kimi 密钥页",
        fields: BEARER_FIELDS,
        auto_creds: false,
    },
    SourceDef {
        id: "siliconflow",
        name: "硅基流动",
        subtitle: "账户余额",
        help_url: "https://cloud.siliconflow.cn/account/ak",
        help_text: "打开硅基流动密钥页",
        fields: BEARER_FIELDS,
        auto_creds: false,
    },
];

fn source_def(id: &str) -> Option<&'static SourceDef> {
    SOURCES.iter().find(|s| s.id == id)
}

/* ---------- 数据源：Trae Code 积分（自动读取本机登录态，无用户可填凭证） ---------- */

/// Trae CN 登录态文件里两个存储键：加密的登录态 + 明文的套餐信息
const TRAE_AUTH_KEY: &str = "iCubeAuthInfo://icube.cloudide";
const TRAE_SERVER_KEY: &str = "iCubeServerData://icube.cloudide";
/// 自研加密格式的 magic 头（"tc" + 版本）
const TRAE_MAGIC: [u8; 6] = [0x74, 0x63, 0x05, 0x10, 0x00, 0x00];
const TRAE_SALT_LEN: usize = 32;
const TRAE_CHECKSUM_LEN: usize = 64;
/// 密钥派生用的固定 64 字节混淆表（= byteCrypto.js 里 Sle ^ ble），hex 表示
const TRAE_PAD_HEX: &str = "4dd4c2e6b83162090e52b3c7a6733ba41cb2462b829ab58a196b39db57177524f49baf7f08e8d68d26a72e37c1a95a2f1f05a51892aef2949732b62a38aadd58";

/// Trae CN 登录态文件的候选路径（按平台取）。Electron 应用的用户目录在不同系统下位置不同：
/// - macOS：`$HOME/Library/Application Support/Trae CN/User/globalStorage/storage.json`
/// - Windows：`%APPDATA%\Trae CN\User\globalStorage\storage.json`（Windows 的「AppData\Roaming」）
fn trae_storage_candidates() -> Vec<PathBuf> {
    const REL: &str = "Trae CN/User/globalStorage/storage.json";
    let mut list: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                list.push(PathBuf::from(appdata).join(REL));
            }
        }
        // APPDATA 拿不到时，回退到 %USERPROFILE%\AppData\Roaming
        if let Ok(profile) = std::env::var("USERPROFILE") {
            if !profile.trim().is_empty() {
                list.push(PathBuf::from(profile).join("AppData/Roaming").join(REL));
            }
        }
    } else {
        let home = std::env::var("HOME").unwrap_or_default();
        list.push(PathBuf::from(home).join("Library/Application Support").join(REL));
    }
    list
}

/// Trae CN 登录态文件路径：优先取第一个「真实存在」的候选，都不存在时回退到主候选
fn trae_storage_path() -> PathBuf {
    let candidates = trae_storage_candidates();
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates.into_iter().next().unwrap_or_default())
}

/// 读取并解析 Trae CN 登录态文件
fn trae_read_storage() -> Result<serde_json::Value, String> {
    let path = trae_storage_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|_| "未找到 Trae CN 登录态文件，请先登录 Trae CN 桌面端".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("Trae CN 登录态文件解析失败: {}", e))
}

/// KDF：SHA512(salt) 拼固定 64 字节混淆表 -> SHA512；取前 16 字节为 key，其后 16 字节为 iv
fn trae_derive_key_iv(salt: &[u8]) -> Result<([u8; 16], [u8; 16]), String> {
    let h1 = Sha512::digest(salt);
    let pad = hex::decode(TRAE_PAD_HEX).map_err(|e| format!("PAD 常量解析失败: {}", e))?;
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(&h1);
    buf.extend_from_slice(&pad);
    let h2 = Sha512::digest(&buf);
    let mut key = [0u8; 16];
    let mut iv = [0u8; 16];
    key.copy_from_slice(&h2[0..16]);
    iv.copy_from_slice(&h2[16..32]);
    Ok((key, iv))
}

/// 解密登录态：base64 -> AES-128-CBC(PKCS7) -> SHA-512 校验 -> UTF-8 明文
fn trae_decrypt_blob(b64: &str) -> Result<String, String> {
    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    use base64::Engine;

    let blob = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("登录态 base64 解码失败: {}", e))?;
    let body_off = TRAE_MAGIC.len() + TRAE_SALT_LEN;
    if blob.len() <= body_off || blob[..TRAE_MAGIC.len()] != TRAE_MAGIC {
        return Err("登录态存储格式未知".to_string());
    }
    let (key, iv) = trae_derive_key_iv(&blob[TRAE_MAGIC.len()..body_off])?;

    let mut buf = blob[body_off..].to_vec();
    let plain = cbc::Decryptor::<aes::Aes128>::new_from_slices(&key, &iv)
        .map_err(|e| format!("AES 初始化失败: {}", e))?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| "AES 解密失败（长度或填充异常）".to_string())?
        .to_vec();

    if plain.len() <= TRAE_CHECKSUM_LEN {
        return Err("解密数据过短".to_string());
    }
    if Sha512::digest(&plain[TRAE_CHECKSUM_LEN..]).as_slice() != &plain[..TRAE_CHECKSUM_LEN] {
        return Err("登录态 SHA-512 校验失败".to_string());
    }
    String::from_utf8(plain[TRAE_CHECKSUM_LEN..].to_vec())
        .map_err(|e| format!("登录态明文不是合法 UTF-8: {}", e))
}

/// 解密并解析出登录态 JSON（含 token / expiredAt / refreshToken / account）
fn trae_auth_json(storage: &serde_json::Value) -> Result<serde_json::Value, String> {
    let b64 = storage
        .get(TRAE_AUTH_KEY)
        .and_then(|v| v.as_str())
        .ok_or_else(|| "登录态文件里没有登录信息（请先在 Trae CN 里登录）".to_string())?;
    serde_json::from_str(&trae_decrypt_blob(b64)?)
        .map_err(|e| format!("登录态 JSON 解析失败: {}", e))
}

/// 从明文键里取套餐身份（如 Ultra），取不到返回空串
fn trae_identity(storage: &serde_json::Value) -> String {
    storage
        .get(TRAE_SERVER_KEY)
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| {
            v.get("entitlementInfo")
                .and_then(|e| e.get("identityStr"))
                .and_then(|s| s.as_str())
                .map(|x| x.to_string())
        })
        .unwrap_or_default()
}

/// 登录态是否已过期（expiredAt 解析不出时不拦）
fn trae_expired(expired_at: &str) -> bool {
    if expired_at.trim().is_empty() {
        return false;
    }
    match chrono::DateTime::parse_from_rfc3339(expired_at.trim()) {
        Ok(dt) => dt.with_timezone(&chrono::Utc) < chrono::Utc::now(),
        Err(_) => false,
    }
}

/// 探测结果：文件路径、套餐身份、过期时间、token、不可用原因（None 表示可用）
struct TraeProbe {
    storage_path: String,
    identity: String,
    expired_at: String,
    token: Option<String>,
    unavailable: Option<String>,
}

/// 现场读取本机 Trae CN 登录态并解密。不写钥匙串、不落盘，也不打印 token。
fn trae_probe() -> TraeProbe {
    let path = trae_storage_path();
    let mut probe = TraeProbe {
        storage_path: path.display().to_string(),
        identity: String::new(),
        expired_at: String::new(),
        token: None,
        unavailable: None,
    };

    let storage = match trae_read_storage() {
        Ok(v) => v,
        Err(e) => {
            probe.unavailable = Some(e);
            return probe;
        }
    };
    probe.identity = trae_identity(&storage);

    let auth = match trae_auth_json(&storage) {
        Ok(v) => v,
        Err(e) => {
            probe.unavailable = Some(format!("Trae CN 登录态不可用：{}", e));
            return probe;
        }
    };

    let token = auth.get("token").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let expired = auth.get("expiredAt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    probe.expired_at = expired.clone();

    if token.is_empty() {
        probe.unavailable =
            Some("Trae CN 登录态里没有有效的 token，请在 Trae CN 里重新登录".to_string());
        return probe;
    }
    if trae_expired(&expired) {
        probe.unavailable = Some("Trae 登录态已过期，请在 Trae CN 里重新登录".to_string());
        return probe;
    }
    probe.token = Some(token);
    probe
}

/// 取数：POST IDE 同款积分接口，返回原始响应（前端自行聚合）
async fn fetch_trae_credits(token: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .post("https://api.trae.cn/trae/api/v2/pay/ide_user_ent_usage")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Cloud-IDE-JWT {}", token))
        .json(&serde_json::json!({ "require_usage": true, "req_source": 2 }))
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("接口返回 HTTP {}: {}", status.as_u16(), text));
    }
    serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {}", e))
}

/// 组装 trae 给前端的 data：原始响应 + 登录态元信息（套餐身份 / 过期时间 / 文件路径）
fn trae_payload(raw: serde_json::Value, probe: &TraeProbe) -> serde_json::Value {
    serde_json::json!({
        "raw": raw,
        "identity": probe.identity,
        "expiredAt": probe.expired_at,
        "file": probe.storage_path,
    })
}

/* ---------- 凭证存取 ---------- */

/// 凭证文件（v2）：按数据源 id 索引，`active` 记录当前展示的场景
///
/// ```json
/// { "version": 2, "active": "volcengine",
///   "sources": { "volcengine": { "accessKey": "…", "secretKey": "…" },
///                "deepseek": { "apiKey": "…" } } }
/// ```
///
/// v1 是单平台平铺结构 `{ "accessKey": …, "secretKey": … }`。读取时若命中 v1，
/// 会自动迁移成上面这份（归到 sources.volcengine）并落盘一次，
/// 保证老用户升级后已存的凭证不丢（对应待办 M-09）。
fn read_store(app: &tauri::AppHandle) -> serde_json::Value {
    let Ok(path) = credentials_path(app) else {
        return serde_json::json!({ "version": 2, "sources": {} });
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return serde_json::json!({ "version": 2, "sources": {} });
    };
    let mut store: serde_json::Value =
        serde_json::from_str(&text).unwrap_or(serde_json::json!({}));

    // v1 迁移：仅当存在平铺的 accessKey / secretKey 且还没有 sources 时执行
    let legacy_ak = store
        .get("accessKey")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let legacy_sk = store
        .get("secretKey")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let migrated = store.get("sources").is_none();

    if migrated {
        let mut sources = serde_json::Map::new();
        if !legacy_ak.is_empty() && !legacy_sk.is_empty() {
            sources.insert(
                "volcengine".to_string(),
                serde_json::json!({ "accessKey": legacy_ak, "secretKey": legacy_sk }),
            );
        }
        store = serde_json::json!({
            "version": 2,
            "active": "volcengine",
            "sources": serde_json::Value::Object(sources),
        });
        let _ = write_store(app, &store);
    }
    if store.get("sources").is_none() {
        store["sources"] = serde_json::json!({});
    }
    store
}

/// 写入 JSON 文件，并把文件权限收紧为 600（仅当前用户可读）
fn write_private_json(path: &PathBuf, value: &serde_json::Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("写入配置失败: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// 写入凭证文件
fn write_store(app: &tauri::AppHandle, store: &serde_json::Value) -> Result<(), String> {
    write_private_json(&credentials_path(app)?, store)
}

/* ---------- 偏好设置（每场景独立刷新间隔，E-02） ---------- */

/// 默认刷新间隔（秒），也是未设置过该场景时的取值
const DEFAULT_REFRESH_SECS: u64 = 60;
const MIN_REFRESH_SECS: u64 = 5;
const MAX_REFRESH_SECS: u64 = 3600;

/// 读取偏好设置：`{ "refresh": { "<sourceId>": 60 } }`
fn read_settings(app: &tauri::AppHandle) -> serde_json::Value {
    let Ok(path) = settings_path(app) else {
        return serde_json::json!({ "refresh": {} });
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return serde_json::json!({ "refresh": {} });
    };
    let mut store: serde_json::Value =
        serde_json::from_str(&text).unwrap_or(serde_json::json!({}));
    // 文件被手工改坏（不是对象 / refresh 不是对象）时重置，避免下面索引写入时 panic
    if !store.is_object() {
        store = serde_json::json!({});
    }
    if !store.get("refresh").map(|v| v.is_object()).unwrap_or(false) {
        store["refresh"] = serde_json::json!({});
    }
    store
}

fn write_settings(app: &tauri::AppHandle, store: &serde_json::Value) -> Result<(), String> {
    write_private_json(&settings_path(app)?, store)
}

/* ---------- 凭证读写：系统钥匙串优先（E-01），不可用时退回凭证文件 ---------- */

/// 钥匙串里的服务名；账号用数据源 id（如 volcengine），每条密码存该平台凭证的 JSON
const KEYRING_SERVICE: &str = "com.tangsanlang.floating";

/// 改 identifier 之前用过的钥匙串服务名，解锁时把条目一次性搬到新服务名下
const LEGACY_KEYRING_SERVICE: &str = "com.usheai.afpusage";

/* ---------- 会话级解锁闸门（按数据源懒加载） ---------- */

/// 本次运行内已解锁（可读钥匙串）的数据源 id 列表。默认空，**不写盘**。
///
/// 启动阶段列表为空，因此所有读钥匙串的函数都直接返回空、绝不调用 keyring，不会弹授权框；
/// 用户**第一次**切到需要凭证的 tab（或点「连接账号」）时，`unlock_all_sources` 会把**全部**
/// 有凭证的数据源一次性读进内存并加入这里，此后切换任何 tab 都命中已解锁状态、不再重复弹框。
/// 解锁与否只对本次运行有效，下次启动仍是「首次触发才读」。
static UNLOCKED_SOURCES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// 该数据源本次运行是否已解锁（可读钥匙串）
fn source_unlocked(id: &str) -> bool {
    UNLOCKED_SOURCES
        .lock()
        .map(|v| v.iter().any(|x| x == id))
        .unwrap_or(false)
}

/// 置位 / 清除某数据源的会话解锁状态（仅本次运行有效）
fn set_source_unlocked(id: &str, on: bool) {
    if let Ok(mut v) = UNLOCKED_SOURCES.lock() {
        let present = v.iter().any(|x| x == id);
        if on && !present {
            v.push(id.to_string());
        } else if !on && present {
            v.retain(|x| x != id);
        }
    }
}

/// 本次运行是否有任意数据源已解锁（用于判断钥匙串存储方式是否需真实探测）
fn any_source_unlocked() -> bool {
    UNLOCKED_SOURCES
        .lock()
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// 钥匙串是否可用：进程内只探测一次，结果缓存下来
static KEYCHAIN_READY: Mutex<Option<bool>> = Mutex::new(None);

fn keychain_ready() -> bool {
    if let Some(ready) = KEYCHAIN_READY.lock().ok().and_then(|slot| *slot) {
        return ready;
    }
    let ready = keyring::Entry::store_status().is_ok();
    if let Ok(mut slot) = KEYCHAIN_READY.lock() {
        *slot = Some(ready);
    }
    ready
}

fn keychain_entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, id).map_err(|e| format!("打开系统钥匙串失败: {}", e))
}

/// 读钥匙串里某个平台的凭证；没有条目或钥匙串不可用时返回 None。
/// 该源未解锁时直接返回 None，绝不调用 keyring（避免启动 / 未连接时弹出授权框）。
fn read_keychain_creds(id: &str) -> Option<HashMap<String, String>> {
    if !source_unlocked(id) {
        return None;
    }
    let entry = keychain_entry(id).ok()?;
    let secret = entry.get_password().ok()?;
    let map: HashMap<String, String> = serde_json::from_str(&secret).ok()?;
    Some(clean_creds(map))
}

fn clean_creds(map: HashMap<String, String>) -> HashMap<String, String> {
    map.into_iter()
        .filter_map(|(k, v)| {
            let v = v.trim().to_string();
            if v.is_empty() {
                None
            } else {
                Some((k, v))
            }
        })
        .collect()
}

/// 读凭证文件里该平台的明文凭证（历史数据，或钥匙串不可用时的兜底）
fn read_plaintext_creds(app: &tauri::AppHandle, id: &str) -> HashMap<String, String> {
    let store = read_store(app);
    let mut out = HashMap::new();
    if let Some(obj) = store
        .get("sources")
        .and_then(|s| s.get(id))
        .and_then(|v| v.as_object())
    {
        for (k, val) in obj {
            if let Some(s) = val.as_str().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                out.insert(k.clone(), s.to_string());
            }
        }
    }
    out
}

/// 写入明文凭证到凭证文件（仅钥匙串不可用时使用）
fn write_plaintext_creds(
    app: &tauri::AppHandle,
    id: &str,
    creds: &HashMap<String, String>,
) -> Result<(), String> {
    let mut store = read_store(app);
    store["version"] = serde_json::json!(2);
    store["sources"][id] = serde_json::to_value(creds).map_err(|e| e.to_string())?;
    write_store(app, &store)
}

/// 从凭证文件里抹掉某个平台的明文凭证
fn remove_plaintext_creds(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let mut store = read_store(app);
    let removed = store
        .get_mut("sources")
        .and_then(|s| s.as_object_mut())
        .map(|sources| sources.remove(id).is_some())
        .unwrap_or(false);
    if !removed {
        return Ok(());
    }
    write_store(app, &store)
}

/* ---------- 明文标记：哪些平台已保存过凭证（不含任何密钥值） ---------- */

/// 读取「已保存凭证的平台 id 列表」标记（存于凭证文件，仅 id、不含密钥）。
/// 用于在锁定态不读钥匙串时也能判断某平台「是否已保存凭证」。
fn saved_platform_ids(app: &tauri::AppHandle) -> Vec<String> {
    read_store(app)
        .get("savedIds")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 整体重写「已保存凭证的平台 id」标记列表
fn write_saved_ids(app: &tauri::AppHandle, ids: &[String]) -> Result<(), String> {
    let mut store = read_store(app);
    store["savedIds"] = serde_json::json!(ids);
    write_store(app, &store)
}

/// 增 / 删单个平台 id（幂等；只是标记，不含任何凭证值）
fn mark_saved(app: &tauri::AppHandle, id: &str, saved: bool) -> Result<(), String> {
    let mut ids = saved_platform_ids(app);
    let present = ids.iter().any(|x| x == id);
    if saved && !present {
        ids.push(id.to_string());
    } else if !saved && present {
        ids.retain(|x| x != id);
    } else {
        return Ok(());
    }
    write_saved_ids(app, &ids)
}

/// 该平台是否已保存过凭证：标记里有该 id，或凭证文件里仍有该平台明文
fn has_saved_creds(app: &tauri::AppHandle, id: &str) -> bool {
    saved_platform_ids(app).iter().any(|x| x == id) || !read_plaintext_creds(app, id).is_empty()
}

/* ---------- 一次性迁移标记（避免每次解锁重跑） ---------- */

/// 某一次性迁移是否已执行过（存于凭证文件，参考 version 字段的写法）
fn migration_done(app: &tauri::AppHandle, key: &str) -> bool {
    read_store(app)
        .get("migrations")
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// 标记某一次性迁移已执行
fn mark_migration_done(app: &tauri::AppHandle, key: &str) {
    let mut store = read_store(app);
    if !store.get("migrations").map(|v| v.is_object()).unwrap_or(false) {
        store["migrations"] = serde_json::json!({});
    }
    store["migrations"][key] = serde_json::json!(true);
    let _ = write_store(app, &store);
}

/// 读某平台凭证：钥匙串优先，取不到再退回凭证文件。
/// 该源未解锁时直接返回空，钥匙串与明文都不读（「先连接再加载」）。
fn read_source_creds(app: &tauri::AppHandle, id: &str) -> HashMap<String, String> {
    if !source_unlocked(id) {
        return HashMap::new();
    }
    if keychain_ready() {
        if let Some(creds) = read_keychain_creds(id) {
            return creds;
        }
    }
    read_plaintext_creds(app, id)
}

/// 写某平台凭证：钥匙串可用则写钥匙串，并抹掉文件里的同一份明文
fn write_source_creds(
    app: &tauri::AppHandle,
    id: &str,
    creds: &HashMap<String, String>,
) -> Result<(), String> {
    if keychain_ready() {
        let entry = keychain_entry(id)?;
        let blob = serde_json::to_string(creds).map_err(|e| e.to_string())?;
        entry
            .set_password(&blob)
            .map_err(|e| format!("写入系统钥匙串失败: {}", e))?;
        // 同一份密钥不留两处：钥匙串写成功即清掉文件里的明文
        let _ = remove_plaintext_creds(app, id);
        return Ok(());
    }
    write_plaintext_creds(app, id, creds)
}

/// 删除某平台凭证：钥匙串条目与文件明文都清掉
fn delete_source_creds(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let mut failure: Option<String> = None;
    if keychain_ready() {
        if let Ok(entry) = keychain_entry(id) {
            match entry.delete_credential() {
                // 本来就没有这条，也算清干净了
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => failure = Some(format!("删除钥匙串凭证失败: {}", e)),
            }
        }
    }
    let _ = remove_plaintext_creds(app, id);
    match failure {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// 一次性迁移：把凭证文件里某平台的历史明文搬进系统钥匙串，成功后抹掉文件里的那份。
/// **只在解锁该源时调用**（启动不执行），且只处理这一个平台，不触碰其他平台的钥匙串。
///
/// 钥匙串里已有该平台条目时跳过（避免用旧明文覆盖），那份明文会留到用户下次
/// 保存凭证时才清掉 —— 宁可多留一份，也不冒丢掉密钥的风险。
fn migrate_source_to_keychain(app: &tauri::AppHandle, id: &str) {
    let key = format!("plaintext:{}", id);
    if migration_done(app, &key) || !keychain_ready() {
        return;
    }
    let creds = clean_creds(read_plaintext_creds(app, id));
    if !creds.is_empty() && read_keychain_creds(id).is_none() {
        if let Ok(entry) = keychain_entry(id) {
            if let Ok(blob) = serde_json::to_string(&creds) {
                if entry.set_password(&blob).is_ok() {
                    let _ = remove_plaintext_creds(app, id);
                }
            }
        }
    }
    mark_migration_done(app, &key);
}

/// 一次性迁移：把旧服务名（改 identifier 前）下某平台的钥匙串条目搬到新服务名下。
/// **只在解锁该源时调用**（启动不执行），且只处理这一个平台，不触碰其他平台的钥匙串。
///
/// 新服务名下已有同平台条目时跳过（不覆盖），旧条目读完确认新条目写成功后才删。
fn migrate_legacy_service_for(app: &tauri::AppHandle, id: &str) {
    let key = format!("legacyService:{}", id);
    if migration_done(app, &key) || !keychain_ready() {
        return;
    }
    if let Ok(old) = keyring::Entry::new(LEGACY_KEYRING_SERVICE, id) {
        if let Ok(secret) = old.get_password() {
            if let Ok(new) = keychain_entry(id) {
                if new.get_password().is_err() && new.set_password(&secret).is_ok() {
                    let _ = old.delete_credential();
                }
            }
        }
    }
    mark_migration_done(app, &key);
}

/// 该数据源的必需字段是否都填了
fn creds_complete(def: &SourceDef, creds: &HashMap<String, String>) -> bool {
    def.fields.iter().all(|f| creds.contains_key(f.key))
}

/// 取当前生效的凭证：应用内设置优先，火山方舟再兜底 .env / 环境变量。
/// 该源未解锁时直接返回 None —— 既不读钥匙串，也跳过 .env 兜底（否则方舟会在未连接时照常出数）。
fn effective_creds(
    app: &tauri::AppHandle,
    def: &SourceDef,
) -> Option<(HashMap<String, String>, &'static str)> {
    if !source_unlocked(def.id) {
        return None;
    }
    let saved = read_source_creds(app, def.id);
    if creds_complete(def, &saved) {
        return Some((saved, "settings"));
    }
    if def.id == "volcengine" {
        if let Some((ak, sk)) = load_env_credentials() {
            let mut m = HashMap::new();
            m.insert("accessKey".to_string(), ak);
            m.insert("secretKey".to_string(), sk);
            return Some((m, "env"));
        }
    }
    None
}

/// 读取当前场景 id（未设置时取注册表第一个）
fn active_source_id(app: &tauri::AppHandle) -> String {
    read_store(app)
        .get("active")
        .and_then(|v| v.as_str())
        .filter(|s| source_def(s).is_some())
        .unwrap_or(SOURCES[0].id)
        .to_string()
}

/// 兜底来源：环境变量 / .env（仅方便本地开发，不参与打包发布）
fn load_env_credentials() -> Option<(String, String)> {
    let mut file_vars: HashMap<String, String> = HashMap::new();
    for path in env_file_candidates() {
        let vars = parse_env_file(&path);
        if !vars.is_empty() {
            file_vars = vars;
            break;
        }
    }

    let pick = |name: &str, alias: &str| -> Option<String> {
        std::env::var(name)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| std::env::var(alias).ok().filter(|v| !v.trim().is_empty()))
            .or_else(|| {
                file_vars
                    .get(name)
                    .or_else(|| file_vars.get(alias))
                    .filter(|v| !v.trim().is_empty())
                    .cloned()
            })
    };

    let ak = pick("VOLC_ACCESSKEY", "VOLC_AK")?;
    let sk = pick("VOLC_SECRETKEY", "VOLC_SK")?;
    Some((ak, sk))
}

/* ---------- 取数：每个平台独立实现，统一返回该平台的原始响应 ---------- */

/// 带 Bearer 鉴权的 GET，返回原始 JSON（各平台返回结构不同，由前端各自解析）
async fn http_get_json(url: &str, api_key: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("接口返回 HTTP {}: {}", status.as_u16(), text));
    }
    serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {}", e))
}

fn require(creds: &HashMap<String, String>, key: &str, label: &str) -> Result<String, String> {
    creds
        .get(key)
        .cloned()
        .ok_or_else(|| format!("缺少 {}", label))
}

/// 按数据源 id 取数。返回该平台原始的响应体，字段结构各异。
async fn fetch_source_raw(
    source_id: &str,
    creds: &HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    match source_id {
        "volcengine" => {
            let ak = require(creds, "accessKey", "Access Key")?;
            let sk = require(creds, "secretKey", "Secret Key")?;
            fetch_afp_usage(&ak, &sk).await
        }
        "deepseek" => {
            let key = require(creds, "apiKey", "API Key")?;
            http_get_json("https://api.deepseek.com/user/balance", &key).await
        }
        "kimi" => {
            let key = require(creds, "apiKey", "API Key")?;
            http_get_json("https://api.moonshot.cn/v1/users/me/balance", &key).await
        }
        "siliconflow" => {
            let key = require(creds, "apiKey", "API Key")?;
            http_get_json("https://api.siliconflow.cn/v1/user/info", &key).await
        }
        other => Err(format!("未支持的数据源: {}", other)),
    }
}

fn build_auth_headers(ak: &str, sk: &str, x_date: &str, date_short: &str, body: &[u8]) -> String {
    let content_sha256 = sha256_hex(body);
    let signed_headers = "content-type;host;x-content-sha256;x-date";
    let canonical_headers = format!(
        "content-type:application/json\nhost:{}\nx-content-sha256:{}\nx-date:{}\n",
        HOST, content_sha256, x_date
    );
    let canonical_query = format!("Action={}&Version={}", ACTION, VERSION);
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        "POST", "/", canonical_query, canonical_headers, signed_headers, content_sha256
    );

    let credential_scope = format!("{}/{}/{}/request", date_short, REGION, SERVICE);
    let string_to_sign = format!(
        "{}\n{}\n{}\n{}",
        "HMAC-SHA256",
        x_date,
        credential_scope,
        sha256_hex(canonical_request.as_bytes())
    );

    let mut signing_key = hmac_sha256(sk.as_bytes(), date_short.as_bytes());
    signing_key = hmac_sha256(&signing_key, REGION.as_bytes());
    signing_key = hmac_sha256(&signing_key, SERVICE.as_bytes());
    signing_key = hmac_sha256(&signing_key, b"request");
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    format!(
        "HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        ak, credential_scope, signed_headers, signature
    )
}

async fn fetch_afp_usage(ak: &str, sk: &str) -> Result<serde_json::Value, String> {
    let now = chrono::Utc::now();
    let x_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_short = now.format("%Y%m%d").to_string();
    let body: Vec<u8> = b"{}".to_vec();

    let authorization = build_auth_headers(ak, sk, &x_date, &date_short, &body);
    let content_sha256 = sha256_hex(&body);

    let url = format!("https://{}/?Action={}&Version={}", HOST, ACTION, VERSION);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-Date", x_date)
        .header("X-Content-Sha256", content_sha256)
        .header("Authorization", authorization)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("接口返回 HTTP {}: {}", status.as_u16(), text));
    }

    let payload: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {}", e))?;

    match payload.get("Result") {
        Some(result) => Ok(result.clone()),
        None => Err(format!("返回异常: {}", text)),
    }
}

/// 平台清单：供设置弹窗渲染「平台列表 + 各平台字段」，并回填已保存的值。
/// **不触碰钥匙串**：已保存与否只看明文标记，凭证值仅在该源解锁后才回填。
#[tauri::command]
fn list_sources(app: tauri::AppHandle) -> serde_json::Value {
    let any_unlocked = any_source_unlocked();
    let path = credentials_path(&app)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let settings = read_settings(&app);
    let interval_of = |id: &str| -> u64 {
        settings
            .get("refresh")
            .and_then(|r| r.get(id))
            .and_then(|v| v.as_u64())
            .map(|s| s.clamp(MIN_REFRESH_SECS, MAX_REFRESH_SECS))
            .unwrap_or(DEFAULT_REFRESH_SECS)
    };

    let items: Vec<serde_json::Value> = SOURCES
        .iter()
        .map(|def| {
            // auto_creds 的源没有可保存的凭证，也不去读钥匙串 / 凭证文件（避免无谓的钥匙串访问）
            let has_creds = if def.auto_creds {
                false
            } else {
                has_saved_creds(&app, def.id)
            };
            // 该源本次运行是否已解锁（可读钥匙串）
            let src_unlocked = !def.auto_creds && source_unlocked(def.id);
            // 已保存的值：该源解锁后读钥匙串回填；未解锁只从明文文件读（通常为空）
            let saved = if def.auto_creds {
                HashMap::new()
            } else if src_unlocked {
                read_source_creds(&app, def.id)
            } else {
                read_plaintext_creds(&app, def.id)
            };
            // auto_creds 的源（trae）没有可保存的凭证，「已配置」= 本机登录态可读且未过期
            let auto_info = if def.auto_creds {
                let probe = trae_probe();
                Some(serde_json::json!({
                    "file": probe.storage_path,
                    "expiredAt": probe.expired_at,
                    "identity": probe.identity,
                    "available": probe.unavailable.is_none(),
                    "unavailable": probe.unavailable,
                }))
            } else {
                None
            };
            // 三态：未配置 / 已保存但未解锁 / 已解锁可用
            let (configured, ready, cred_source) = match &auto_info {
                Some(info) => {
                    let ok = info.get("available").and_then(|v| v.as_bool()).unwrap_or(false);
                    (ok, ok, "auto")
                }
                None => {
                    // configured 只看「是否保存过凭证」（与是否解锁无关，未解锁也要据此显示 tab / 列表项）
                    (
                        has_creds,
                        has_creds && src_unlocked,
                        if has_creds { "settings" } else { "none" },
                    )
                }
            };
            let fields: Vec<serde_json::Value> = def
                .fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "key": f.key,
                        "label": f.label,
                        "secret": f.secret,
                        "placeholder": f.placeholder,
                        "value": saved.get(f.key).cloned().unwrap_or_default(),
                    })
                })
                .collect();
            let mut item = serde_json::json!({
                "id": def.id,
                "name": def.name,
                "subtitle": def.subtitle,
                "helpUrl": def.help_url,
                "helpText": def.help_text,
                "configured": configured,
                "ready": ready,
                "unlocked": src_unlocked,
                "hasCreds": has_creds,
                "credentialSource": cred_source,
                "interval": interval_of(def.id),
                "fields": fields,
                "autoCreds": def.auto_creds,
            });
            if let Some(info) = auto_info {
                item["autoInfo"] = info;
            }
            item
        })
        .collect();

    serde_json::json!({
        "path": path,
        "active": active_source_id(&app),
        "unlocked": any_unlocked,
        // 未解锁任何源时不探测钥匙串可用性（避免触碰 keyring），先按钥匙串假设；解锁后回填真实值
        "storage": if any_unlocked {
            if keychain_ready() { "keychain" } else { "file" }
        } else {
            "keychain"
        },
        "defaultInterval": DEFAULT_REFRESH_SECS,
        "minInterval": MIN_REFRESH_SECS,
        "maxInterval": MAX_REFRESH_SECS,
        "sources": items,
    })
}

/// 设置某个场景的自动刷新间隔（秒），返回夹紧后的实际取值
#[tauri::command]
fn set_refresh_interval(
    app: tauri::AppHandle,
    source_id: String,
    seconds: u64,
) -> Result<u64, String> {
    source_def(&source_id).ok_or_else(|| format!("未支持的数据源: {}", source_id))?;
    let secs = seconds.clamp(MIN_REFRESH_SECS, MAX_REFRESH_SECS);

    let mut settings = read_settings(&app);
    settings["refresh"][source_id.as_str()] = serde_json::json!(secs);
    write_settings(&app, &settings)?;
    Ok(secs)
}

/// 保存某平台的凭证，并把该平台设为当前场景
#[tauri::command]
fn save_source_credentials(
    app: tauri::AppHandle,
    source_id: String,
    values: HashMap<String, String>,
) -> Result<(), String> {
    let def = source_def(&source_id).ok_or_else(|| format!("未支持的数据源: {}", source_id))?;

    if def.auto_creds {
        return Err(format!("「{}」的凭证自动读取本机登录态，无需手动填写。", def.name));
    }

    let mut clean: HashMap<String, String> = HashMap::new();
    for f in def.fields {
        let v = values.get(f.key).map(|s| s.trim().to_string()).unwrap_or_default();
        if v.is_empty() {
            return Err(format!("请填写完整的 {}", f.label));
        }
        clean.insert(f.key.to_string(), v);
    }

    write_source_creds(&app, def.id, &clean)?;

    // 刚写入的源本次运行即可读（无需再走一次「连接账号」）
    set_source_unlocked(def.id, true);

    // 凭证本身已进钥匙串 / 凭证文件，配置文件里只留「当前场景」这个非敏感标记
    let mut store = read_store(&app);
    store["active"] = serde_json::json!(def.id);
    write_store(&app, &store)?;

    // 同步明文标记（仅平台 id），供锁定态判断「已保存凭证」；写钥匙串本身不弹授权框
    mark_saved(&app, def.id, true)
}

/// 清除某平台的凭证（只影响该平台，其他平台配置保留）
#[tauri::command]
fn clear_source_credentials(app: tauri::AppHandle, source_id: String) -> Result<(), String> {
    source_def(&source_id).ok_or_else(|| format!("未支持的数据源: {}", source_id))?;
    delete_source_creds(&app, &source_id)?;
    // 该源已无凭证，收回本次运行的解锁态（再切到它的 tab 会重新按需读取）
    set_source_unlocked(&source_id, false);
    // 成功删除后从明文标记里移除该平台
    mark_saved(&app, &source_id, false)
}

/// 连接账号：把本次会话需要的钥匙串**一次性全部**读进内存。
///
/// 启动阶段不读钥匙串、不弹授权框；用户**第一次**切到某个需要凭证的 tab（或点「连接账号」）时
/// 才调用这里 —— 它随即把**所有**平台的密钥一起读出来并缓存解锁态。此后切换任何 tab 都直接用
/// 已缓存的凭证，不再重复弹授权框（「首次触发一次、之后全局免再输」）。
/// `source_id` 仅用于校验（前端始终传当前 tab），实际执行的是全量解锁。
/// 返回读到的平台：`{"unlocked": true, "platforms": ["volcengine", ...]}`。
#[tauri::command]
fn unlock_source(app: tauri::AppHandle, source_id: String) -> Result<serde_json::Value, String> {
    source_def(&source_id).ok_or_else(|| format!("未支持的数据源: {}", source_id))?;
    unlock_all_sources(&app)
}

/// 全量解锁：把所有非 auto 数据源的凭证读进内存并置位解锁态。
/// 每个源只为它自己跑一次性迁移；读不到凭证的源会收回解锁态（前端仍按「未连接」展示）。
fn unlock_all_sources(app: &tauri::AppHandle) -> Result<serde_json::Value, String> {
    let mut platforms: Vec<String> = Vec::new();
    for def in SOURCES.iter().filter(|d| !d.auto_creds) {
        // 先置位解锁，迁移与读取才能访问钥匙串（此时才可能弹授权框）
        set_source_unlocked(def.id, true);
        migrate_legacy_service_for(app, def.id);
        migrate_source_to_keychain(app, def.id);

        // 读该源在钥匙串里的凭证；有则同步「已保存」明文标记（供未解锁时判断）
        if read_source_creds(app, def.id).is_empty() {
            set_source_unlocked(def.id, false);
        } else {
            mark_saved(app, def.id, true)?;
            platforms.push(def.id.to_string());
        }
    }
    Ok(serde_json::json!({ "unlocked": true, "platforms": platforms }))
}

/// 切换当前展示的场景
#[tauri::command]
fn set_active_source(app: tauri::AppHandle, source_id: String) -> Result<(), String> {
    source_def(&source_id).ok_or_else(|| format!("未支持的数据源: {}", source_id))?;
    let mut store = read_store(&app);
    store["version"] = serde_json::json!(2);
    store["active"] = serde_json::json!(source_id);
    write_store(&app, &store)
}

/// 取当前（或指定）场景的数据
#[tauri::command]
async fn get_source_data(
    app: tauri::AppHandle,
    source_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let id = source_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| active_source_id(&app));
    let def = source_def(&id).ok_or_else(|| format!("未支持的数据源: {}", id))?;

    // 该源未解锁：钥匙串类数据源不取数，返回可被前端识别的「待授权」结构（而非红色报错）
    if !def.auto_creds && !source_unlocked(&id) {
        return Ok(serde_json::json!({
            "source": def.id,
            "name": def.name,
            "subtitle": def.subtitle,
            "state": "locked",
        }));
    }

    let data = if def.auto_creds {
        // 现场解密本机登录态取 token；失败时把具体原因（未登录 / 已过期）回给前端
        let probe = trae_probe();
        let token = match (probe.token.clone(), probe.unavailable.clone()) {
            (Some(t), _) => t,
            (None, Some(reason)) => return Err(reason),
            (None, None) => return Err("Trae CN 登录态不可用".to_string()),
        };
        let raw = fetch_trae_credits(&token).await?;
        trae_payload(raw, &probe)
    } else {
        let (creds, _) = effective_creds(&app, def).ok_or_else(|| {
            format!("尚未配置「{}」的凭证，请点击右上角「设置」填写。", def.name)
        })?;
        fetch_source_raw(def.id, &creds).await?
    };

    Ok(serde_json::json!({
        "source": def.id,
        "name": def.name,
        "subtitle": def.subtitle,
        "data": data,
    }))
}

/// 用尚未保存的凭证试调一次接口，用于「测试连接」
#[tauri::command]
async fn test_source(
    source_id: String,
    values: HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    let def = source_def(&source_id).ok_or_else(|| format!("未支持的数据源: {}", source_id))?;

    if def.auto_creds {
        // 无用户字段：直接现场解密登录态试调
        let probe = trae_probe();
        let token = match (probe.token.clone(), probe.unavailable.clone()) {
            (Some(t), _) => t,
            (None, Some(reason)) => return Err(reason),
            (None, None) => return Err("Trae CN 登录态不可用".to_string()),
        };
        let raw = fetch_trae_credits(&token).await?;
        return Ok(serde_json::json!({ "source": def.id, "data": trae_payload(raw, &probe) }));
    }

    let mut creds: HashMap<String, String> = HashMap::new();
    for f in def.fields {
        match values
            .get(f.key)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            Some(v) => {
                creds.insert(f.key.to_string(), v);
            }
            None => return Err(format!("请先填写 {}", f.label)),
        }
    }

    let data = fetch_source_raw(def.id, &creds).await?;
    Ok(serde_json::json!({ "source": def.id, "data": data }))
}

/// 用系统默认浏览器打开链接（设置弹窗里的「密钥管理页」）
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("仅支持 http(s) 链接".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("打开链接失败: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("打开链接失败: {}", e))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("打开链接失败: {}", e))?;
    }
    Ok(())
}

/// Windows 专有：在「定住」悬浮模式下拦截最小化，避免 Win+D（显示桌面）/
/// Win+M（最小化所有窗口）把置顶悬浮窗一起隐藏。
///
/// 直接声明 user32 的少量 API，不引入 windows crate，避免与其版本 / feature 组合
/// 产生类型或链接上的差异。
#[cfg(target_os = "windows")]
mod float_guard {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    const GWLP_WNDPROC: i32 = -4;
    const WM_SIZE: u32 = 0x0005;
    const WM_SYSCOMMAND: u32 = 0x0112;
    const SC_MINIMIZE: usize = 0xF020;
    const SIZE_MINIMIZED: usize = 1;
    const SW_RESTORE: i32 = 9;

    type Hwnd = *mut c_void;
    type Lresult = isize;
    type WndProc = unsafe extern "system" fn(Hwnd, u32, usize, isize) -> Lresult;

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowLongPtrW(hwnd: Hwnd, nindex: i32, dwnewlong: isize) -> isize;
        fn CallWindowProcW(
            lp_prev_wnd_proc: Option<WndProc>,
            hwnd: Hwnd,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> Lresult;
        fn ShowWindow(hwnd: Hwnd, ncmdshow: i32) -> i32;
    }

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static PREV: AtomicIsize = AtomicIsize::new(0);

    /// 开关：仅在「定住」悬浮模式下拦截最小化。
    pub fn set_active(on: bool) {
        ACTIVE.store(on, Ordering::SeqCst);
    }

    /// 子类化主窗口过程。GWLP_WNDPROC 只能在创建窗口的线程上设置，
    /// 因此必须从 Tauri 的 setup 钩子（主线程）调用。
    pub fn install(hwnd: Hwnd) {
        unsafe {
            let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, guard_proc as usize as isize);
            if prev != 0 {
                PREV.store(prev, Ordering::SeqCst);
            }
        }
    }

    unsafe extern "system" fn guard_proc(
        hwnd: Hwnd,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> Lresult {
        if ACTIVE.load(Ordering::SeqCst) {
            // 吞掉最小化命令，窗口保持显示
            if msg == WM_SYSCOMMAND && (wparam & 0xFFF0) == SC_MINIMIZE {
                return 0;
            }
            // 兜底：若仍被最小化，立即还原
            if msg == WM_SIZE && wparam == SIZE_MINIMIZED {
                ShowWindow(hwnd, SW_RESTORE);
                return 0;
            }
        }
        let prev = PREV.load(Ordering::SeqCst);
        CallWindowProcW(
            std::mem::transmute::<isize, Option<WndProc>>(prev),
            hwnd,
            msg,
            wparam,
            lparam,
        )
    }
}

// 普通窗口尺寸
const NORMAL_W: f64 = 420.0;
const NORMAL_H: f64 = 500.0;
const NORMAL_MIN_W: f64 = 380.0;
const NORMAL_MIN_H: f64 = 480.0;
// 悬浮窗（定住）尺寸
// 宽度按内容自适应：前端量出卡片「贴合内容」时的宽度后调 set_float_width 收缩窗口，
// 上限 COMPACT_W（容纳最长文案「近 5 小时剩余 + MM-DD HH:MM 重置」），
// 短内容平台（如 Trae 的积分）就不用再在右侧空出一大片。
// 高度以内容里最高的元素（52px 的圆环）为基准：卡片竖向留白 = COMPACT_H - 52，
// 定 70 后上下各约 9px（原 88 是 36px，用户反馈太空，砍掉一半）。
const COMPACT_W: f64 = 264.0;
const COMPACT_H: f64 = 70.0;
const COMPACT_MIN_W: f64 = 180.0;
const COMPACT_MIN_H: f64 = 60.0;

// 悬浮窗放大 / 缩小过渡动画：总时长与每帧间隔
const ANIM_MS: u64 = 180;
const ANIM_FRAME_MS: u64 = 8;

/// 悬浮窗（定住态）上次摆放的物理坐标与「是否定住」都存在 settings.json 里，
/// 跨进程保留：下次启动直接回到退出时那个悬浮窗，而不是回到主界面。
/// 仅内存态没法满足「退出软件后下次回到悬浮窗」的需求，所以落盘（不含任何密钥）。
static PINNED: AtomicBool = AtomicBool::new(false);

/// 写入「定住」状态与悬浮窗坐标（`pos` 为 None 时保留原有的坐标）
fn persist_pin_state(app: &tauri::AppHandle, pinned: bool, pos: Option<(i32, i32)>) {
    let mut settings = read_settings(app);
    settings["pinned"] = serde_json::json!(pinned);
    if let Some((x, y)) = pos {
        settings["floatPos"] = serde_json::json!({ "x": x, "y": y });
    }
    let _ = write_settings(app, &settings);
}

/// 上次是否处于「定住」态
fn saved_pinned(app: &tauri::AppHandle) -> bool {
    read_settings(app)
        .get("pinned")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// 上次悬浮窗摆放的物理坐标
fn saved_float_pos(app: &tauri::AppHandle) -> Option<(i32, i32)> {
    let settings = read_settings(app);
    let p = settings.get("floatPos")?;
    Some((p.get("x")?.as_i64()? as i32, p.get("y")?.as_i64()? as i32))
}

/// 记录悬浮窗当前位置（在窗口仍为悬浮尺寸时调用）。
fn save_float_pos(window: &tauri::WebviewWindow) {
    let pos = window.outer_position().ok().map(|p| (p.x, p.y));
    persist_pin_state(window.app_handle(), false, pos);
}

/// 把目标外框左上角约束到当前显示器工作区（已扣除菜单栏 / Dock / 任务栏）。
/// `size` 为窗口「目标」外框尺寸（含标题栏等 chrome），返回修正后的坐标。
fn clamped_pos(window: &tauri::WebviewWindow, pos: (i32, i32), size: (u32, u32)) -> (i32, i32) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return pos;
    };

    let area = monitor.work_area();
    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);
    let (sw, sh) = (size.0 as i32, size.1 as i32);

    // 窗口比工作区还大时，上限取工作区左上角，避免 min > max 触发 clamp 断言。
    let x = pos.0.clamp(ax, (ax + aw - sw).max(ax));
    let y = pos.1.clamp(ay, (ay + ah - sh).max(ay));
    (x, y)
}

/// 逻辑尺寸 -> 物理尺寸（按当前显示器缩放比）。
fn physical_size(window: &tauri::WebviewWindow, w: f64, h: f64) -> (u32, u32) {
    let scale = window.scale_factor().unwrap_or(1.0);
    ((w * scale).round() as u32, (h * scale).round() as u32)
}

/// 窗口外框与内尺寸的差值（标题栏 / 边框占用的像素），与窗口大小无关。
fn chrome_delta(window: &tauri::WebviewWindow) -> (i32, i32) {
    match (window.outer_size(), window.inner_size()) {
        (Ok(o), Ok(i)) => (
            (o.width as i32 - i.width as i32).max(0),
            (o.height as i32 - i.height as i32).max(0),
        ),
        _ => (0, 0),
    }
}

/// 缓动函数：先快后慢，收尾平稳。
fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

/// 插值并取整。
fn lerp_i32(from: i32, to: i32, t: f64) -> i32 {
    (from as f64 + (to as f64 - from as f64) * t).round() as i32
}

/// 让窗口从「当前坐标 + 当前尺寸」平滑过渡到目标矩形。
///
/// - `to_pos` 为目标外框左上角（物理像素，与 `set_position` 语义一致）
/// - `to_size` 为目标内尺寸（物理像素，与 `set_size` 语义一致）
///
/// 起点取自窗口实时状态，终点即目标矩形本身，最后一帧精确落到目标值，
/// 因此动画结束不会出现「再跳一下」。
fn animate_rect(window: &tauri::WebviewWindow, to_pos: (i32, i32), to_size: (u32, u32)) {
    let (Ok(from_pos), Ok(from_size)) = (window.outer_position(), window.inner_size()) else {
        // 读不到起点时退化为直接落到目标，至少保证位置正确
        let _ = window.set_size(tauri::PhysicalSize::new(to_size.0, to_size.1));
        let _ = window.set_position(tauri::PhysicalPosition::new(to_pos.0, to_pos.1));
        return;
    };

    let (fx, fy) = (from_pos.x, from_pos.y);
    let (fw, fh) = (from_size.width as i32, from_size.height as i32);

    let steps = (ANIM_MS / ANIM_FRAME_MS).max(1);
    for step in 1..=steps {
        let t = ease_out_cubic(step as f64 / steps as f64);
        let x = lerp_i32(fx, to_pos.0, t);
        let y = lerp_i32(fy, to_pos.1, t);
        let w = lerp_i32(fw, to_size.0 as i32, t).max(1) as u32;
        let h = lerp_i32(fh, to_size.1 as i32, t).max(1) as u32;
        let _ = window.set_size(tauri::PhysicalSize::new(w, h));
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        std::thread::sleep(std::time::Duration::from_millis(ANIM_FRAME_MS));
    }

    // 精确落到目标，消除逐帧取整的累计误差
    let _ = window.set_size(tauri::PhysicalSize::new(to_size.0, to_size.1));
    let _ = window.set_position(tauri::PhysicalPosition::new(to_pos.0, to_pos.1));
}

/// 置为「定住」形态：小尺寸下限、不可缩放、无边框、置顶、无阴影。
/// 启动恢复与手动点「定住」共用，启动时不走动画。
fn apply_pinned_style(window: &tauri::WebviewWindow) -> Result<(), String> {
    window
        .set_min_size(Some(tauri::LogicalSize::new(COMPACT_MIN_W, COMPACT_MIN_H)))
        .map_err(|e| e.to_string())?;
    window.set_resizable(false).map_err(|e| e.to_string())?;
    window.set_always_on_top(true).map_err(|e| e.to_string())?;
    window.set_decorations(false).map_err(|e| e.to_string())?;
    // 关闭 macOS 原生窗口阴影：透明窗口下它会在悬浮卡片四周的透明区域里投出一圈灰色阴影
    window.set_shadow(false).map_err(|e| e.to_string())?;
    Ok(())
}

/// 计算「定住」后的目标矩形：高度固定为悬浮高度，宽度取前端量到的贴合宽度
/// （`None` 则用上限 COMPACT_W），位置优先还原上次摆放处，否则沿用当前坐标。
fn pinned_target(window: &tauri::WebviewWindow, width: Option<f64>) -> ((i32, i32), (u32, u32)) {
    let w = width.unwrap_or(COMPACT_W).clamp(COMPACT_MIN_W, COMPACT_W);
    let target_size = physical_size(window, w, COMPACT_H);
    let cur = window.outer_position().map(|p| (p.x, p.y)).unwrap_or((0, 0));
    let pos = clamped_pos(window, saved_float_pos(window.app_handle()).unwrap_or(cur), target_size);
    (pos, target_size)
}

/// 切换「定住」悬浮窗模式：缩小窗口、无边框、置顶。
#[tauri::command]
async fn set_pinned(
    window: tauri::WebviewWindow,
    on: bool,
    width: Option<f64>,
) -> Result<(), String> {
    if on {
        apply_pinned_style(&window)?;

        // 提前算好最终落点（宽度用前端量出的贴合宽度，动画一次到位、动画后不再跳一下）
        let (target_pos, target_size) = pinned_target(&window, width);

        // 先让前端切到悬浮布局，再动画，保证收缩过程里展示的就是悬浮卡片
        let _ = window.emit("pin-state", true);
        animate_rect(&window, target_pos, target_size);

        // 记住「已定住」与当前坐标：中途直接退出软件也能回到这里
        PINNED.store(true, Ordering::Relaxed);
        persist_pin_state(window.app_handle(), true, Some(target_pos));

        // 定住期间拦截 Win+D / Win+M，避免悬浮窗被一并隐藏
        #[cfg(target_os = "windows")]
        float_guard::set_active(true);
    } else {
        // 在窗口仍为悬浮尺寸时记录当前位置，供下次定住时还原
        save_float_pos(&window);
        PINNED.store(false, Ordering::Relaxed);
        // 退出悬浮态时收起提示窗，避免它孤零零挂在主界面上方
        if let Some(tip) = window.app_handle().get_webview_window("tip") {
            let _ = tip.hide();
        }
        #[cfg(target_os = "windows")]
        float_guard::set_active(false);
        window.set_always_on_top(false).map_err(|e| e.to_string())?;
        window.set_shadow(true).map_err(|e| e.to_string())?;

        // macOS 上 set_decorations(true) 是通过 GCD 异步重设 styleMask 的，而
        // set_title_bar_style(Overlay) 里的 FullSizeContentView 是同步读取当前 styleMask，
        // 若紧接着调用会被随后生效的异步 mask 覆盖，导致窗口顶部残留 28px 原生标题栏空带、
        // 红绿灯失效。因此放到独立线程：先恢复 decorations，等异步 mask 生效后再套用 Overlay。
        //
        // 注意：动画期间保持悬浮态的最小尺寸（COMPACT_MIN），否则从 236x100 放大到 420x500
        // 的中间帧都小于 380x480，会被系统 min_size 钳制导致动画失效；等动画结束再放开。
        let w = window.clone();
        std::thread::spawn(move || {
            let _ = w.set_decorations(true);
            // set_decorations 的 styleMask 是异步生效的：轮询「外框 - 内尺寸」的差值，
            // 一旦出现标题栏占用就说明 mask 已落地，立即套用 Overlay，把中间那一下
            // 原生标题栏（红绿灯）闪现的时间压到最短。
            for _ in 0..60 {
                if chrome_delta(&w).1 > 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            #[cfg(target_os = "macos")]
            let _ = w.set_title_bar_style(tauri::TitleBarStyle::Overlay);
            // 等 Overlay 生效（标题栏占用归零）后再测量目标矩形
            for _ in 0..60 {
                if chrome_delta(&w).1 == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }

            // 目标矩形：普通尺寸 + 约束回工作区后的坐标（此时标题栏已恢复，chrome 差值准确）
            let (cw, ch) = chrome_delta(&w);
            let inner = physical_size(&w, NORMAL_W, NORMAL_H);
            let outer = (
                (inner.0 as i32 + cw).max(0) as u32,
                (inner.1 as i32 + ch).max(0) as u32,
            );
            let cur = w.outer_position().map(|p| (p.x, p.y)).unwrap_or((0, 0));
            let target_pos = clamped_pos(&w, cur, outer);

            let _ = w.emit("pin-state", false);
            animate_rect(&w, target_pos, inner);

            // 动画结束后再放开最小尺寸与可缩放
            let _ = w.set_min_size(Some(tauri::LogicalSize::new(NORMAL_MIN_W, NORMAL_MIN_H)));
            let _ = w.set_resizable(true);
        });
    }
    Ok(())
}

/// 悬浮卡按内容自适应宽度：前端量出卡片「贴合内容」时的宽度后回调。
/// 只在定住态生效（普通窗口的尺寸由用户自己拖），并把宽度夹在 [COMPACT_MIN_W, COMPACT_W] 之间。
#[tauri::command]
fn set_float_width(window: tauri::WebviewWindow, width: f64) -> Result<(), String> {
    if !PINNED.load(Ordering::Relaxed) {
        return Ok(());
    }
    let w = width.clamp(COMPACT_MIN_W, COMPACT_W);
    let (pw, ph) = physical_size(&window, w, COMPACT_H);
    window
        .set_size(tauri::PhysicalSize::new(pw, ph))
        .map_err(|e| e.to_string())
}

/* ---------- 悬浮卡片右键菜单 ---------- */

/// 悬浮卡片右键菜单的两个菜单项 id
const CARD_MENU_UNPIN: &str = "card-unpin";
const CARD_MENU_QUIT: &str = "card-quit";

/// 弹出悬浮卡片的右键菜单。菜单由系统绘制，因此可以超出悬浮窗这个极小的窗口边界
/// （在网页里自绘菜单会被窗口裁掉）。
/// 「关闭悬浮窗」发回前端走 set_pinned，与卡片上的 × 同一条路径；
/// 「退出软件」直接退出——退出前会由 run 回调把定住状态与坐标落盘。
#[tauri::command]
async fn show_card_menu(window: tauri::WebviewWindow) -> Result<(), String> {
    let menu = tauri::menu::MenuBuilder::new(&window)
        .text(CARD_MENU_UNPIN, "关闭悬浮窗")
        .text(CARD_MENU_QUIT, "退出软件")
        .build()
        .map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

/// 启动时前端来问「上次是不是定住态」：窗口在 setup 里已按定住形态摆好，
/// 前端据此在窗口还隐藏时就把布局切成悬浮卡片，再调 show_window，避免主界面闪一下。
#[tauri::command]
fn startup_state(app: tauri::AppHandle) -> serde_json::Value {
    serde_json::json!({ "pinned": saved_pinned(&app) })
}

/// 显示主窗口。窗口以 visible:false 创建，等前端把布局（主界面 / 悬浮卡片）切好再显示。
#[tauri::command]
fn show_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())
}

/* ---------- 悬浮卡片悬停提示窗 ---------- */

/// 提示窗尺寸（逻辑像素）与它和卡片之间的间距。
/// 宽度固定，高度按内容自适应：各平台提示行数不同，固定 186 会让行数少的
/// （比如 Trae 只有 4 行）下方空出一大块。TIP_H 只作为首帧兜底高度与上限。
const TIP_W: f64 = 240.0;
const TIP_H: f64 = 186.0;
const TIP_MIN_H: f64 = 64.0;
const TIP_GAP: f64 = 6.0;

/// 上一次量到的提示内容高度（逻辑像素）。下次显示先用它开窗，
/// 避免先按 186 开、再被前端量到的高度缩一下的可见跳动。
static TIP_LAST_H: AtomicU32 = AtomicU32::new(186);

/// 显示悬浮卡片的悬停提示窗。
///
/// 单独开一个窗口而不是在卡片窗口里画：卡片窗口只有 264x100，提示要挂在卡片上方、
/// 内容比窗口还高，在原窗口里画一定会被裁掉。提示窗在 setup 里设了
/// `ignore_cursor_events`，不接收任何鼠标事件，所以不会挡住它后面（桌面或其他应用）的点击。
/// 内容由前端算好传进来（套餐 / 剩余 / 已用 / 预计可用天数等），这里只负责定位与显隐。
#[tauri::command]
fn show_float_tip(app: tauri::AppHandle, payload: serde_json::Value) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("找不到主窗口")?;
    let tip = app.get_webview_window("tip").ok_or("找不到提示窗口")?;

    let scale = main.scale_factor().map_err(|e| e.to_string())?;
    let pos = main.outer_position().map_err(|e| e.to_string())?;
    let size = main.outer_size().map_err(|e| e.to_string())?;

    let tip_w = (TIP_W * scale).round() as u32;
    let tip_h = (TIP_LAST_H.load(Ordering::Relaxed) as f64 * scale).round() as u32;
    let gap = (TIP_GAP * scale).round() as i32;

    let top = main
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| m.work_area().position.y)
        .unwrap_or(0);

    // 默认贴在卡片正上方；上方放不下就让到卡片下方
    let mut y = pos.y - tip_h as i32 - gap;
    if y < top {
        y = pos.y + size.height as i32 + gap;
    }
    y = y.max(top);

    let _ = tip.set_size(tauri::PhysicalSize::new(tip_w, tip_h));
    let _ = tip.set_position(tauri::PhysicalPosition::new(pos.x, y));
    // 先把内容推过去再显示，避免中间闪一帧空白
    let _ = tip.emit("tip-data", payload);
    tip.show().map_err(|e| e.to_string())
}

/// 提示窗按内容自适应高度：提示窗前端量出内容真实高度后回调（逻辑像素）。
/// 宽度不变，只改高度，并按「贴卡片上方、放不下就挪到下方」的同一规则重新摆位，
/// 免得收缩后位置跑偏。
#[tauri::command]
fn set_tip_height(app: tauri::AppHandle, height: f64) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("找不到主窗口")?;
    let tip = app.get_webview_window("tip").ok_or("找不到提示窗口")?;

    let scale = main.scale_factor().map_err(|e| e.to_string())?;
    let pos = main.outer_position().map_err(|e| e.to_string())?;
    let size = main.outer_size().map_err(|e| e.to_string())?;

    let h = height.clamp(TIP_MIN_H, TIP_H);
    TIP_LAST_H.store(h as u32, Ordering::Relaxed);

    let tip_w = (TIP_W * scale).round() as u32;
    let tip_h = (h * scale).round() as u32;
    let gap = (TIP_GAP * scale).round() as i32;

    let top = main
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| m.work_area().position.y)
        .unwrap_or(0);

    let mut y = pos.y - tip_h as i32 - gap;
    if y < top {
        y = pos.y + size.height as i32 + gap;
    }
    y = y.max(top);

    let _ = tip.set_size(tauri::PhysicalSize::new(tip_w, tip_h));
    let _ = tip.set_position(tauri::PhysicalPosition::new(pos.x, y));
    Ok(())
}

/// 隐藏悬停提示窗（鼠标离开卡片、开始拖拽、退出悬浮态时调用）
#[tauri::command]
fn hide_float_tip(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(tip) = app.get_webview_window("tip") {
        tip.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // 启动阶段零钥匙串访问：两次一次性迁移（明文→钥匙串、遗留服务名迁移）
            // 已挪到「连接账号」按需解锁流程里执行，见 unlock_source。

            // Windows：子类化主窗口过程，为「定住」悬浮窗保留防最小化的能力
            #[cfg(target_os = "windows")]
            {
                if let Some(win) = app.get_webview_window("main") {
                    if let Ok(hwnd) = win.hwnd() {
                        float_guard::install(hwnd.0);
                    }
                }
            }

            // 悬停提示窗：纯展示，不接收鼠标事件（不挡住后面的点击）、不抢焦点
            if let Some(tip) = app.get_webview_window("tip") {
                let _ = tip.set_ignore_cursor_events(true);
                let _ = tip.set_focusable(false);
            }

            let Some(win) = app.get_webview_window("main") else {
                return Ok(());
            };

            // 上次是定住态：窗口还隐藏着，先把悬浮形态与坐标摆好（不走动画，否则会有一次无意义的缩放）
            if saved_pinned(app.handle()) {
                PINNED.store(true, Ordering::Relaxed);
                if let Err(e) = apply_pinned_style(&win) {
                    eprintln!("恢复悬浮窗失败：{e}");
                }
                let (pos, size) = pinned_target(&win, None);
                let _ = win.set_size(tauri::PhysicalSize::new(size.0, size.1));
                let _ = win.set_position(tauri::PhysicalPosition::new(pos.0, pos.1));
                #[cfg(target_os = "windows")]
                float_guard::set_active(true);
            }

            // 兜底：前端若没能调 show_window（脚本出错），3 秒后强制显示，
            // 免得出现「一个窗口都没有、只能去活动监视器杀进程」的死局。
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if let Some(w) = handle.get_webview_window("main") {
                    if !w.is_visible().unwrap_or(true) {
                        let _ = w.show();
                    }
                }
            });

            Ok(())
        })
        .on_menu_event(|app, event| match event.id().0.as_str() {
            CARD_MENU_UNPIN => {
                let _ = app.emit("menu-unpin", ());
            }
            CARD_MENU_QUIT => app.exit(0),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            list_sources,
            save_source_credentials,
            clear_source_credentials,
            set_active_source,
            set_refresh_interval,
            get_source_data,
            test_source,
            unlock_source,
            open_url,
            set_pinned,
            set_float_width,
            set_tip_height,
            show_card_menu,
            startup_state,
            show_window,
            show_float_tip,
            hide_float_tip
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // 退出前落盘：定住态下把「当前」坐标存下来，下次启动才能回到退出时那一刻的位置
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if PINNED.load(Ordering::Relaxed) {
                    let pos = app
                        .get_webview_window("main")
                        .and_then(|w| w.outer_position().ok())
                        .map(|p| (p.x, p.y));
                    persist_pin_state(app, true, pos);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自校验：读取本机 Trae CN 登录态，断言解密结果的 SHA-512 校验通过且能解析出 token。
    /// 文件不存在时跳过（不失败）。断言中不输出 token 内容。
    #[test]
    fn trae_decrypt_self_check() {
        let path = trae_storage_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            println!("跳过：未找到 Trae CN 登录态文件（{}）", path.display());
            return;
        };
        let storage: serde_json::Value =
            serde_json::from_str(&text).expect("登录态文件应为合法 JSON");

        let auth = trae_auth_json(&storage).expect("解密 + SHA-512 校验应通过");
        let token = auth.get("token").and_then(|v| v.as_str()).unwrap_or("");
        assert!(!token.is_empty(), "解密结果应能解析出非空 token");

        println!(
            "Trae 登录态解密自校验通过（SHA-512 校验 OK，token 已解析但未输出）；套餐身份：{}",
            trae_identity(&storage)
        );
    }

    /// 未解锁任何源（默认）：读钥匙串的函数必须直接返回 None，绝不调用 keyring，
    /// 因此启动过程不会触碰钥匙串、不会弹出 macOS 授权框。
    #[test]
    fn locked_reads_skip_keychain() {
        assert!(!source_unlocked("deepseek"), "会话解锁集合默认应为空");
        // 未解锁的源读取「直接返回空」，函数体内不会调用 keyring（否则真机会弹授权框）
        assert!(
            read_keychain_creds("deepseek").is_none(),
            "未解锁的源读钥匙串应返回 None"
        );
    }
}
