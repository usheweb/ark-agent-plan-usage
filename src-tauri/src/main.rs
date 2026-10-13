// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use tauri::Manager;

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

/// 凭证文件路径：应用配置目录（macOS 下为 ~/Library/Application Support/<identifier>/）
fn credentials_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("获取配置目录失败: {}", e))?;
    Ok(dir.join("credentials.json"))
}

/// 读取应用内设置中保存的凭证
fn load_saved_credentials(app: &tauri::AppHandle) -> Option<(String, String)> {
    let path = credentials_path(app).ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let ak = v.get("accessKey")?.as_str()?.trim().to_string();
    let sk = v.get("secretKey")?.as_str()?.trim().to_string();
    if ak.is_empty() || sk.is_empty() {
        return None;
    }
    Some((ak, sk))
}

/// 把凭证写入应用配置目录，并把文件权限收紧为 600（仅当前用户可读）
fn save_saved_credentials(app: &tauri::AppHandle, ak: &str, sk: &str) -> Result<(), String> {
    let path = credentials_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    let data = serde_json::json!({ "accessKey": ak, "secretKey": sk });
    let bytes = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("写入凭证失败: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
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

/// 凭证优先级：应用内设置 > 环境变量 / .env
fn load_credentials(app: &tauri::AppHandle) -> Result<(String, String), String> {
    if let Some(pair) = load_saved_credentials(app) {
        return Ok(pair);
    }
    load_env_credentials().ok_or_else(|| {
        "尚未配置火山引擎 Access Key，请点击右上角「设置」填写。获取地址：https://console.volcengine.com/iam/keymanage"
            .to_string()
    })
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

#[tauri::command]
async fn get_afp_usage(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let (ak, sk) = load_credentials(&app)?;
    fetch_afp_usage(&ak, &sk).await
}

/// 读取当前生效的凭证（供设置弹窗回填），并说明来源
#[tauri::command]
fn get_credentials(app: tauri::AppHandle) -> serde_json::Value {
    let path = credentials_path(&app)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let (configured, ak, sk, source) = match (load_saved_credentials(&app), load_env_credentials()) {
        (Some((ak, sk)), _) => (true, ak, sk, "settings"),
        (None, Some((ak, sk))) => (true, ak, sk, "env"),
        (None, None) => (false, String::new(), String::new(), "none"),
    };
    serde_json::json!({
        "configured": configured, "accessKey": ak, "secretKey": sk, "source": source, "path": path
    })
}

#[tauri::command]
fn save_credentials(
    app: tauri::AppHandle,
    access_key: String,
    secret_key: String,
) -> Result<(), String> {
    let ak = access_key.trim();
    let sk = secret_key.trim();
    if ak.is_empty() || sk.is_empty() {
        return Err("Access Key 与 Secret Key 均不能为空".to_string());
    }
    save_saved_credentials(&app, ak, sk)
}

#[tauri::command]
fn clear_credentials(app: tauri::AppHandle) -> Result<(), String> {
    let path = credentials_path(&app)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("删除凭证失败: {}", e))?;
    }
    Ok(())
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

/// 用填写的凭证试调一次接口，用于「测试连接」
#[tauri::command]
async fn test_credentials(
    access_key: String,
    secret_key: String,
) -> Result<serde_json::Value, String> {
    let ak = access_key.trim();
    let sk = secret_key.trim();
    if ak.is_empty() || sk.is_empty() {
        return Err("请先填写 Access Key 与 Secret Key".to_string());
    }
    fetch_afp_usage(ak, sk).await
}

// 普通窗口尺寸
const NORMAL_W: f64 = 420.0;
const NORMAL_H: f64 = 500.0;
const NORMAL_MIN_W: f64 = 380.0;
const NORMAL_MIN_H: f64 = 480.0;
// 悬浮窗（定住）尺寸
const COMPACT_W: f64 = 236.0;
const COMPACT_H: f64 = 100.0;
const COMPACT_MIN_W: f64 = 200.0;
const COMPACT_MIN_H: f64 = 90.0;

/// 切换「定住」悬浮窗模式：缩小窗口、无边框、置顶。
#[tauri::command]
async fn set_pinned(window: tauri::WebviewWindow, on: bool) -> Result<(), String> {
    if on {
        window
            .set_min_size(Some(tauri::LogicalSize::new(COMPACT_MIN_W, COMPACT_MIN_H)))
            .map_err(|e| e.to_string())?;
        window.set_resizable(false).map_err(|e| e.to_string())?;
        window.set_always_on_top(true).map_err(|e| e.to_string())?;
        window.set_decorations(false).map_err(|e| e.to_string())?;
        // 关闭 macOS 原生窗口阴影：透明窗口下它会在悬浮卡片四周的透明区域里投出一圈灰色阴影
        window.set_shadow(false).map_err(|e| e.to_string())?;
        window
            .set_size(tauri::LogicalSize::new(COMPACT_W, COMPACT_H))
            .map_err(|e| e.to_string())?;
    } else {
        window.set_always_on_top(false).map_err(|e| e.to_string())?;
        window.set_shadow(true).map_err(|e| e.to_string())?;
        window
            .set_min_size(Some(tauri::LogicalSize::new(NORMAL_MIN_W, NORMAL_MIN_H)))
            .map_err(|e| e.to_string())?;
        window.set_resizable(true).map_err(|e| e.to_string())?;

        // macOS 上 set_decorations(true) 是通过 GCD 异步重设 styleMask 的，而
        // set_title_bar_style(Overlay) 里的 FullSizeContentView 是同步读取当前 styleMask，
        // 若紧接着调用会被随后生效的异步 mask 覆盖，导致窗口顶部残留 28px 原生标题栏空带、
        // 红绿灯失效。因此放到独立线程：先恢复 decorations，等异步 mask 生效后再套用
        // Overlay 并校正尺寸。
        let w = window.clone();
        std::thread::spawn(move || {
            let _ = w.set_decorations(true);
            std::thread::sleep(std::time::Duration::from_millis(150));
            #[cfg(target_os = "macos")]
            let _ = w.set_title_bar_style(tauri::TitleBarStyle::Overlay);
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = w.set_size(tauri::LogicalSize::new(NORMAL_W, NORMAL_H));
        });
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_afp_usage,
            get_credentials,
            save_credentials,
            clear_credentials,
            test_credentials,
            open_url,
            set_pinned
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
