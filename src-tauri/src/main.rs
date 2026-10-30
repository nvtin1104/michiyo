#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ai;
mod commands;
mod config;
mod db;
mod providers;
mod reminders;
mod secrets;
mod sync;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let h = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = commands::open_main(h, "chat".into()).await;
            });
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(commands::Runtime::default())
        .invoke_handler(tauri::generate_handler![
            commands::activity_list,
            commands::activity_delete,
            commands::activity_delete_range,
            commands::notes_list,
            commands::notes_save,
            commands::notes_delete,
            commands::habits_list,
            commands::habits_save,
            commands::habits_delete,
            commands::habits_check_in,
            commands::events_range,
            commands::events_save,
            commands::events_delete,
            commands::events_set_done,
            commands::set_timezone,
            commands::reminders_pending,
            commands::reminders_dismiss,
            commands::reminders_snooze,
            commands::history_list,
            commands::history_save,
            commands::history_delete,
            commands::save_secret,
            commands::secret_status,
            commands::translate,
            commands::chat,
            commands::stop_chat,
            commands::test_connection,
            commands::ai_models,
            commands::local_status,
            commands::local_pull_model,
            commands::local_cancel_pull,
            commands::local_delete_model,
            commands::cloud_pair,
            sync::sync_status,
            sync::sync_now,
            commands::ai_summarize,
            commands::ai_suggest_actions,
            commands::open_panel,
            commands::close_chat,
            commands::open_main,
            commands::window_minimize,
            commands::window_close_to_mascot,
            commands::open_translation_in_main,
            commands::open_chat_in_main,
            commands::take_shell_chat_draft,
            commands::take_translation_draft,
            commands::apply_config,
            commands::set_hit_mask,
            commands::skins::list_skins,
            commands::skins::load_skin,
            commands::take_selection,
            commands::take_chat_prompt,
            commands::ask_translation,
            commands::reset_position,
            commands::motion::motion_scene,
            commands::motion::motion_position,
            commands::motion::mascot_mouse_down,
            commands::copy_text,
            commands::resize_chat,
            commands::menu_action,
            commands::mascot_menu
        ])
        .setup(|app| {
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let show = MenuItem::with_id(app, "show", "Hiện / Ẩn", true, None::<&str>)?;
            let main = MenuItem::with_id(app, "main", "Mở Michiyo", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Cài đặt", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&main, &show, &settings, &quit])?;
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
                                let _ = app.emit_to("mascot", "mascot-boom", ());
                            }
                        }
                    }
                    "main" => {
                        let h = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = commands::open_main(h, "chat".into()).await;
                        });
                    }
                    "settings" => {
                        let h = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = commands::open_main(h, "settings".into()).await;
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
            reminders::start(app.handle().clone());
            sync::start(app.handle().clone());
            let h = app.handle().clone();
            let setup_handle = h.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = commands::apply_config(setup_handle.clone()).await {
                    let _ = setup_handle.emit("config-error", e);
                }
                let _ = commands::open_main(setup_handle, "chat".into()).await;
            });
            // Poll only the tiny mascot hit mask; emit pointer/hit events only when values change.
            std::thread::spawn(move || {
                let mut last_point: Option<(i32, i32)> = None;
                let mut last_hit: Option<bool> = None;
                loop {
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
                        let y =
                            ((cursor.y - p.y as f64) * 160. / size.height as f64).floor() as i32;
                        if last_point != Some((x, y)) {
                            last_point = Some((x, y));
                            let _ = w.emit("mascot-pointer", serde_json::json!({"x":x,"y":y}));
                        }
                        let runtime = h.state::<commands::Runtime>();
                        let mask = runtime.mask.lock().unwrap();
                        let hit =
                            x >= 0 && x < 160 && y >= 0 && y < 160 && mask[(y * 160 + x) as usize];
                        drop(mask);
                        if last_hit != Some(hit) {
                            last_hit = Some(hit);
                            let _ = w.set_ignore_cursor_events(!hit);
                        }
                    }
                }
            });
            let h = app.handle().clone();
            std::thread::spawn(move || {
                let mut last = String::new();
                let mut watching = false;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    let runtime = h.state::<commands::Runtime>();
                    if runtime.capturing.load(std::sync::atomic::Ordering::SeqCst) {
                        continue;
                    }
                    if !runtime
                        .clipboard_watch
                        .load(std::sync::atomic::Ordering::SeqCst)
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
                            let ignored = runtime.ignored_clipboard.lock().unwrap().take();
                            if ignored.as_ref() == Some(&text) {
                                last = text;
                                continue;
                            }
                            if text != last && !text.trim().is_empty() {
                                last = text.clone();
                                let _ = commands::show_panel(&h, "translate", true);
                                *runtime.pending_selection.lock().unwrap() = Some(text);
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
                let runtime = h.state::<commands::Runtime>();
                runtime.move_generation.fetch_add(1, Ordering::SeqCst);
                if !runtime.move_save_active.swap(true, Ordering::SeqCst) {
                    let h = h.clone();
                    std::thread::spawn(move || loop {
                        let generation = h
                            .state::<commands::Runtime>()
                            .move_generation
                            .load(Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(160));
                        if h.state::<commands::Runtime>()
                            .move_generation
                            .load(Ordering::SeqCst)
                            != generation
                            || commands::motion::mascot_mouse_down()
                        {
                            continue;
                        }
                        let _ = commands::save_mascot_position(h.clone());
                        if let Some(chat) = h.get_webview_window("chat") {
                            if chat.is_visible().unwrap_or(false) {
                                let _ = commands::position_panel(&h, &chat, false);
                            }
                        }
                        let runtime = h.state::<commands::Runtime>();
                        runtime.move_save_active.store(false, Ordering::SeqCst);
                        if runtime.move_generation.load(Ordering::SeqCst) == generation
                            || runtime.move_save_active.swap(true, Ordering::SeqCst)
                        {
                            break;
                        }
                    });
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if w.label() == "main" {
                    let _ = commands::close_main_to_mascot_impl(w.app_handle());
                    return;
                }
                let _ = w.hide();
                if w.label() == "chat" {
                    commands::notify_chat_visibility(w.app_handle());
                }
                if w.label() == "mascot" {
                    let _ = w.app_handle().emit_to("mascot", "mascot-visible", false);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("Không khởi động được Michiyo");
}
