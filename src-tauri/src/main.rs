#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tauri::{Window, Manager};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// =========================================================
// DATA STRUCTURES
// =========================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LauncherConfig {
    username: String,
    auth_token: Option<String>,
    mc_dir: String,
    ram: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProgressPayload {
    status: String,
    val: u64,
    max_val: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ServerStatus {
    online: bool,
    players_online: u32,
    players_max: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LoginResponse {
    token: String,
    username: String,
}

// =========================================================
// UTILITIES AND HELPERS
// =========================================================

fn get_default_mc_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".battleverse_launcher")
}

fn get_config_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".battleverse_config.json")
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn resolve_mediafire(url: &str) -> String {
    if !url.contains("mediafire.com") {
        return url.to_string();
    }
    
    // Resolve direct download link from Mediafire HTML
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(10))
        .build();
        
    if let Ok(c) = client {
        if let Ok(resp) = c.get(url).send() {
            if let Ok(text) = resp.text() {
                if let Some(pos) = text.find("href=\"https://download") {
                    let sub = &text[pos + 6..];
                    if let Some(end) = sub.find('"') {
                        return sub[..end].to_string();
                    }
                }
            }
        }
    }
    url.to_string()
}

fn get_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn get_download_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

// =========================================================
// WINDOWS OPTIMIZATIONS
// =========================================================

#[cfg(target_os = "windows")]
fn get_active_power_scheme() -> Option<String> {
    let out = Command::new("powercfg")
        .args(&["-getactivescheme"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .ok()?;
    let out_str = String::from_utf8_lossy(&out.stdout);
    
    // GUID format: 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c
    // Extract GUID via string matching
    if let Some(pos) = out_str.find("GUID:") {
        let sub = &out_str[pos + 5..].trim();
        if sub.len() >= 36 {
            return Some(sub[..36].to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn set_power_scheme(guid: &str) {
    let _ = Command::new("powercfg")
        .args(&["-setactive", guid])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .status();
}

#[cfg(target_os = "windows")]
fn empty_working_set() {
    unsafe {
        let current_process = windows_sys::Win32::System::Threading::GetCurrentProcess();
        windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet(current_process);
    }
}

#[cfg(target_os = "windows")]
fn set_process_high_priority(pid: u32) {
    use windows_sys::Win32::System::Threading::{
        OpenProcess, SetPriorityClass, HIGH_PRIORITY_CLASS, PROCESS_SET_INFORMATION
    };
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle != 0 {
            SetPriorityClass(handle, HIGH_PRIORITY_CLASS);
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
    }
}

// Fallback stubs for non-Windows platforms
#[cfg(not(target_os = "windows"))]
fn get_active_power_scheme() -> Option<String> { None }
#[cfg(not(target_os = "windows"))]
fn set_power_scheme(_guid: &str) {}
#[cfg(not(target_os = "windows"))]
fn empty_working_set() {}
#[cfg(not(target_os = "windows"))]
fn set_process_high_priority(_pid: u32) {}

// =========================================================
// TAURI COMMANDS
// =========================================================

#[tauri::command]
fn load_config() -> LauncherConfig {
    let path = get_config_file_path();
    if path.exists() {
        if let Ok(file) = File::open(&path) {
            if let Ok(config) = serde_json::from_reader::<_, LauncherConfig>(file) {
                return config;
            }
        }
    }
    
    LauncherConfig {
        username: "".to_string(),
        auth_token: None,
        mc_dir: get_default_mc_dir().to_string_lossy().to_string(),
        ram: "6".to_string(),
    }
}

#[tauri::command]
fn save_config(config: LauncherConfig) -> Result<(), String> {
    let path = get_config_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    
    let file = File::create(&path).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(file, &config).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn fetch_remote_config(url: String) -> Result<serde_json::Value, String> {
    let client = get_http_client();
    let resp = client.get(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    Ok(json)
}

#[tauri::command]
async fn check_server_status(ip: String) -> Result<ServerStatus, String> {
    let url = format!("https://api.mcsrvstat.us/3/{}", ip);
    let client = get_http_client();
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let data = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    
    let online = data.get("online").and_then(|v| v.as_bool()).unwrap_or(false);
    let players_online = data.get("players")
        .and_then(|p| p.get("online"))
        .and_then(|o| o.as_u64())
        .unwrap_or(0) as u32;
    let players_max = data.get("players")
        .and_then(|p| p.get("max"))
        .and_then(|m| m.as_u64())
        .unwrap_or(100) as u32;
        
    Ok(ServerStatus { online, players_online, players_max })
}

#[tauri::command]
async fn login_request(api_url: String, payload: serde_json::Value) -> Result<LoginResponse, String> {
    let client = get_http_client();
    let resp = client.post(&format!("{}/login", api_url))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if resp.status().is_success() {
        let data = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
        let token = data.get("token").and_then(|t| t.as_str()).ok_or("No token in response")?.to_string();
        let username = data.get("user")
            .and_then(|u| u.get("username"))
            .and_then(|un| un.as_str())
            .unwrap_or("Player")
            .to_string();
            
        Ok(LoginResponse { token, username })
    } else {
        let data = resp.json::<serde_json::Value>().await.ok();
        let err_msg = data.as_ref()
            .and_then(|d| d.get("error"))
            .and_then(|e| e.as_str())
            .unwrap_or("Login failed");
        Err(err_msg.to_string())
    }
}

#[tauri::command]
async fn fetch_messages(api_url: String, token: String) -> Result<serde_json::Value, String> {
    let client = get_http_client();
    let resp = client.get(&format!("{}/messages", api_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    let json = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    Ok(json)
}

#[tauri::command]
async fn prepare_join(api_url: String, token: String) -> Result<(), String> {
    let client = get_http_client();
    let _ = client.post(&format!("{}/prepare-join", api_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await;
    Ok(())
}

#[tauri::command]
async fn download_and_install_pack(
    window: Window,
    download_url: String,
    mc_dir: String,
    ignore_list: Vec<String>,
) -> Result<(), String> {
    let mc_dir_path = Path::new(&mc_dir);
    if !mc_dir_path.exists() {
        fs::create_dir_all(mc_dir_path).map_err(|e| e.to_string())?;
    }
    
    // Resolve download link
    let resolved_url = tokio::task::spawn_blocking(move || resolve_mediafire(&download_url))
        .await
        .map_err(|e| e.to_string())?;
        
    let client = get_download_client();
    
    let send_progress = |status: &str, val: u64, max_val: u64| {
        let _ = window.emit("download-progress", ProgressPayload {
            status: status.to_string(),
            val,
            max_val,
        });
    };
    
    send_progress("СОЕДИНЕНИЕ С СЕРВЕРОМ...", 0, 100);
    
    let resp = client.get(&resolved_url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    let total_size = resp.content_length().unwrap_or(0);
    let zip_path = mc_dir_path.join("pack.zip");
    let mut file = File::create(&zip_path).map_err(|e| e.to_string())?;
    
    send_progress("ЗАГРУЗКА ИГРОВЫХ АРХИВОВ...", 0, total_size.max(1));
    
    let mut downloaded = 0;
    let mut stream = resp.bytes_stream();
    
    while let Some(chunk_result) = futures_util::StreamExt::next(&mut stream).await {
        let chunk = chunk_result.map_err(|e| e.to_string())?;
        std::io::copy(&mut chunk.as_ref(), &mut file).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        send_progress("ЗАГРУЗКА ИГРОВЫХ АРХИВОВ...", downloaded, total_size.max(1));
    }
    
    drop(file);
    
    // Backup settings
    send_progress("РЕЗЕРВНОЕ КОПИРОВАНИЕ...", 100, 100);
    let backup_dir = mc_dir_path.join(".battleverse_backup");
    let _ = fs::remove_dir_all(&backup_dir);
    let _ = fs::create_dir_all(&backup_dir);
    
    for item in &ignore_list {
        let src = mc_dir_path.join(item);
        let dst = backup_dir.join(item);
        if src.exists() {
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if src.is_dir() {
                let _ = copy_dir_all(&src, &dst);
            } else {
                let _ = fs::copy(&src, &dst);
            }
        }
    }
    
    // Clean old files
    send_progress("ОЧИСТКА СТАРЫХ ДАННЫХ...", 100, 100);
    let clean_dirs = ["mods", "config", "scripts", "kubejs", "tacz", "defaultconfigs"];
    for folder in &clean_dirs {
        let p = mc_dir_path.join(folder);
        if p.exists() {
            let _ = fs::remove_dir_all(p);
        }
    }
    
    // Extract zip
    send_progress("ОБРАБОТКА ДАННЫХ...", 0, 100);
    let zip_path_clone = zip_path.clone();
    let mc_dir_clone = mc_dir_path.to_path_buf();
    let window_clone = window.clone();
    
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let zip_file = File::open(&zip_path_clone).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| e.to_string())?;
        let total_files = archive.len();
        
        for i in 0..total_files {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let outpath = match file.enclosed_name() {
                Some(path) => mc_dir_clone.join(path),
                None => continue,
            };
            
            if (*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p).map_err(|e| e.to_string())?;
                    }
                }
                let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
            }
            
            if i % 10 == 0 || i == total_files - 1 {
                let _ = window_clone.emit("download-progress", ProgressPayload {
                    status: "РАСПАКОВКА ФАЙЛОВ...".to_string(),
                    val: i as u64,
                    max_val: total_files as u64,
                });
            }
        }
        Ok(())
    }).await.map_err(|e| e.to_string())??;
    
    let _ = fs::remove_file(zip_path);
    
    // Restore backup
    send_progress("ВОССТАНОВЛЕНИЕ НАСТРОЕК...", 100, 100);
    for item in &ignore_list {
        let src = backup_dir.join(item);
        let dst = mc_dir_path.join(item);
        if src.exists() {
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if src.is_dir() {
                let _ = copy_dir_all(&src, &dst);
            } else {
                let _ = fs::copy(&src, &dst);
            }
        }
    }
    let _ = fs::remove_dir_all(&backup_dir);
    
    send_progress("ГОТОВО", 100, 100);
    Ok(())
}

fn collect_jars(dir: &Path) -> Vec<String> {
    let mut jars = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                jars.extend(collect_jars(&path));
            } else if path.extension().map_or(false, |ext| ext == "jar") {
                jars.push(path.to_string_lossy().to_string());
            }
        }
    }
    jars
}

fn get_libraries_from_json(mc_dir: &str, version_id: &str) -> Vec<String> {
    let json_path = Path::new(mc_dir)
        .join("versions")
        .join(version_id)
        .join(format!("{}.json", version_id));
        
    let mut libs = Vec::new();
    if let Ok(content) = fs::read_to_string(&json_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(lib_array) = v.get("libraries").and_then(|l| l.as_array()) {
                for item in lib_array {
                    let mut allowed = true;
                    if let Some(rules) = item.get("rules").and_then(|r| r.as_array()) {
                        for rule in rules {
                            if let Some(action) = rule.get("action").and_then(|a| a.as_str()) {
                                if let Some(os) = rule.get("os").and_then(|o| o.as_object()) {
                                    if let Some(os_name) = os.get("name").and_then(|n| n.as_str()) {
                                        if os_name == "windows" {
                                            allowed = action == "allow";
                                        } else {
                                            allowed = action == "disallow";
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    if allowed {
                        if let Some(path) = item.get("downloads")
                            .and_then(|d| d.get("artifact"))
                            .and_then(|a| a.get("path"))
                            .and_then(|p| p.as_str()) {
                                libs.push(path.to_string());
                        } else if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                            let parts: Vec<&str> = name.split(':').collect();
                            if parts.len() >= 3 {
                                let group = parts[0].replace('.', "/");
                                let artifact = parts[1];
                                let version = parts[2];
                                let classifier = if parts.len() == 4 { format!("-{}", parts[3]) } else { "".to_string() };
                                let path = format!("{}/{}/{}/{}-{}{}.jar", group, artifact, version, artifact, version, classifier);
                                libs.push(path);
                            }
                        }
                    }
                }
            }
        }
    }
    libs
}

fn get_jvm_args_from_json(mc_dir: &str, version_id: &str) -> Result<Vec<String>, String> {
    let json_path = Path::new(mc_dir)
        .join("versions")
        .join(version_id)
        .join(format!("{}.json", version_id));
        
    if !json_path.exists() {
        return Err(format!("Файл конфигурации версии не найден: {:?}", json_path));
    }
    
    let content = fs::read_to_string(&json_path)
        .map_err(|e| format!("Не удалось прочитать JSON версии: {}", e))?;
        
    let v: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Не удалось распарсить JSON версии: {}", e))?;
        
    let mut jvm_args = Vec::new();
    
    if let Some(jvm_array) = v.get("arguments").and_then(|a| a.get("jvm")).and_then(|j| j.as_array()) {
        for item in jvm_array {
            if let Some(s) = item.as_str() {
                jvm_args.push(s.to_string());
            } else if let Some(obj) = item.as_object() {
                let mut allowed = true;
                if let Some(rules) = obj.get("rules").and_then(|r| r.as_array()) {
                    for rule in rules {
                        if let Some(action) = rule.get("action").and_then(|a| a.as_str()) {
                            if let Some(os) = rule.get("os").and_then(|o| o.as_object()) {
                                if let Some(os_name) = os.get("name").and_then(|n| n.as_str()) {
                                    if os_name == "windows" {
                                        allowed = action == "allow";
                                    } else {
                                        allowed = action == "disallow";
                                    }
                                }
                            }
                        }
                    }
                }
                if allowed {
                    if let Some(value) = obj.get("value") {
                        if let Some(val_str) = value.as_str() {
                            jvm_args.push(val_str.to_string());
                        } else if let Some(val_arr) = value.as_array() {
                            for val_item in val_arr {
                                if let Some(val_item_str) = val_item.as_str() {
                                    jvm_args.push(val_item_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    let libraries_dir = Path::new(mc_dir).join("libraries").to_string_lossy().to_string();
    let natives_dir = Path::new(mc_dir).join("versions").join(version_id).join("natives").to_string_lossy().to_string();
    
    for arg in &mut jvm_args {
        *arg = arg.replace("${library_directory}", &libraries_dir);
        *arg = arg.replace("${natives_directory}", &natives_dir);
        *arg = arg.replace("${classpath_separator}", ";");
        *arg = arg.replace("${version_name}", version_id);
    }
    
    Ok(jvm_args)
}

fn get_game_args_from_json(mc_dir: &str, version_id: &str) -> Vec<String> {
    let json_path = Path::new(mc_dir)
        .join("versions")
        .join(version_id)
        .join(format!("{}.json", version_id));
        
    let mut game_args = Vec::new();
    if let Ok(content) = fs::read_to_string(&json_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(game_array) = v.get("arguments").and_then(|a| a.get("game")).and_then(|g| g.as_array()) {
                for item in game_array {
                    if let Some(s) = item.as_str() {
                        game_args.push(s.to_string());
                    }
                }
            }
        }
    }
    game_args
}

#[tauri::command]
async fn launch_game(
    window: Window,
    app_handle: tauri::AppHandle,
    mc_dir: String,
    mc_version: String,
    forge_version: String,
    username: String,
    ram_gb: u32,
) -> Result<(), String> {
    let ram_mb = ram_gb * 1024;
    
    let send_progress = |status: &str, val: u64, max_val: u64| {
        let _ = window.emit("download-progress", ProgressPayload {
            status: status.to_string(),
            val,
            max_val,
        });
    };
    
    send_progress("ПОДГОТОВКА СРЕДЫ...", 10, 100);
    
    // Find java.exe under mc_dir/runtime/...
    let java_exe = Path::new(&mc_dir)
        .join("runtime")
        .join("java-runtime-gamma")
        .join("windows-x64")
        .join("java-runtime-gamma")
        .join("bin")
        .join("java.exe");
        
    let java_path = if java_exe.exists() {
        java_exe.to_string_lossy().to_string()
    } else {
        "java".to_string() // fallback to system Java
    };
    
    // Construct classpath
    send_progress("СБОРКА КЛАССОВ (CLASSPATH)...", 40, 100);
    
    let libraries_dir = Path::new(&mc_dir).join("libraries");
    if !libraries_dir.exists() {
        return Err("Папка libraries не найдена! Установите сборку перед запуском.".to_string());
    }
    
    let mut jars = Vec::new();
    
    // 1. Get libraries from vanilla JSON
    let vanilla_libs = get_libraries_from_json(&mc_dir, &mc_version);
    for lib in vanilla_libs {
        let full_path = libraries_dir.join(&lib);
        jars.push(full_path.to_string_lossy().to_string());
    }
    
    // 2. Get libraries from Forge JSON
    let version_id = if forge_version.is_empty() {
        mc_version.clone()
    } else {
        format!("{}-forge-{}", mc_version, forge_version)
    };
    
    let forge_libs = get_libraries_from_json(&mc_dir, &version_id);
    for lib in forge_libs {
        let full_path = libraries_dir.join(&lib);
        jars.push(full_path.to_string_lossy().to_string());
    }
    
    let forge_client_jar = Path::new(&mc_dir)
        .join("versions")
        .join(&version_id)
        .join(format!("{}.jar", version_id));
        
    if forge_client_jar.exists() {
        jars.push(forge_client_jar.to_string_lossy().to_string());
    }
    
    let classpath = jars.join(";");
    
    // Build arguments
    send_progress("НАСТРОЙКА АРГУМЕНТОВ...", 70, 100);
    
    let mut args = Vec::new();
    
    // Memory settings
    args.push(format!("-Xmx{}M", ram_mb));
    args.push(format!("-Xms{}M", ram_mb));
    
    // Load JVM args dynamically from JSON
    let dynamic_jvm_args = get_jvm_args_from_json(&mc_dir, &version_id)?;
    
    // Filter out -cp and classpath from dynamic jvm args, we push them manually
    let mut skip_next = false;
    for arg in dynamic_jvm_args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "-cp" || arg == "-classpath" {
            skip_next = true;
            continue;
        }
        if arg == "${classpath}" {
            continue;
        }
        args.push(arg);
    }
    
    // Push classpath
    args.push("-cp".to_string());
    args.push(classpath);
    
    // Main class
    args.push("cpw.mods.bootstraplauncher.BootstrapLauncher".to_string());
    
    // Game arguments
    args.push("--username".to_string());
    args.push(username.clone());
    
    args.push("--version".to_string());
    args.push(version_id.clone());
    
    args.push("--gameDir".to_string());
    args.push(mc_dir.clone());
    
    args.push("--assetsDir".to_string());
    args.push(Path::new(&mc_dir).join("assets").to_string_lossy().to_string());
    
    args.push("--assetIndex".to_string());
    args.push("5".to_string());
    
    let user_uuid = format!("{:x}", uuid::Uuid::new_v3(&uuid::Uuid::NAMESPACE_DNS, username.as_bytes()));
    args.push("--uuid".to_string());
    args.push(user_uuid);
    
    args.push("--accessToken".to_string());
    args.push("".to_string());
    
    args.push("--clientId".to_string());
    args.push("${clientid}".to_string());
    
    args.push("--xuid".to_string());
    args.push("${auth_xuid}".to_string());
    
    args.push("--userType".to_string());
    args.push("msa".to_string());
    
    args.push("--versionType".to_string());
    args.push("release".to_string());
    
    // Append version-specific game arguments from JSON
    let extra_game_args = get_game_args_from_json(&mc_dir, &version_id);
    args.extend(extra_game_args);
    
    // Launch game and write logs
    send_progress("ЗАПУСК ИГРЫ...", 100, 100);
    
    let log_file_path = Path::new(&mc_dir).join("launcher_crash_log.txt");
    let mut log_file = std::fs::File::create(&log_file_path)
        .map_err(|e| format!("Не удалось создать файл лога: {}", e))?;
        
    use std::io::Write;
    let _ = writeln!(log_file, "Java Path: {}", java_path);
    let _ = writeln!(log_file, "Arguments: {:?}", args);
    let _ = writeln!(log_file, "Cwd: {}", mc_dir);
    let _ = writeln!(log_file, "----------------------------------------");
    
    let log_file_out = log_file.try_clone().map_err(|e| e.to_string())?;
    let log_file_err = log_file.try_clone().map_err(|e| e.to_string())?;
    
    let prev_power_plan = get_active_power_scheme();
    set_power_scheme("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c");
    
    let mut mc_process_cmd = Command::new(java_path);
    mc_process_cmd.args(&args);
    mc_process_cmd.current_dir(&mc_dir);
    mc_process_cmd.stdout(std::process::Stdio::from(log_file_out));
    mc_process_cmd.stderr(std::process::Stdio::from(log_file_err));
    
    #[cfg(target_os = "windows")]
    mc_process_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    let child = mc_process_cmd.spawn();
    match child {
        Ok(mut child_proc) => {
            let pid = child_proc.id();
            set_process_high_priority(pid);
            empty_working_set();
            
            let window_clone = window.clone();
            let _ = window.emit("game-active", true);
            
            tokio::task::spawn(async move {
                let _ = child_proc.wait();
                if let Some(plan) = prev_power_plan {
                    set_power_scheme(&plan);
                }
                let _ = window_clone.emit("game-active", false);
                let _ = window_clone.emit("download-progress", ProgressPayload {
                    status: "".to_string(),
                    val: 0,
                    max_val: 100,
                });
            });
            Ok(())
        }
        Err(e) => {
            if let Some(plan) = prev_power_plan {
                set_power_scheme(&plan);
            }
            Err(format!("Не удалось запустить Java процесс игры: {}", e))
        }
    }
}

#[tauri::command]
fn check_local_version(mc_dir: String, remote_version: String) -> String {
    let version_file = Path::new(&mc_dir).join("modpack_version.txt");
    if !version_file.exists() {
        return "INSTALL".to_string();
    }
    match fs::read_to_string(version_file) {
        Ok(content) => {
            if content.trim() == remote_version.trim() {
                "LAUNCH".to_string()
            } else {
                "UPDATE".to_string()
            }
        }
        Err(_) => "INSTALL".to_string(),
    }
}

#[tauri::command]
fn write_version_file(mc_dir: String, version: String) -> Result<(), String> {
    let version_file = Path::new(&mc_dir).join("modpack_version.txt");
    fs::write(version_file, version.trim()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn select_game_directory(current_dir: String) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $f = New-Object System.Windows.Forms.FolderBrowserDialog; \
             $f.SelectedPath = '{}'; \
             $f.Description = 'Выберите папку для установки игры Battleverse'; \
             if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ $f.SelectedPath }}",
            current_dir.replace("'", "''")
        );
        let out = Command::new("powershell")
            .args(&["-NoProfile", "-Command", &script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
            .map_err(|e| e.to_string())?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if path.is_empty() {
            Ok(current_dir)
        } else {
            Ok(path)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(current_dir)
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            fetch_remote_config,
            check_server_status,
            login_request,
            fetch_messages,
            prepare_join,
            download_and_install_pack,
            launch_game,
            check_local_version,
            write_version_file,
            select_game_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

