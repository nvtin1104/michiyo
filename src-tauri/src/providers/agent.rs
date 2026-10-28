//! Claude Agent SDK runs outside WebView. Credentials travel only over private stdin.
use crate::secrets;
use serde::Deserialize;
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tauri::Manager;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::Notify,
};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Event {
    Delta { text: String },
    Done { text: String },
    Error { message: String },
    Cancelled,
}

fn directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/claude-agent"))
    } else {
        Ok(app
            .path()
            .resource_dir()
            .map_err(|_| "Không tìm thấy tài nguyên ứng dụng")?
            .join("claude-agent"))
    }
}

// Closing the job also kills the SDK's Claude Code child if cancellation/timeout
// prevents graceful shutdown. It never touches unrelated user processes.
#[cfg(windows)]
struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessTree {}
#[cfg(windows)]
impl ProcessTree {
    fn attach(pid: u32) -> Result<Self, String> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{JobObjects::*, Threading::*},
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err("Không tạo được tiến trình SDK an toàn".into());
            }
            let guard = Self(job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                return Err("Không giới hạn được vòng đời SDK".into());
            }
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false as i32, pid);
            if process.is_null() {
                return Err("Không truy cập được tiến trình SDK".into());
            }
            let attached = AssignProcessToJobObject(job, process);
            CloseHandle(process);
            if attached == 0 {
                return Err("Không quản lý được tiến trình SDK".into());
            }
            Ok(guard)
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

pub async fn run(
    app: &tauri::AppHandle,
    mut request: Value,
    cancel: Arc<Notify>,
    mut on_delta: impl FnMut(&str) + Send,
) -> Result<String, String> {
    let directory = directory(app)?;
    let node = directory.join(if cfg!(windows) { "node.exe" } else { "node" });
    if !node.is_file()
        || !directory.join("worker.mjs").is_file()
        || !directory
            .join(if cfg!(windows) {
                "claude.exe"
            } else {
                "claude"
            })
            .is_file()
    {
        return Err(
            "Chưa có Claude Agent SDK runtime. Chạy npm run build:agent rồi mở lại app.".into(),
        );
    }
    request["apiKey"] = Value::String(secrets::get("claude")?);
    let mut command = Command::new(&node);
    command
        .arg(directory.join("worker.mjs"))
        .current_dir(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .env_clear();
    for name in [
        "PATH",
        "Path",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "LANG",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| "Không chạy được Claude Agent SDK runtime")?;
    #[cfg(windows)]
    let _tree = ProcessTree::attach(child.id().ok_or("Không tìm thấy tiến trình SDK")?)?;
    let mut stdin = child.stdin.take().ok_or("Không kết nối được SDK stdin")?;
    let stdout = child.stdout.take().ok_or("Không kết nối được SDK stdout")?;
    let mut payload = serde_json::to_vec(&request).map_err(|_| "Yêu cầu SDK không hợp lệ")?;
    payload.push(b'\n');
    tokio::select! {_=cancel.notified()=>return Ok(String::new()),written=stdin.write_all(&payload)=>{written.map_err(|_|"Không gửi được yêu cầu tới SDK")?;}}
    drop(payload);
    drop(request);
    let mut lines = BufReader::new(stdout).lines();
    let deadline = tokio::time::sleep(Duration::from_secs(130));
    tokio::pin!(deadline);
    let result = loop {
        tokio::select! {
            _=cancel.notified()=>{let _=stdin.write_all(b"{\"cancel\":true}\n").await;break Ok(String::new());},
            _=&mut deadline=>{let _=stdin.write_all(b"{\"cancel\":true}\n").await;break Err("Claude Agent SDK phản hồi quá lâu. Hãy thử lại.".into());},
            line=lines.next_line()=>{
                let line=match line{Ok(Some(line))=>line,Ok(None)=>break Err("Claude Agent SDK đóng kết nối trước khi hoàn tất".into()),Err(_)=>break Err("Kết nối SDK bị gián đoạn".into())};
                if line.len()>2_000_000{break Err("Phản hồi SDK quá lớn".into())}
                match serde_json::from_str::<Event>(&line){
                    Ok(Event::Delta{text})=>on_delta(&text),
                    Ok(Event::Done{text})=>break Ok(text),
                    Ok(Event::Error{message})=>break Err(message),
                    Ok(Event::Cancelled)=>break Ok(String::new()),
                    Err(_)=>break Err("Phản hồi SDK không hợp lệ".into())
                }
            }
        }
    };
    drop(stdin);
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[tokio::test]
    async fn dropping_process_tree_stops_owned_worker() {
        let node =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/claude-agent/node.exe");
        assert!(node.is_file(), "Run npm run build:agent before Rust tests");
        let mut worker = Command::new(node)
            .args(["-e", "setInterval(()=>{},1000)"])
            .creation_flags(0x08000000)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let tree = ProcessTree::attach(worker.id().unwrap()).unwrap();
        drop(tree);
        let status = tokio::time::timeout(Duration::from_secs(3), worker.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(!status.success());
    }
    #[test]
    fn decodes_unicode_deltas() {
        assert!(
            matches!(serde_json::from_str::<Event>(r#"{"type":"delta","text":"Chào 🌱"}"#).unwrap(),Event::Delta{text} if text=="Chào 🌱")
        );
    }
    #[test]
    fn done_and_error_are_distinct() {
        assert!(matches!(
            serde_json::from_str::<Event>(r#"{"type":"done","text":"OK"}"#).unwrap(),
            Event::Done { .. }
        ));
        assert!(matches!(
            serde_json::from_str::<Event>(r#"{"type":"error","message":"Không kết nối"}"#).unwrap(),
            Event::Error { .. }
        ));
    }
}
