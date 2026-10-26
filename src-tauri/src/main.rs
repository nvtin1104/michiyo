#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
mod config;
mod providers;
mod secrets;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("mascot") {
                let _ = w.show();
                let _ = app.emit_to("mascot", "mascot-visible", true);
            }
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::Runtime::default())
        .invoke_handler(tauri::generate_handler![
            commands::save_secret,
            commands::secret_status,
            commands::translate,
            commands::chat,
            commands::stop_chat,
            commands::test_connection,
            commands::open_panel,
            commands::snap_mascot,
            commands::apply_config,
            commands::set_hit_mask,
            commands::skins::list_skins,
            commands::skins::load_skin,
            commands::take_selection,
            commands::take_chat_prompt,
            commands::ask_translation,
            commands::reset_position,
            commands::copy_text
        ])
        .setup(|app| {
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let show = MenuItem::with_id(app, "show", "Hiện / Ẩn", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Cài đặt", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &settings, &quit])?;
            TrayIconBuilder::new()
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/icon.png"
                ))?)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("mascot") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                                let _ = app.emit_to("mascot", "mascot-visible", false);
                            } else {
                                let _ = w.show();
                                let _ = app.emit_to("mascot", "mascot-visible", true);
                            }
                        }
                    }
                    "settings" => {
                        let h = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = commands::show_panel(&h, "settings", false);
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            let w = app.get_webview_window("mascot").unwrap();
            let store = app.store("settings.json")?;
            if let Some(p) = store.get("position") {
                if let (Some(x), Some(y)) = (p["x"].as_i64(), p["y"].as_i64()) {
                    let _ = w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
                }
            } else if let Some(m) = w.primary_monitor()? {
                let p = m.position();
                let s = m.size();
                let _ = w.set_position(tauri::PhysicalPosition::new(
                    p.x + s.width as i32 - 180,
                    p.y + s.height as i32 - 230,
                ));
            }
            let h = app.handle().clone();
            let setup_handle = h.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = commands::apply_config(setup_handle.clone()).await {
                    let _ = setup_handle.emit("config-error", e);
                }
            });
            // Poll only the tiny mascot hit mask; ignore transparent pixels even when click-through is active.
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(60));
                let Some(w) = h.get_webview_window("mascot") else {
                    break;
                };
                if !w.is_visible().unwrap_or(false) {
                    continue;
                }
                if let (Ok(cursor), Ok(p), Ok(size)) =
                    (w.cursor_position(), w.outer_position(), w.inner_size())
                {
                    let x = ((cursor.x - p.x as f64) * 160. / size.width as f64).floor() as i32;
                    let y = ((cursor.y - p.y as f64) * 160. / size.height as f64).floor() as i32;
                    let runtime = h.state::<commands::Runtime>();
                    let mask = runtime.mask.lock().unwrap();
                    let hit =
                        x >= 0 && x < 160 && y >= 0 && y < 160 && mask[(y * 160 + x) as usize];
                    let _ = w.set_ignore_cursor_events(!hit);
                }
            });
            let h = app.handle().clone();
            std::thread::spawn(move || {
                let mut last = String::new();
                let mut watching = false;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    if h.state::<commands::Runtime>()
                        .capturing
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        continue;
                    }
                    if config::read(&h)
                        .ok()
                        .and_then(|c| c["clipboardWatch"].as_bool())
                        != Some(true)
                    {
                        watching = false;
                        continue;
                    }
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(text) = clipboard.get_text() {
                            if !watching {
                                last = text;
                                watching = true;
                                continue;
                            }
                            let ignored = h
                                .state::<commands::Runtime>()
                                .ignored_clipboard
                                .lock()
                                .unwrap()
                                .take();
                            if ignored.as_ref() == Some(&text) {
                                last = text;
                                continue;
                            }
                            if text != last && !text.trim().is_empty() {
                                last = text.clone();
                                let _ = commands::show_panel(&h, "translate", true);
                                *h.state::<commands::Runtime>()
                                    .pending_selection
                                    .lock()
                                    .unwrap() = Some(text);
                                let _ = h.emit_to("translate", "selection", ());
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|w, event| {
            if matches!(event, tauri::WindowEvent::Moved(_)) && w.label() == "mascot" {
                use std::sync::atomic::Ordering;
                let h = w.app_handle().clone();
                let generation = h
                    .state::<commands::Runtime>()
                    .move_generation
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(160));
                    if h.state::<commands::Runtime>()
                        .move_generation
                        .load(Ordering::SeqCst)
                        != generation
                    {
                        return;
                    }
                    #[cfg(windows)]
                    if unsafe {
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(1)
                    } < 0
                    {
                        continue;
                    }
                    let _ = commands::snap_mascot(h);
                    break;
                });
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = w.hide();
                if w.label() == "mascot" {
                    let _ = w.app_handle().emit_to("mascot", "mascot-visible", false);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("Không khởi động được Michiyo");
}
