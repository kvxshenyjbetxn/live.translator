use super::CaptureError;
use crate::{
    audio::{AudioMeters, CaptureQueues, FaultSignal},
    sources::{
        unchanged, windows_source_identity as identity, AudioSource, ProcessIdentity,
        ResolvedSource,
    },
};
use std::{
    collections::{BTreeMap, HashSet},
    mem::size_of,
    sync::Arc,
};
use windows::{
    core::{Interface, PWSTR},
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE},
        Media::Audio::{
            eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
            IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
            },
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    },
};

const ENUM_ERROR: CaptureError = CaptureError("Не вдалося отримати список аудіопрограм Windows.");

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn process(pid: u32, active: bool) -> Option<ProcessIdentity> {
    if pid == 0 || pid > i32::MAX as u32 {
        return None;
    }
    unsafe {
        let handle = OwnedHandle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_FORMAT(0),
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
        .ok()?;
        let executable = String::from_utf16(&path[..len as usize]).ok()?;
        let mut created = FILETIME::default();
        let (mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(handle.0, &mut created, &mut exit, &mut kernel, &mut user).ok()?;
        Some(ProcessIdentity {
            object_id: pid,
            pid: pid as i32,
            executable,
            started: (created.dwHighDateTime as u64, created.dwLowDateTime as u64),
            active,
        })
    }
}

/// Snapshot window ownership and its executable without inspecting window contents.
pub fn foreground_process() -> Option<(u32, String)> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return None;
        }
        let mut pid = 0;
        if GetWindowThreadProcessId(window, Some(&mut pid)) == 0 {
            return None;
        }
        process(pid, false).map(|p| (pid, p.executable))
    }
}

fn process_parents() -> Result<Vec<(u32, u32)>, CaptureError> {
    unsafe {
        let snapshot =
            OwnedHandle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|_| ENUM_ERROR)?);
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut parents = Vec::new();
        Process32FirstW(snapshot.0, &mut entry).map_err(|_| ENUM_ERROR)?;
        loop {
            parents.push((entry.th32ProcessID, entry.th32ParentProcessID));
            if Process32NextW(snapshot.0, &mut entry).is_err() {
                break;
            }
        }
        Ok(parents)
    }
}
fn descendants(root: u32, parents: &[(u32, u32)]) -> HashSet<u32> {
    let mut result = HashSet::from([root]);
    loop {
        let before = result.len();
        for &(pid, parent) in parents {
            if result.contains(&parent) {
                result.insert(pid);
            }
        }
        if result.len() == before {
            return result;
        }
    }
}

fn processes() -> Result<Vec<ProcessIdentity>, CaptureError> {
    let excluded = descendants(std::process::id(), &process_parents()?);
    unsafe {
        // A dedicated thread allows COM initialization regardless of the caller's apartment.
        let result = CoInitializeEx(None, COINIT_MULTITHREADED);
        result.ok().map_err(|_| ENUM_ERROR)?;
        let _com = ComGuard;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| ENUM_ERROR)?;
        let devices = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|_| ENUM_ERROR)?;
        let mut found = BTreeMap::<u32, ProcessIdentity>::new();
        for endpoint in 0..devices.GetCount().map_err(|_| ENUM_ERROR)? {
            let device = devices.Item(endpoint).map_err(|_| ENUM_ERROR)?;
            let manager: IAudioSessionManager2 =
                device.Activate(CLSCTX_ALL, None).map_err(|_| ENUM_ERROR)?;
            let sessions = manager.GetSessionEnumerator().map_err(|_| ENUM_ERROR)?;
            for index in 0..sessions.GetCount().map_err(|_| ENUM_ERROR)? {
                let session = sessions.GetSession(index).map_err(|_| ENUM_ERROR)?;
                let control: IAudioSessionControl2 = session.cast().map_err(|_| ENUM_ERROR)?;
                let pid = control.GetProcessId().map_err(|_| ENUM_ERROR)?;
                if excluded.contains(&pid) {
                    continue;
                }
                let active = session.GetState().map_err(|_| ENUM_ERROR)? == AudioSessionStateActive;
                if let Some(p) = found.get_mut(&pid) {
                    p.active |= active;
                } else if let Some(p) = process(pid, active) {
                    found.insert(pid, p);
                }
            }
        }
        Ok(found.into_values().collect())
    }
}
fn snapshot() -> Result<Vec<ProcessIdentity>, CaptureError> {
    std::thread::spawn(processes)
        .join()
        .map_err(|_| ENUM_ERROR)?
}
pub fn list_sources() -> Result<Vec<AudioSource>, CaptureError> {
    let mut groups = BTreeMap::<String, AudioSource>::new();
    for p in snapshot()? {
        let (id, name) = identity(&p.executable);
        let entry = groups.entry(id.clone()).or_insert(AudioSource {
            id,
            name,
            active: false,
            detail: Some("Аудіосесії Windows".into()),
        });
        entry.active |= p.active;
    }
    let mut result: Vec<_> = groups.into_values().collect();
    result.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(result)
}
pub fn resolve_source(id: &str) -> Result<ResolvedSource, CaptureError> {
    let processes: Vec<_> = snapshot()?
        .into_iter()
        .filter(|p| identity(&p.executable).0 == id)
        .collect();
    if processes.is_empty() {
        return Err(CaptureError(
            "Програма більше недоступна. Оновіть список джерел.",
        ));
    }
    Ok(ResolvedSource {
        id: id.into(),
        processes,
    })
}
pub fn source_still_valid(source: &ResolvedSource) -> bool {
    let current: Vec<_> = source
        .processes
        .iter()
        .filter_map(|p| process(p.pid as u32, false))
        .collect();
    unchanged(&source.processes, &current)
}

mod loopback;

pub struct CaptureHandle {
    source: ResolvedSource,
    worker: Option<loopback::Worker>,
}
impl CaptureHandle {
    pub fn prepare(source: ResolvedSource) -> Result<Self, CaptureError> {
        loopback::selected_root(&source)?;
        Ok(Self {
            source,
            worker: None,
        })
    }
    pub fn rate(&self) -> u32 {
        48000
    }
    pub fn start(
        &mut self,
        queues: CaptureQueues,
        meters: Arc<AudioMeters>,
        fault: FaultSignal,
    ) -> Result<(), CaptureError> {
        if self.worker.is_some() {
            return Err(CaptureError("Джерело вже запущене."));
        }
        self.worker = Some(loopback::Worker::start(
            self.source.clone(),
            queues,
            meters,
            fault,
        )?);
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), CaptureError> {
        if let Some(mut worker) = self.worker.take() {
            worker.stop()?;
        }
        Ok(())
    }
}
impl Drop for CaptureHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_is_case_insensitive_and_distinguishes_paths() {
        assert_eq!(
            identity(r"C:\Apps\Browser.exe").0,
            identity(r"c:\apps\browser.EXE").0
        );
        assert_ne!(
            identity(r"C:\Apps\Browser.exe").0,
            identity(r"D:\Apps\Browser.exe").0
        );
    }
    #[test]
    fn excludes_entire_self_tree() {
        assert_eq!(
            descendants(1, &[(3, 2), (2, 1), (4, 9)]),
            HashSet::from([1, 2, 3])
        );
    }
    #[test]
    fn invalid_source_fails_closed() {
        assert!(CaptureHandle::prepare(ResolvedSource {
            id: "x".into(),
            processes: vec![]
        })
        .is_err());
    }
}
