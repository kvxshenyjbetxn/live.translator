use serde::Serialize;
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioSource {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub detail: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessIdentity {
    pub object_id: u32,
    pub pid: i32,
    pub executable: String,
    pub started: (u64, u64),
    pub active: bool,
}
#[derive(Clone)]
pub struct ResolvedSource {
    pub id: String,
    pub processes: Vec<ProcessIdentity>,
}
pub fn source_identity(executable: &str) -> (String, String) {
    if let Some(index) = executable.find(".app/Contents/") {
        let root = &executable[..index + 4];
        let name = std::path::Path::new(root)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        (format!("app:{root}"), name)
    } else {
        let name = std::path::Path::new(executable)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        (format!("exe:{executable}"), name)
    }
}
/// Windows audio sessions are grouped by the full executable path, not process ancestry.
pub fn windows_source_identity(path: &str) -> (String, String) {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_owned();
    (format!("exe:{}", path.to_lowercase()), name)
}
pub fn unchanged(previous: &[ProcessIdentity], current: &[ProcessIdentity]) -> bool {
    !previous.is_empty()
        && previous.iter().all(|old| {
            current.iter().any(|now| {
                old.object_id == now.object_id
                    && old.pid == now.pid
                    && old.executable == now.executable
                    && old.started == now.started
            })
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_outer_app_not_similar_prefix() {
        assert_eq!(
            source_identity(
                "/Applications/Chrome.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"
            ),
            ("app:/Applications/Chrome.app".into(), "Chrome".into())
        );
        assert_ne!(
            source_identity("/Applications/Chrome.app/Contents/MacOS/Chrome").0,
            source_identity("/Applications/Chrome Beta.app/Contents/MacOS/Chrome").0
        );
        assert_eq!(source_identity("/usr/bin/afplay").0, "exe:/usr/bin/afplay");
    }
    #[test]
    fn identity_rejects_reused_pid_and_missing_member() {
        let p = ProcessIdentity {
            object_id: 1,
            pid: 10,
            executable: "/app".into(),
            started: (100, 1),
            active: true,
        };
        assert!(unchanged(
            std::slice::from_ref(&p),
            std::slice::from_ref(&p)
        ));
        assert!(!unchanged(
            std::slice::from_ref(&p),
            &[ProcessIdentity {
                started: (200, 1),
                ..p.clone()
            }]
        ));
        assert!(!unchanged(&[p], &[]));
    }
}
