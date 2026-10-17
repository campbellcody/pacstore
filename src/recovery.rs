//! Contextual next steps; diagnostics remain visible alongside this guidance.
pub fn advice(message: &str) -> &'static str {
    let text = message.to_lowercase();
    let text = text.split(". review logs").next().unwrap_or(&text);
    let text = text.split(". check network").next().unwrap_or(text);
    if text.contains("database lock")
        || text.contains("transaction lock")
        || text.contains("locked")
        || text.contains("lock database")
    {
        "Wait for other package managers to finish, then reload and retry. Never remove an active database lock."
    } else if text.contains("not installed") && text.contains("helper") {
        "Install the transaction helper and its polkit policy using the setup instructions, then try again."
    } else if text.contains("supported begin") || text.contains("protocol") {
        "Rebuild and reinstall the application and transaction helper from the same version, then retry."
    } else if text.contains("authorization") || text.contains("polkit") {
        "Retry when your desktop authentication agent is running. Dismissing authentication leaves packages unchanged."
    } else if text.contains("signature") || text.contains("keyring") || text.contains("gpg") {
        "Check the system clock and Arch keyring using pacman, then retry. Keep signature verification enabled."
    } else if text.contains("disk")
        || text.contains("insufficient space")
        || text.contains("free space")
        || text.contains("no space")
    {
        "Free space on the installation and package-cache filesystems, reload package state, then retry."
    } else if text.contains("refresh repositories")
        || text.contains("network")
        || text.contains("mirror")
        || text.contains("download")
        || text.contains("timed out")
    {
        "Check your connection and configured mirrors, then retry. Cached browsing remains available offline."
    } else if text.contains("depend") || text.contains("conflict") {
        "Review the dependency or conflict details and proposed removals. Resolve the conflict using pacman before retrying; avoid force-overwriting files."
    } else if text.contains("launch") || text.contains("gio") {
        "Reload to check that the application is still installed. Launching requires a desktop session and the gio utility."
    } else if text.contains("configuration") || text.contains("preferences") {
        "Check the preferences file described in the README. Invalid files are preserved; correct or move the file before saving again."
    } else {
        "Review the operation log, reload package state, and address the reported cause before retrying. Failed transactions may have applied some changes."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_matches_the_failure_without_suggesting_unsafe_shortcuts() {
        assert!(advice("database is locked").contains("Never remove"));
        assert!(advice("invalid signature").contains("verification enabled"));
        assert!(advice("Insufficient disk space").contains("Free space"));
    }
}
