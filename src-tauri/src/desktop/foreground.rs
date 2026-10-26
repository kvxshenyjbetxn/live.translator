fn identity(pid: u32, executable: Option<&str>) -> Result<String, String> {
    if pid == std::process::id() {
        return Err(
            "Live Translator не може захоплювати власне вікно. Перейдіть у програму-джерело."
                .into(),
        );
    }
    let path = executable
        .filter(|path| !path.is_empty())
        .ok_or("Не вдалося визначити активну програму.")?;
    Ok(translator_core::sources::source_identity(path).0)
}

#[cfg(target_os = "macos")]
pub fn current() -> Result<String, String> {
    // NSWorkspace inspects application metadata; it does not capture screen contents
    // or require Accessibility automation. Snapshot before any window is focused.
    objc2::rc::autoreleasepool(|_| {
        let app = objc2_app_kit::NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .ok_or("Немає активної програми.")?;
        let path = app
            .executableURL()
            .and_then(|url| url.path())
            .map(|s| s.to_string());
        identity(app.processIdentifier() as u32, path.as_deref())
    })
}
#[cfg(not(target_os = "macos"))]
pub fn current() -> Result<String, String> {
    Err("Визначення активної програми на цій платформі ще не підтримується. Оберіть режим «Остання захоплена».".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_rejects_self_and_unknown_without_guessing() {
        assert!(identity(
            std::process::id(),
            Some("/Applications/Me.app/Contents/MacOS/Me")
        )
        .is_err());
        assert!(identity(1234, None).is_err());
        assert_eq!(
            identity(1234, Some("/Applications/Chrome.app/Contents/MacOS/Chrome")).unwrap(),
            "app:/Applications/Chrome.app"
        );
    }
}
