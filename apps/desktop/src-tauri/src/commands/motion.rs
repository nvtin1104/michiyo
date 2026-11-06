use serde_json::{json, Value};
use tauri::Manager;

// Physical coordinates throughout: monitor work area and window position use the same units.
#[tauri::command]
pub async fn motion_scene(app: tauri::AppHandle) -> Result<Value, String> {
    let window = app
        .get_webview_window("mascot")
        .ok_or("Mascot unavailable")?;
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("Monitor unavailable")?;
    let area = monitor.work_area();
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    use std::sync::atomic::Ordering;
    let scan = &app.state::<super::Runtime>().surface_scan_active;
    let surfaces = if scan.swap(true, Ordering::SeqCst) {
        Vec::new()
    } else {
        let handle = app.clone();
        let task = tauri::async_runtime::spawn_blocking(move || {
            let surfaces = find_surfaces();
            handle
                .state::<super::Runtime>()
                .surface_scan_active
                .store(false, Ordering::SeqCst);
            surfaces
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    };
    Ok(
        json!({"x": position.x, "y": position.y, "width": size.width, "height": size.height,
        "area": {"x": area.position.x, "y": area.position.y, "width": area.size.width, "height": area.size.height}, "surfaces": surfaces}),
    )
}

#[tauri::command]
pub fn motion_position(app: tauri::AppHandle, x: i32, y: i32) -> Result<(), String> {
    let window = app
        .get_webview_window("mascot")
        .ok_or("Mascot unavailable")?;
    if mascot_mouse_down() {
        return Err("Pointer interaction in progress".into());
    }
    if !window.is_visible().map_err(|e| e.to_string())? {
        return Err("Mascot hidden".into());
    }
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("Monitor unavailable")?;
    let area = monitor.work_area();
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let max_x = area.position.x + area.size.width.saturating_sub(size.width) as i32;
    let max_y = area.position.y + area.size.height.saturating_sub(size.height) as i32;
    window
        .set_position(tauri::PhysicalPosition::new(
            x.clamp(area.position.x, max_x),
            y.clamp(area.position.y, max_y),
        ))
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn find_surfaces() -> Vec<Value> {
    Vec::new()
}

#[cfg(windows)]
fn find_surfaces() -> Vec<Value> {
    use windows::Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_MULTITHREADED,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, TreeScope_Children, UIA_CustomControlTypeId,
                UIA_GroupControlTypeId, UIA_PaneControlTypeId,
            },
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return vec![];
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return vec![];
        }
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return vec![];
        }
        let result = (|| -> windows::core::Result<Vec<Value>> {
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let root = automation.ElementFromHandle(hwnd)?;
            let condition = automation.CreateTrueCondition()?;
            let mut queue = vec![(root, 0)];
            let mut out = Vec::new();
            let mut visited = 0;
            while let Some((parent, depth)) = queue.pop() {
                visited += 1;
                if visited > 80 { break; }
                if depth >= 3 || out.len() >= 64 { continue; }
                let Ok(children) = parent.FindAll(TreeScope_Children, &condition) else { continue };
                let count = children.Length().unwrap_or(0).min(40);
                for i in 0..count {
                    let Ok(element) = children.GetElement(i) else { continue };
                    if element.CurrentIsOffscreen().unwrap_or(true.into()).as_bool() { continue; }
                    if let (Ok(kind), Ok(rect)) = (element.CurrentControlType(), element.CurrentBoundingRectangle()) {
                        if (kind == UIA_PaneControlTypeId || kind == UIA_GroupControlTypeId || kind == UIA_CustomControlTypeId)
                            && rect.right - rect.left >= 100 && rect.bottom - rect.top >= 40 {
                            out.push(json!({"x":rect.left,"y":rect.top,"width":rect.right-rect.left,"height":rect.bottom-rect.top}));
                        }
                    }
                    queue.push((element, depth + 1));
                }
            }
            Ok(out)
        })().unwrap_or_default();
        CoUninitialize();
        result
    }
}
#[tauri::command]
pub fn mascot_mouse_down() -> bool {
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(1) < 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}
