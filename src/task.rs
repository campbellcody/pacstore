//! One owned result per worker. Polling never blocks the event loop.
use crate::error::{AppError, Result};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub struct Task<T>(Receiver<Result<T>>);

impl<T: Send + 'static> Task<T> {
    pub fn spawn(
        ctx: &eframe::egui::Context,
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker_ctx = ctx.clone();
        let failure_sender = sender.clone();
        if let Err(source) = std::thread::Builder::new()
            .name("pacstore-worker".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                    .unwrap_or(Err(AppError::WorkerStopped));
                if let Err(error) = &result {
                    log::error!("{error}");
                }
                let _ = sender.send(result);
                worker_ctx.request_repaint();
            })
        {
            let _ = failure_sender.send(Err(AppError::Io {
                operation: "Start background task",
                source,
            }));
            ctx.request_repaint();
        }
        Self(receiver)
    }

    pub fn poll(&self) -> Option<Result<T>> {
        match self.0.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(AppError::WorkerStopped)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_and_panicked_workers_report_errors() {
        let ctx = eframe::egui::Context::default();
        let failed = Task::<()>::spawn(&ctx, || Err(AppError::InvalidRequest("example".into())));
        assert!(matches!(
            failed.0.recv().unwrap(),
            Err(AppError::InvalidRequest(_))
        ));
        let panicked = Task::<()>::spawn(&ctx, || panic!("test worker panic"));
        assert!(matches!(
            panicked.0.recv().unwrap(),
            Err(AppError::WorkerStopped)
        ));
    }
}
