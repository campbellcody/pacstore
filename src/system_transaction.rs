//! Own the helper process in workers. Dropping the UI never kills a commit.
use crate::{
    error::{AppError, Result},
    protocol::{self, Command, Event, Operation, Plan},
};
use std::{
    collections::VecDeque,
    io::BufReader,
    os::unix::fs::MetadataExt,
    process::{Child, Command as Process, Stdio},
    sync::mpsc::{self, Receiver, Sender},
};

const HELPER: &str = "/usr/lib/pacstore/pacstore-helper";

pub struct Session {
    sender: Sender<Command>,
    events: Receiver<Event>,
    pub plan: Option<Plan>,
    pub question: Option<(String, Vec<String>)>,
    pub committing: bool,
    pub cancel_requested: bool,
    pub status: String,
    pub progress: f32,
    pub logs: VecDeque<String>,
    pub outcome: Option<(bool, String)>,
}

fn trusted_helper() -> Result<()> {
    let path = std::path::Path::new(HELPER).canonicalize().map_err(|_| AppError::Backend(
        "Transaction helper is not installed. Build and install pacstore-helper and its polkit policy; see docs/transactions.md.".into()))?;
    for ancestor in path
        .ancestors()
        .chain(std::path::Path::new(HELPER).ancestors())
    {
        let metadata = ancestor.metadata().map_err(|source| AppError::Io {
            operation: "Inspect installed helper",
            source,
        })?;
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(AppError::Backend("The installed helper and its parent directories must be root-owned and not writable by other users.".into()));
        }
    }
    Ok(())
}

impl Session {
    pub fn start(ctx: &eframe::egui::Context, operation: Operation) -> Result<Self> {
        operation.validate().map_err(AppError::InvalidRequest)?;
        trusted_helper()?;
        Self::spawn(ctx, operation, || {
            Process::new("/usr/bin/pkexec")
                .arg(HELPER)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
    }

    fn spawn(
        ctx: &eframe::egui::Context,
        operation: Operation,
        launch: impl FnOnce() -> std::io::Result<Child> + Send + 'static,
    ) -> Result<Self> {
        let (sender, commands) = mpsc::channel();
        let (events, receiver) = mpsc::sync_channel(2048);
        sender
            .send(Command::Begin {
                version: protocol::VERSION,
                operation,
            })
            .map_err(|_| AppError::WorkerStopped)?;
        let ctx = ctx.clone();
        std::thread::Builder::new().name("pacstore-transaction".into()).spawn(move || {
            let send = |event| {
                if matches!(event, Event::Log { .. } | Event::Progress { .. }) { let _ = events.try_send(event); }
                else { let _ = events.send(event); }
                ctx.request_repaint();
            };
            let mut child = match launch() {
                Ok(child) => child,
                Err(error) => { send(Event::Finished { success: false, message: format!("Could not start polkit authorization: {error}") }); return; }
            };
            let mut stdin = child.stdin.take().expect("piped helper stdin");
            let writer = std::thread::spawn(move || {
                for command in commands {
                    if protocol::write(&mut stdin, &command).is_err() { break; }
                }
            });
            let stderr = child.stderr.take().expect("piped helper stderr");
            let err_sender = events.clone(); let err_ctx = ctx.clone();
            let errors = std::thread::spawn(move || {
                use std::io::BufRead;
                for (index, line) in BufReader::new(stderr).lines().map_while(std::result::Result::ok).enumerate() {
                    if index >= 500 { continue; }
                    let _ = err_sender.try_send(Event::Log { message: line }); err_ctx.request_repaint();
                }
            });
            let mut output = BufReader::new(child.stdout.take().expect("piped helper stdout"));
            let mut finished = None;
            while let Ok(event) = protocol::read::<Event>(&mut output) {
                if matches!(event, Event::Finished { .. }) { finished = Some(event); }
                else { send(event); }
            }
            let status = child.wait();
            let _ = errors.join();
            // Wait for process exit before releasing the UI's busy state/lock.
            let event = match (finished, status) {
                (Some(event @ Event::Finished { success: false, .. }), _) => event,
                (Some(event), Ok(status)) if status.success() => event,
                (_, status) => Event::Finished { success: false, message: match status.ok().and_then(|s| s.code()) {
                    Some(126) => "Authorization was canceled. No package transaction started.".into(),
                    Some(127) => "Authorization failed or no polkit agent is available.".into(),
                    _ => "The helper exited unexpectedly. Check transaction logs and pacman state before retrying; a transaction may have partially completed.".into(),
                } },
            };
            send(event);
            // Detach writer: Session drop closes its command channel after Finished.
            drop(writer);
        }).map_err(|source| AppError::Io { operation: "Start transaction worker", source })?;
        Ok(Self {
            sender,
            events: receiver,
            plan: None,
            question: None,
            committing: false,
            cancel_requested: false,
            status: "Waiting for polkit authorization…".into(),
            progress: 0.0,
            logs: VecDeque::new(),
            outcome: None,
        })
    }

    pub fn send(&mut self, command: Command) {
        if matches!(command, Command::Cancel {}) {
            self.cancel_requested = true;
            self.plan = None;
            self.question = None;
        }
        if matches!(command, Command::Confirm {}) {
            self.committing = true;
            self.plan = None;
        }
        if let Command::Answer { choice } = &command {
            if let Some((_, choices)) = &self.question
                && let Some(label) = choices.get(*choice)
            {
                self.logs.push_back(format!("Answer: {label}"));
            }
            self.question = None;
        }
        if self.sender.send(command).is_err() {
            self.status = "Helper connection closed; waiting for process exit…".into();
        }
    }

    pub fn poll(&mut self) {
        // Bound each pass so streaming progress never starves the event loop.
        for _ in 0..256 {
            let event = match self.events.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.outcome.is_none() {
                        self.outcome = Some((false, "Transaction worker stopped unexpectedly. Check pacman state before retrying.".into()));
                    }
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            };
            match event {
                Event::Status { message } => self.status = message,
                Event::Log { message } => {
                    self.logs.push_back(message);
                    while self.logs.len() > 500 {
                        self.logs.pop_front();
                    }
                }
                Event::Progress { message, percent } => {
                    self.status = message;
                    self.progress = (percent / 100.0).clamp(0.0, 1.0);
                }
                Event::Question { message, choices } => {
                    if self.cancel_requested {
                        self.send(Command::Cancel {});
                    } else {
                        self.status = "Waiting for your answer…".into();
                        self.logs.push_back(message.clone());
                        self.question = Some((message, choices));
                    }
                }
                Event::Prepared { plan } => {
                    if self.cancel_requested {
                        self.send(Command::Cancel {});
                    } else {
                        self.status = "Prepared changes are ready for review".into();
                        self.plan = Some(plan);
                    }
                }
                Event::Committing => {
                    self.committing = true;
                    self.status = "Applying package changes. Keep the system running.".into();
                }
                Event::Finished { success, message } => {
                    self.logs.push_back(message.clone());
                    self.outcome = Some((success, message));
                    self.question = None;
                    self.plan = None;
                }
            }
        }
        while self.logs.len() > 500 {
            self.logs.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fake(script: &'static str) -> Session {
        Session::spawn(
            &eframe::egui::Context::default(),
            Operation::Upgrade {},
            move || {
                Process::new("/bin/sh")
                    .arg("-c")
                    .arg(script)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
            },
        )
        .unwrap()
    }
    fn until(session: &mut Session, predicate: impl Fn(&Session) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            session.poll();
            if predicate(session) {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "helper timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn plan_waits_for_confirmation_then_commits_and_exits() {
        let mut session = fake(
            r#"read request
printf '%s\n' '{"type":"prepared","plan":{"changes":[{"name":"fixture","old":null,"new":"1"}],"download_bytes":10,"disk_delta":20,"root_free":1000,"cache_free":1000,"decisions":[]}}'
read confirmation
case "$confirmation" in *confirm*) ;; *) exit 1;; esac
printf '%s\n' '{"type":"committing"}' '{"type":"progress","message":"Applying fixture","percent":75}' '{"type":"finished","success":true,"message":"Completed"}'
"#,
        );
        until(&mut session, |s| s.plan.is_some());
        assert!(!session.committing);
        assert!(session.outcome.is_none());
        session.send(Command::Confirm {});
        until(&mut session, |s| s.outcome.is_some());
        assert!(session.outcome.as_ref().unwrap().0);
        assert_eq!(session.progress, 0.75);
    }
    #[test]
    fn cancellation_and_authorization_failure_are_reported() {
        for (script, message) in [
            ("exit 126", "canceled"),
            ("exit 127", "Authorization failed"),
        ] {
            let mut session = fake(script);
            until(&mut session, |s| s.outcome.is_some());
            assert!(!session.outcome.as_ref().unwrap().0);
            assert!(session.outcome.as_ref().unwrap().1.contains(message));
        }
        let mut session = fake(
            r#"read request
printf '%s\n' '{"type":"question","message":"Replace fixture?","choices":["No","Yes"]}'
read answer
printf '%s\n' '{"type":"finished","success":false,"message":"Canceled before commit"}'
"#,
        );
        until(&mut session, |s| s.question.is_some());
        session.send(Command::Cancel {});
        until(&mut session, |s| s.outcome.is_some());
        assert!(!session.committing);
        assert!(!session.outcome.as_ref().unwrap().0);
    }
}

#[cfg(test)]
mod disconnect_tests {
    use super::*;
    #[test]
    fn dropping_the_gui_session_does_not_kill_a_started_commit() {
        let marker = std::env::temp_dir().join(format!(
            "pacstore-commit-marker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let child_marker = marker.clone();
        let mut session = Session::spawn(&eframe::egui::Context::default(), Operation::Upgrade {}, move || {
            Process::new("/bin/sh").args(["-c", r#"read request
printf '%s\n' '{"type":"prepared","plan":{"changes":[],"download_bytes":0,"disk_delta":0,"root_free":null,"cache_free":null,"decisions":[]}}'
read confirmation
printf '%s\n' '{"type":"committing"}'
sleep 0.1
printf done > "$1"
printf '%s\n' '{"type":"finished","success":true,"message":"Completed"}'
"#, "fixture"]).arg(child_marker).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        }).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while session.plan.is_none() {
            session.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        session.send(Command::Confirm {});
        while session.status != "Applying package changes. Keep the system running." {
            session.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        drop(session);
        while !marker.exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::fs::remove_file(marker).unwrap();
    }
}
