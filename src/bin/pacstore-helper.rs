#[path = "../protocol.rs"]
mod protocol;

use alpm::{Alpm, DownloadEvent, Question, TransFlag};
use protocol::{Change, Command, Event, Operation, Plan};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Result<T> = std::result::Result<T, String>;

// Broken GUI pipes must never panic during a package commit.
fn emit(event: Event) -> bool {
    protocol::write(&mut std::io::stdout().lock(), &event).is_ok()
}
fn input() -> Result<Command> {
    protocol::read(&mut std::io::stdin().lock())
}
fn free_space(path: &str) -> Option<u64> {
    let path = std::ffi::CString::new(path).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: pointers are valid and statvfs initializes the struct on success.
    if unsafe { libc::statvfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some(info.f_bavail.saturating_mul(info.f_frsize))
}

fn callbacks(handle: &Alpm, canceled: Rc<Cell<bool>>, decisions: Rc<RefCell<Vec<String>>>) {
    handle.set_log_cb((), |_, message, _| {
        emit(Event::Log {
            message: message.trim().into(),
        });
    });
    handle.set_progress_cb((), |kind, package, percent, total, current, _| {
        emit(Event::Progress {
            message: format!("{kind:?}: {package} ({current}/{total})"),
            percent: percent as f32,
        });
    });
    handle.set_dl_cb((), |file, event, _| {
        if let DownloadEvent::Progress(progress) = event.event() {
            let percent = if progress.total > 0 {
                progress.downloaded as f32 / progress.total as f32 * 100.0
            } else {
                0.0
            };
            emit(Event::Progress {
                message: format!(
                    "Downloading {file}: {} / {} bytes",
                    progress.downloaded, progress.total
                ),
                percent,
            });
        }
    });
    handle.set_event_cb((), |event, _| {
        let message = match event.event() {
            alpm::Event::ScriptletInfo(info) => info.line().trim().to_owned(),
            alpm::Event::HookRunStart(info) => format!(
                "Running hook {}: {}",
                info.name(),
                info.desc().unwrap_or("")
            ),
            _ => format!("{:?}", event.event_type()),
        };
        emit(Event::Log { message });
    });
    handle.set_question_cb(
        (canceled, decisions),
        |mut question, (canceled, decisions)| {
            if let Question::RemovePkgs(mut skip) = question.question() {
                skip.set_skip(false);
                emit(Event::Log { message: "Unresolvable dependencies cannot be skipped during a full upgrade; preparation will fail.".into() });
                return;
            }
            let message = match question.question() {
                Question::InstallIgnorepkg(q) => {
                    format!("Install ignored package {}?", q.pkg().name())
                }
                Question::Replace(q) => format!(
                    "Replace {} with {}/{}?",
                    q.oldpkg().name(),
                    q.newdb().name(),
                    q.newpkg().name()
                ),
                Question::Conflict(q) => format!(
                    "{} conflicts with {} ({}). Remove the conflicting installed package?",
                    q.conflict().package1().name(),
                    q.conflict().package2().name(),
                    q.conflict().reason()
                ),
                Question::Corrupted(q) => format!(
                    "Remove invalid cached file {} ({})?",
                    q.filepath(),
                    q.reason()
                ),
                Question::RemovePkgs(q) => format!(
                    "Skip packages with unresolvable dependencies: {}?",
                    q.packages()
                        .iter()
                        .map(|p| p.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Question::SelectProvider(q) => format!("Choose a provider for {}", q.depend()),
                Question::ImportKey(q) => {
                    format!("Import signing key {} ({})?", q.fingerprint(), q.uid())
                }
            };
            let choices = if let Question::SelectProvider(provider) = question.question() {
                provider
                    .providers()
                    .iter()
                    .map(|p| format!("{} {}", p.name(), p.version()))
                    .collect()
            } else {
                vec!["No".into(), "Yes".into()]
            };
            if !emit(Event::Question { message: message.clone(), choices: choices.clone() }) {
                canceled.set(true);
                if let Question::SelectProvider(mut provider) = question.question() { provider.set_index(-1); }
                else { question.set_answer(false); }
                return;
            }
            let choice = match input() {
                Ok(Command::Answer { choice }) if choice < choices.len() => Some(choice),
                _ => {
                    canceled.set(true);
                    None
                }
            };
            if let Question::SelectProvider(mut provider) = question.question() {
                provider.set_index(choice.map(|i| i as i32).unwrap_or(-1));
            } else {
                question.set_answer(choice == Some(1));
            }
            if let Some(choice) = choice {
                decisions
                    .borrow_mut()
                    .push(format!("{message}: {}", choices[choice]));
            }
        },
    );
}

fn prepare(handle: &mut Alpm, operation: &Operation) -> Result<()> {
    if matches!(operation, Operation::Install { .. } | Operation::Upgrade {}) {
        emit(Event::Status {
            message: "Refreshing repositories before a full system upgrade…".into(),
        });
        handle.syncdbs_mut().update(false).map_err(|e| {
            format!("Refresh repositories: {e}. Check network, mirrors, signatures, and keyring.")
        })?;
    }
    let flags = match operation {
        Operation::Remove { cascade: true, .. } => TransFlag::CASCADE,
        Operation::Remove { .. } | Operation::CleanOrphans {} => TransFlag::NONE,
        _ => TransFlag::NEEDED,
    };
    handle.trans_init(flags).map_err(|e| format!("Acquire transaction lock: {e}. If another package manager is running, wait for it; never remove its lock."))?;
    match operation {
        Operation::CleanOrphans {} => {
            for package in handle.localdb().pkgs() {
                if package.reason() == alpm::PackageReason::Depend
                    && package.required_by().is_empty()
                    && package.optional_for().is_empty()
                {
                    handle
                        .trans_remove_pkg(package)
                        .map_err(|e| format!("Prepare orphan cleanup: {e}"))?;
                }
            }
        }
        Operation::Remove { target, .. } => {
            let package = handle
                .localdb()
                .pkg(target.as_str())
                .map_err(|e| format!("Installed package {target}: {e}"))?;
            handle
                .trans_remove_pkg(package)
                .map_err(|e| format!("Remove {target}: {e}"))?;
        }
        Operation::Install { targets } => {
            for target in targets {
                let (repository, name) = target
                    .split_once('/')
                    .map(|(r, n)| (Some(r), n))
                    .unwrap_or((None, target.as_str()));
                let package = handle
                    .syncdbs()
                    .iter()
                    .filter(|db| {
                        repository.is_none_or(|r| r == db.name())
                            && db
                                .usage()
                                .is_ok_and(|usage| usage.contains(alpm::Usage::INSTALL))
                    })
                    .find_map(|db| db.pkg(name).ok())
                    .ok_or_else(|| {
                        format!("Package {target} is unavailable in configured repositories")
                    })?;
                if !handle.trans_add().iter().any(|p| p.name() == name) {
                    handle
                        .trans_add_pkg(package)
                        .map_err(|e| format!("Add {target}: {e}"))?;
                }
            }
            handle
                .sync_sysupgrade(false)
                .map_err(|e| format!("Resolve full system upgrade: {e}"))?;
        }
        Operation::Upgrade {} => handle
            .sync_sysupgrade(false)
            .map_err(|e| format!("Resolve full system upgrade: {e}"))?,
    }
    handle
        .trans_prepare()
        .map_err(|e| format!("Resolve dependencies/conflicts: {e:?}"))
}

fn plan(handle: &Alpm, decisions: Vec<String>) -> Plan {
    let mut changes = Vec::new();
    let mut download_bytes = 0;
    let mut disk_delta = 0;
    for p in handle.trans_add() {
        let old = handle.localdb().pkg(p.name()).ok();
        disk_delta += p.isize() - old.map(|p| p.isize()).unwrap_or(0);
        download_bytes += p.download_size();
        changes.push(Change {
            name: p.name().into(),
            old: old.map(|p| p.version().to_string()),
            new: Some(p.version().to_string()),
        });
    }
    for p in handle.trans_remove() {
        if !handle.trans_add().iter().any(|add| add.name() == p.name()) {
            disk_delta -= p.isize();
            changes.push(Change {
                name: p.name().into(),
                old: Some(p.version().to_string()),
                new: None,
            });
        }
    }
    Plan {
        changes,
        download_bytes,
        disk_delta,
        root_free: free_space(handle.root()),
        cache_free: handle.cachedirs().first().and_then(free_space),
        decisions,
    }
}

fn run() -> Result<String> {
    if std::env::args_os().len() != 1 {
        return Err("The helper accepts only its stdin protocol, not command-line options".into());
    }
    // SAFETY: geteuid has no preconditions and does not modify process state.
    if unsafe { libc::geteuid() } != 0
        || std::env::var("PKEXEC_UID")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .is_none_or(|uid| uid == 0)
    {
        return Err(
            "Launch the installed helper through polkit from an unprivileged session".into(),
        );
    }
    let operation = match input()? {
        Command::Begin {
            version: protocol::VERSION,
            operation,
        } => operation,
        _ => return Err("Expected a supported Begin request".into()),
    };
    operation.validate()?;
    let conf = alpm_utils::config::Config::options()
        .pacman_conf_bin("/usr/bin/pacman-conf")
        .pacman_conf("/etc/pacman.conf")
        .read()
        .map_err(|e| e.to_string())?;
    if !conf.xfer_command.is_empty() {
        return Err("Custom XferCommand is not supported by this helper. Use pacman for this configuration.".into());
    }
    let mut handle = alpm_utils::alpm_with_conf(&conf).map_err(|e| e.to_string())?;
    handle
        .set_ignoregroups(conf.ignore_group.iter())
        .map_err(|e| e.to_string())?;
    handle.set_check_space(true);
    let canceled = Rc::new(Cell::new(false));
    let decisions = Rc::new(RefCell::new(Vec::new()));
    callbacks(&handle, canceled.clone(), decisions.clone());
    // Check before refresh; libalpm also acquires/checks the real transaction lock.
    if std::path::Path::new(handle.lockfile()).exists() {
        return Err("Pacman's database is locked by another package manager. Wait for it; never delete the lock automatically.".into());
    }
    let result = (|| {
        prepare(&mut handle, &operation)?;
        if canceled.get() {
            return Err("Preparation canceled; no packages changed".into());
        }
        let plan = plan(&handle, decisions.borrow().clone());
        for change in &plan.changes {
            if change.new.is_none() && conf.hold_pkg.iter().any(|p| p == &change.name) {
                return Err(format!(
                    "{} is protected by HoldPkg. Use pacman to review this removal.",
                    change.name
                ));
            }
        }
        if plan.changes.is_empty() {
            return Ok("Nothing to do".to_owned());
        }
        if plan
            .root_free
            .is_some_and(|free| plan.disk_delta > 0 && free < plan.disk_delta as u64)
        {
            return Err("Insufficient space on the installation filesystem".into());
        }
        if plan
            .cache_free
            .is_some_and(|free| plan.download_bytes > 0 && free < plan.download_bytes as u64)
        {
            return Err("Insufficient space on the package cache filesystem".into());
        }
        if !emit(Event::Prepared { plan }) {
            return Err("Could not deliver the prepared plan; no package changes started.".into());
        }
        if !matches!(input(), Ok(Command::Confirm {})) {
            return Err("Transaction canceled before commit; no packages changed".into());
        }
        emit(Event::Committing);
        // Once commit begins, no UI cancellation or disconnect kills this process.
        handle.trans_commit().map_err(|e| format!("Transaction failed: {e}. Review logs, free space, signatures, keyring, and mirrors before retrying."))?;
        Ok("Package transaction completed".to_owned())
    })();
    let release = handle.trans_release();
    let message = result?;
    release.map_err(|e| {
        format!("Transaction ended but its database lock could not be released: {e}")
    })?;
    Ok(message)
}

fn main() {
    match run() {
        Ok(message) => {
            emit(Event::Finished {
                success: true,
                message,
            });
        }
        Err(message) => {
            emit(Event::Finished {
                success: false,
                message,
            });
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Root(std::path::PathBuf);
    impl Root {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "pacstore-alpm-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            for part in [
                "db/local",
                "cache",
                "gpg",
                "package/usr/share/pacstore-fixture",
            ] {
                std::fs::create_dir_all(path.join(part)).unwrap();
            }
            Self(path)
        }
        fn handle(&self) -> Alpm {
            let root = self.0.to_str().unwrap();
            let mut handle = Alpm::new(format!("{root}/"), format!("{root}/db/")).unwrap();
            handle.add_cachedir(format!("{root}/cache/")).unwrap();
            handle.set_gpgdir(format!("{root}/gpg/")).unwrap();
            handle.add_architecture("any").unwrap();
            handle
        }
        fn archive(&self) -> String {
            self.archive_package("pacstore-fixture", "1-1", None)
        }
        fn archive_package(&self, name: &str, version: &str, dependency: Option<&str>) -> String {
            self.archive_with_optional(name, version, dependency, None)
        }
        fn archive_with_optional(
            &self,
            name: &str,
            version: &str,
            dependency: Option<&str>,
            optional: Option<&str>,
        ) -> String {
            let directory = self.0.join(format!("package-{name}-{version}"));
            let payload = directory.join(format!("usr/share/{name}"));
            std::fs::create_dir_all(&payload).unwrap();
            let mut metadata = format!(
                "pkgname = {name}\npkgbase = {name}\npkgver = {version}\npkgdesc = Disposable test fixture\narch = any\nsize = 8\nbuilddate = 1\npackager = Pacstore tests\n"
            );
            if let Some(dependency) = dependency {
                metadata.push_str(&format!("depend = {dependency}\n"));
            }
            if let Some(optional) = optional {
                metadata.push_str(&format!("optdepend = {optional}: optional feature\n"));
            }
            std::fs::write(directory.join(".PKGINFO"), metadata).unwrap();
            std::fs::write(payload.join("data"), version).unwrap();
            let archive = self.0.join(format!("{name}-{version}.pkg.tar.gz"));
            assert!(
                std::process::Command::new("bsdtar")
                    .args([
                        "-czf",
                        archive.to_str().unwrap(),
                        "-C",
                        directory.to_str().unwrap(),
                        ".PKGINFO",
                        "usr"
                    ])
                    .status()
                    .unwrap()
                    .success()
            );
            archive.to_string_lossy().into_owned()
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn disposable_root_previews_cancels_installs_and_removes() {
        let root = Root::new();
        let archive = root.archive();
        let mut handle = root.handle();
        let flags = TransFlag::NO_SCRIPTLET | TransFlag::NO_HOOKS;
        handle.trans_init(flags).unwrap();
        let package = handle
            .pkg_load(archive.as_str(), true, alpm::SigLevel::NONE)
            .unwrap();
        handle.trans_add_pkg(package).unwrap();
        handle.trans_prepare().unwrap();
        let preview = plan(&handle, Vec::new());
        assert_eq!(preview.changes[0].name, "pacstore-fixture");
        assert!(preview.changes[0].old.is_none());
        // Canceling a prepared transaction writes no package files.
        handle.trans_release().unwrap();
        assert!(!root.0.join("usr/share/pacstore-fixture/data").exists());
        handle.trans_init(flags).unwrap();
        let package = handle
            .pkg_load(archive.as_str(), true, alpm::SigLevel::NONE)
            .unwrap();
        handle.trans_add_pkg(package).unwrap();
        handle.trans_prepare().unwrap();
        handle.trans_commit().unwrap();
        handle.trans_release().unwrap();
        assert!(root.0.join("usr/share/pacstore-fixture/data").exists());
        handle.trans_init(flags).unwrap();
        handle
            .trans_remove_pkg(handle.localdb().pkg("pacstore-fixture").unwrap())
            .unwrap();
        handle.trans_prepare().unwrap();
        assert!(plan(&handle, Vec::new()).changes[0].new.is_none());
        handle.trans_commit().unwrap();
        handle.trans_release().unwrap();
        assert!(!root.0.join("usr/share/pacstore-fixture/data").exists());
    }
    #[test]
    fn dependent_removal_requires_explicit_cascade() {
        let root = Root::new();
        let mut handle = root.handle();
        let flags = TransFlag::NO_SCRIPTLET | TransFlag::NO_HOOKS;
        handle.trans_init(flags).unwrap();
        for archive in [
            root.archive(),
            root.archive_package("pacstore-dependent", "1-1", Some("pacstore-fixture")),
        ] {
            handle
                .trans_add_pkg(
                    handle
                        .pkg_load(archive.as_str(), true, alpm::SigLevel::NONE)
                        .unwrap(),
                )
                .unwrap();
        }
        handle.trans_prepare().unwrap();
        handle.trans_commit().unwrap();
        handle.trans_release().unwrap();
        assert!(
            prepare(
                &mut handle,
                &Operation::Remove {
                    target: "pacstore-fixture".into(),
                    cascade: false
                }
            )
            .is_err()
        );
        handle.trans_release().unwrap();
        prepare(
            &mut handle,
            &Operation::Remove {
                target: "pacstore-fixture".into(),
                cascade: true,
            },
        )
        .unwrap();
        let preview = plan(&handle, Vec::new());
        assert_eq!(preview.changes.len(), 2);
        assert!(preview.changes.iter().all(|c| c.new.is_none()));
        handle.trans_release().unwrap();
    }

    #[test]
    fn install_and_full_upgrade_resolve_from_a_disposable_file_repository() {
        let root = Root::new();
        let repository = root.0.join("repo");
        std::fs::create_dir_all(&repository).unwrap();
        let publish = |archive: String| {
            let source = std::path::Path::new(&archive);
            let target = repository.join(source.file_name().unwrap());
            std::fs::copy(source, &target).unwrap();
            let output = std::process::Command::new("repo-add")
                .arg("--quiet")
                .arg(repository.join("fixture.db.tar.gz"))
                .arg(target)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        publish(root.archive());
        let mut handle = root.handle();
        handle
            .register_syncdb_mut("fixture", alpm::SigLevel::NONE)
            .unwrap()
            .set_servers([format!("file://{}", repository.display())].iter())
            .unwrap();
        prepare(
            &mut handle,
            &Operation::Install {
                targets: vec!["fixture/pacstore-fixture".into()],
            },
        )
        .unwrap();
        assert_eq!(
            plan(&handle, Vec::new()).changes[0].new.as_deref(),
            Some("1-1")
        );
        handle.trans_commit().unwrap();
        handle.trans_release().unwrap();
        publish(root.archive_package("pacstore-fixture", "2-1", None));
        handle.syncdbs_mut().update(true).unwrap();
        prepare(&mut handle, &Operation::Upgrade {}).unwrap();
        let preview = plan(&handle, Vec::new());
        assert_eq!(preview.changes[0].old.as_deref(), Some("1-1"));
        assert_eq!(preview.changes[0].new.as_deref(), Some("2-1"));
        handle.trans_commit().unwrap();
        handle.trans_release().unwrap();
        assert_eq!(
            std::fs::read_to_string(root.0.join("usr/share/pacstore-fixture/data")).unwrap(),
            "2-1"
        );
    }

    #[test]
    fn unsigned_archive_is_rejected_when_signatures_are_required() {
        let root = Root::new();
        let archive = root.archive();
        let handle = root.handle();
        assert!(
            handle
                .pkg_load(archive.as_str(), true, alpm::SigLevel::PACKAGE)
                .is_err()
        );
        assert!(!root.0.join("usr/share/pacstore-fixture/data").exists());
    }

    #[test]
    fn unowned_file_conflicts_leave_existing_files_intact() {
        let root = Root::new();
        let archive = root.archive();
        let destination = root.0.join("usr/share/pacstore-fixture");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("data"), "user data").unwrap();
        let mut handle = root.handle();
        handle
            .trans_init(TransFlag::NO_SCRIPTLET | TransFlag::NO_HOOKS)
            .unwrap();
        handle
            .trans_add_pkg(
                handle
                    .pkg_load(archive.as_str(), true, alpm::SigLevel::NONE)
                    .unwrap(),
            )
            .unwrap();
        handle.trans_prepare().unwrap();
        assert!(handle.trans_commit().is_err());
        handle.trans_release().unwrap();
        assert_eq!(
            std::fs::read_to_string(destination.join("data")).unwrap(),
            "user data"
        );
    }

    #[test]
    fn orphan_cleanup_preserves_manual_required_and_optional_packages() {
        let root = Root::new();
        let mut handle = root.handle();
        handle
            .trans_init(TransFlag::NO_SCRIPTLET | TransFlag::NO_HOOKS)
            .unwrap();
        for archive in [
            root.archive(),
            root.archive_package("pacstore-unused", "1-1", None),
            root.archive_package("pacstore-required", "1-1", None),
            root.archive_package("pacstore-optional", "1-1", None),
            root.archive_with_optional(
                "pacstore-anchor",
                "1-1",
                Some("pacstore-required"),
                Some("pacstore-optional"),
            ),
        ] {
            handle
                .trans_add_pkg(
                    handle
                        .pkg_load(archive.as_str(), true, alpm::SigLevel::NONE)
                        .unwrap(),
                )
                .unwrap();
        }
        handle.trans_prepare().unwrap();
        handle.trans_commit().unwrap();
        handle.trans_release().unwrap();
        for name in ["pacstore-unused", "pacstore-required", "pacstore-optional"] {
            handle
                .localdb()
                .pkg(name)
                .unwrap()
                .set_reason(alpm::PackageReason::Depend)
                .unwrap();
        }
        prepare(&mut handle, &Operation::CleanOrphans {}).unwrap();
        let preview = plan(&handle, Vec::new());
        assert_eq!(preview.changes.len(), 1);
        assert_eq!(preview.changes[0].name, "pacstore-unused");
        assert!(preview.changes[0].new.is_none());
        handle.trans_commit().unwrap();
        handle.trans_release().unwrap();
        assert!(handle.localdb().pkg("pacstore-unused").is_err());
        for name in [
            "pacstore-fixture",
            "pacstore-anchor",
            "pacstore-required",
            "pacstore-optional",
        ] {
            assert!(handle.localdb().pkg(name).is_ok());
        }
    }

    #[test]
    fn another_handle_cannot_take_the_database_lock() {
        let root = Root::new();
        let mut first = root.handle();
        first.trans_init(TransFlag::NONE).unwrap();
        let second = root.handle();
        assert!(second.trans_init(TransFlag::NONE).is_err());
        assert!(std::path::Path::new(first.lockfile()).exists());
        first.trans_release().unwrap();
    }
}
