// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_store::StoreExt;
use reqwest;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct Settings {
    show_xau: bool,
    show_ms: bool,
    show_gh: bool,
    show_zs: bool,
    bg_color: String,
}

struct AppSettings(Mutex<Settings>);

const SETTINGS_KEY: &str = "settings";
const STORE_PATH: &str = "settings.bin";

fn default_settings() -> Settings {
    Settings {
        show_xau: true,
        show_ms: true,
        show_gh: true,
        show_zs: true,
        bg_color: "#2c3e50".to_string(),
    }
}

fn log_line(message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut path = std::env::temp_dir();
    path.push("JDfund.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[{}] {}", timestamp, message);
    }
}

// 获取设置
#[tauri::command]
fn get_settings<R: Runtime>(app: AppHandle<R>, _state: State<AppSettings>) -> Settings {
    let store = match app.store(STORE_PATH) {
        Ok(store) => store,
        Err(err) => {
            log_line(&format!("get_settings: open store failed: {err}"));
            return default_settings();
        }
    };
    match store.get(SETTINGS_KEY) {
        Some(value) => match serde_json::from_value(value) {
            Ok(settings) => settings,
            Err(err) => {
                log_line(&format!("get_settings: decode failed: {err}"));
                default_settings()
            }
        },
        None => default_settings(),
    }
}

// 保存设置
#[tauri::command]
fn save_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<AppSettings>,
    settings: Settings,
) -> Result<(), String> {
    let store = app.store(STORE_PATH).map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    store.set(SETTINGS_KEY, value);
    store.save().map_err(|e| e.to_string())?;
    
    match state.0.lock() {
        Ok(mut guard) => *guard = settings.clone(),
        Err(_) => {
            log_line("save_settings: settings lock poisoned");
            return Err("Settings lock poisoned".to_string());
        }
    }
    
    // 通知前端设置已更新
    let _ = app.emit("settings-updated", settings);
    Ok(())
}

// 切换特定设置的显示状态
#[tauri::command]
fn toggle_platform<R: Runtime>(
    app: AppHandle<R>,
    state: State<AppSettings>,
    platform: String,
) -> Result<Settings, String> {
    let mut current = match state.0.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => {
            log_line("toggle_platform: settings lock poisoned");
            return Err("Settings lock poisoned".to_string());
        }
    };
    
    match platform.as_str() {
        "xau" => current.show_xau = !current.show_xau,
        "ms" => current.show_ms = !current.show_ms,
        "gh" => current.show_gh = !current.show_gh,
        "zs" => current.show_zs = !current.show_zs,
        _ => return Err("Unknown platform".to_string()),
    }
    
    save_settings(app.clone(), state, current.clone())?;
    Ok(current)
}

// 设置背景颜色
#[tauri::command]
fn set_bg_color<R: Runtime>(
    app: AppHandle<R>,
    state: State<AppSettings>,
    color: String,
) -> Result<Settings, String> {
    let mut current = match state.0.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => {
            log_line("set_bg_color: settings lock poisoned");
            return Err("Settings lock poisoned".to_string());
        }
    };
    current.bg_color = color;
    save_settings(app.clone(), state, current.clone())?;
    Ok(current)
}

// 退出应用
#[tauri::command]
fn quit_app<R: Runtime>(app: AppHandle<R>) {
    app.exit(0);
}

// 自定义 HTTP 请求（跳过 SSL 验证）
#[tauri::command]
async fn fetch_with_no_ssl(url: String, method: String, body: Option<String>) -> Result<String, String> {
    log_line(&format!("fetch_with_no_ssl: {} {}", method, url));
    
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| {
            log_line(&format!("fetch_with_no_ssl: client build error: {}", e));
            e.to_string()
        })?;
    
    let request = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => {
            let mut req = client.post(&url);
            if let Some(body_str) = body {
                req = req.header("Content-Type", "application/json").body(body_str);
            }
            req
        },
        _ => return Err("Unsupported method".to_string()),
    };
    
    let response = request.send().await.map_err(|e| {
        log_line(&format!("fetch_with_no_ssl: request error: {}", e));
        e.to_string()
    })?;
    
    let status = response.status();
    log_line(&format!("fetch_with_no_ssl: status {}", status));
    
    let text = response.text().await.map_err(|e| {
        log_line(&format!("fetch_with_no_ssl: read body error: {}", e));
        e.to_string()
    })?;
    
    log_line(&format!("fetch_with_no_ssl: response body: {}", &text[..text.len().min(200)]));
    
    Ok(text)
}

// 启动 WebSocket 客户端（在 Rust 后端）
#[tauri::command]
async fn start_websocket<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    log_line("start_websocket: begin");
    
    let app_clone = app.clone();
    tokio::spawn(async move {
        loop {
            log_line("start_websocket: connecting...");
            
            // 使用 native-tls 连接器，跳过证书验证
            let connector = native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap();
            
            let connector = tokio_tungstenite::Connector::NativeTls(connector);
            
            match connect_async_with_config(
                "wss://cfws.jdjygold.com/data",
                None,
                false,
                Some(connector),
            ).await {
                Ok((mut ws_stream, _)) => {
                    log_line("start_websocket: connected");
                    
                    // 发送订阅消息
                    let subscribe_msg = r#"{"action":"2","bizType":"2","keys":["WG-XAUUSD"]}"#;
                    if let Err(e) = ws_stream.send(Message::Text(subscribe_msg.to_string())).await {
                        log_line(&format!("start_websocket: send error: {}", e));
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        continue;
                    }
                    log_line("start_websocket: subscribed");
                    
                    // 接收消息
                    while let Some(msg) = ws_stream.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                log_line(&format!("start_websocket: received: {}", &text[..text.len().min(100)]));
                                let _ = app_clone.emit("xau-price-update", text);
                            }
                            Ok(Message::Close(_)) => {
                                log_line("start_websocket: connection closed");
                                break;
                            }
                            Err(e) => {
                                log_line(&format!("start_websocket: error: {}", e));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    log_line(&format!("start_websocket: connect error: {}", e));
                }
            }
            
            log_line("start_websocket: reconnecting in 5s...");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    });
    
    Ok(())
}

async fn connect_async_with_config(
    url: &str,
    _config: Option<()>,
    _disable_nagle: bool,
    connector: Option<tokio_tungstenite::Connector>,
) -> Result<(tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, tokio_tungstenite::tungstenite::handshake::client::Response), tokio_tungstenite::tungstenite::Error> {
    let url = url::Url::parse(url).unwrap();
    let request = tokio_tungstenite::tungstenite::handshake::client::Request::builder()
        .uri(url.as_str())
        .header("Host", url.host_str().unwrap())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
        .body(())
        .unwrap();
    
    tokio_tungstenite::connect_async_tls_with_config(request, None, false, connector).await
}

// 显示/隐藏窗口
fn toggle_window_visibility<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            // 确保窗口置顶
            let _ = window.set_always_on_top(true);
        }
    }
}

// 设置窗口到右下角
fn position_window_bottom_right<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        // 获取主显示器信息
        if let Ok(Some(monitor)) = window.primary_monitor() {
            let size = monitor.size();
            let position = monitor.position();
            
            // 计算窗口位置 (右下角留一些边距)
            let window_width = 280.0;
            let window_height = 40.0;
            let margin = 10.0;
            
            let x = position.x as f64 + size.width as f64 - window_width - margin;
            let y = position.y as f64 + size.height as f64 - window_height - margin;
            
            let _ = window.set_position(tauri::Position::Physical(
                tauri::PhysicalPosition::new(x as i32, y as i32),
            ));
        }
    }
}

// 创建托盘菜单
fn create_tray_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, tauri::Error> {
    let show_i = MenuItem::with_id(app, "show", "显示/隐藏", true, None::<&str>)?;
    let xau_i = MenuItem::with_id(app, "toggle_xau", "显示 XAU", true, None::<&str>)?;
    let ms_i = MenuItem::with_id(app, "toggle_ms", "显示民生", true, None::<&str>)?;
    let gh_i = MenuItem::with_id(app, "toggle_gh", "显示工行", true, None::<&str>)?;
    let zs_i = MenuItem::with_id(app, "toggle_zs", "显示浙商", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    
    // 颜色子菜单
    let dark_i = MenuItem::with_id(app, "color_dark", "深色", true, None::<&str>)?;
    let blue_i = MenuItem::with_id(app, "color_blue", "蓝色", true, None::<&str>)?;
    let black_i = MenuItem::with_id(app, "color_black", "黑色", true, None::<&str>)?;
    
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    
    Menu::with_items(
        app,
        &[
            &show_i,
            &sep,
            &xau_i,
            &ms_i,
            &gh_i,
            &zs_i,
            &sep2,
            &dark_i,
            &blue_i,
            &black_i,
            &quit_i,
        ],
    )
}

pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        log_line(&format!("panic: {info}"));
    }));
    log_line("app start");
    tauri::Builder::default()
        .plugin(tauri_plugin_websocket::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(AppSettings(Mutex::new(default_settings())))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            toggle_platform,
            set_bg_color,
            quit_app,
            fetch_with_no_ssl,
            start_websocket
        ])
        .setup(|app| {
            log_line("setup: begin");
            // 加载设置
            let settings: Settings = match app.store(STORE_PATH) {
                Ok(store) => match store.get(SETTINGS_KEY) {
                    Some(value) => serde_json::from_value(value).unwrap_or_else(|_| default_settings()),
                    None => default_settings(),
                },
                Err(err) => {
                    log_line(&format!("setup: open store failed: {err}"));
                    default_settings()
                }
            };
            
            // 保存到状态
            match app.state::<AppSettings>().0.lock() {
                Ok(mut guard) => guard.clone_from(&settings),
                Err(_) => {
                    log_line("setup: settings lock poisoned");
                    return Err("settings lock poisoned".into());
                }
            }
            
            // 如果配置未创建窗口，则补建一个，避免重复创建导致闪退
            if app.get_webview_window("main").is_none() {
                let _window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("黄金价格")
                    .inner_size(280.0, 40.0)
                    .min_inner_size(100.0, 30.0)
                    .max_inner_size(400.0, 60.0)
                    .always_on_top(true)
                    .decorations(false)
                    .skip_taskbar(true)
                    .resizable(false)
                    .build()?;
            }
            
            // 设置窗口到右下角
            position_window_bottom_right(app.handle());
            
            // 创建托盘（失败不影响主程序启动）
            if let Ok(tray_menu) = create_tray_menu(app.handle()) {
                if let Some(icon) = app.default_window_icon() {
                    let _ = TrayIconBuilder::new()
                        .icon(icon.clone())
                        .menu(&tray_menu)
                        .tooltip("黄金价格监控")
                        .on_menu_event(|app, event| match event.id.as_ref() {
                            "show" => toggle_window_visibility(app),
                            "toggle_xau" => {
                                let state = app.state::<AppSettings>();
                                let _ = toggle_platform(app.clone(), state, "xau".to_string());
                            }
                            "toggle_ms" => {
                                let state = app.state::<AppSettings>();
                                let _ = toggle_platform(app.clone(), state, "ms".to_string());
                            }
                            "toggle_gh" => {
                                let state = app.state::<AppSettings>();
                                let _ = toggle_platform(app.clone(), state, "gh".to_string());
                            }
                            "toggle_zs" => {
                                let state = app.state::<AppSettings>();
                                let _ = toggle_platform(app.clone(), state, "zs".to_string());
                            }
                            "color_dark" => {
                                let state = app.state::<AppSettings>();
                                let _ = set_bg_color(app.clone(), state, "#2c3e50".to_string());
                            }
                            "color_blue" => {
                                let state = app.state::<AppSettings>();
                                let _ = set_bg_color(app.clone(), state, "#1e3a5f".to_string());
                            }
                            "color_black" => {
                                let state = app.state::<AppSettings>();
                                let _ = set_bg_color(app.clone(), state, "#000000".to_string());
                            }
                            "quit" => app.exit(0),
                            _ => {}
                        })
                        .on_tray_icon_event(|tray: &tauri::tray::TrayIcon<tauri::Wry>, event: tauri::tray::TrayIconEvent| {
                            if let TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            } = event
                            {
                                toggle_window_visibility(tray.app_handle());
                            }
                        })
                        .build(app);
                    log_line("setup: tray created");
                } else {
                    log_line("setup: tray icon missing");
                }
            } else {
                log_line("setup: tray menu create failed");
            }
            
            log_line("setup: done");
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            log_line(&format!("run error: {err}"));
        });
}
