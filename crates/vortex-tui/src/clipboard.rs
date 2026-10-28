//! 系统剪贴板在独立线程上访问；保留所有者，避免 X11 内容在一次复制后立即失效。
use std::sync::mpsc::{self, SyncSender};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

pub(crate) const MAX_COPY_BYTES: usize = 1024 * 1024;
const UNAVAILABLE: &str = "系统剪贴板不可用；可用终端原生复制（Kitty：Shift 拖选，Ctrl+Shift+C）";

pub(crate) struct Clipboard {
    requests: Option<SyncSender<String>>,
    pub(crate) results: UnboundedReceiver<Result<(), &'static str>>,
    replies: tokio::sync::mpsc::UnboundedSender<Result<(), &'static str>>,
    pending: bool,
}

impl Default for Clipboard {
    fn default() -> Self {
        let (replies, results) = unbounded_channel();
        Self {
            requests: None,
            results,
            replies,
            pending: false,
        }
    }
}

impl Clipboard {
    pub(crate) fn copy(&mut self, text: String) -> Result<(), &'static str> {
        if text.len() > MAX_COPY_BYTES {
            return Err("复制内容超过 1 MiB 限制，请缩小选区");
        }
        if self.pending {
            return Err("正在复制，请稍后重试");
        }
        if self.requests.is_none() {
            let mut clipboard = None;
            let sender = self.start_worker(move |text| {
                if clipboard.is_none() {
                    clipboard = Some(arboard::Clipboard::new().map_err(|_| UNAVAILABLE)?);
                }
                clipboard
                    .as_mut()
                    .unwrap()
                    .set_text(text)
                    .map_err(|_| UNAVAILABLE)
            })?;
            self.requests = Some(sender);
        }
        self.requests
            .as_ref()
            .unwrap()
            .try_send(text)
            .map_err(|_| UNAVAILABLE)?;
        self.pending = true;
        Ok(())
    }

    fn start_worker(
        &self,
        mut write: impl FnMut(String) -> Result<(), &'static str> + Send + 'static,
    ) -> Result<SyncSender<String>, &'static str> {
        let (sender, receiver) = mpsc::sync_channel::<String>(1);
        let replies = self.replies.clone();
        std::thread::Builder::new()
            .name("vortex-clipboard".into())
            .spawn(move || {
                while let Ok(text) = receiver.recv() {
                    if replies.send(write(text)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|_| "无法启动剪贴板线程")?;
        Ok(sender)
    }

    pub(crate) fn completed(&mut self) {
        self.pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn explicit_copy_runs_off_thread_and_reports_success_without_losing_text() {
        let mut service = Clipboard::default();
        let (sent, received) = tokio::sync::oneshot::channel();
        let mut sent = Some(sent);
        service.requests = Some(
            service
                .start_worker(move |text| {
                    sent.take()
                        .unwrap()
                        .send((text, std::thread::current().name().map(str::to_owned)))
                        .unwrap();
                    Ok(())
                })
                .unwrap(),
        );
        service.copy("中文\ncode();".into()).unwrap();
        assert!(service.copy("another".into()).is_err());
        let (text, thread) = tokio::time::timeout(Duration::from_secs(5), received)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(text, "中文\ncode();");
        assert_eq!(thread.as_deref(), Some("vortex-clipboard"));
        assert_eq!(service.results.recv().await, Some(Ok(())));
        service.completed();
        assert!(!service.pending);
    }
    #[tokio::test]
    async fn failure_is_reported_and_does_not_prevent_later_copy() {
        let mut service = Clipboard::default();
        service.requests = Some(service.start_worker(|_| Err(UNAVAILABLE)).unwrap());
        for _ in 0..2 {
            service.copy("fixture".into()).unwrap();
            let result = tokio::time::timeout(Duration::from_secs(5), service.results.recv())
                .await
                .unwrap();
            assert_eq!(result, Some(Err(UNAVAILABLE)));
            service.completed();
        }
    }
    #[test]
    fn over_limit_payload_does_not_start_clipboard_worker() {
        let mut service = Clipboard::default();
        assert!(service.copy("x".repeat(MAX_COPY_BYTES + 1)).is_err());
        assert!(service.requests.is_none());
    }
    #[tokio::test]
    async fn dropping_service_releases_worker_without_waiting_in_tui() {
        let (gone, done) = tokio::sync::oneshot::channel();
        struct OnDrop(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for OnDrop {
            fn drop(&mut self) {
                let _ = self.0.take().unwrap().send(());
            }
        }
        let guard = OnDrop(Some(gone));
        let mut service = Clipboard::default();
        service.requests = Some(
            service
                .start_worker(move |_| {
                    let _ = &guard;
                    Ok(())
                })
                .unwrap(),
        );
        drop(service);
        tokio::time::timeout(Duration::from_secs(5), done)
            .await
            .unwrap()
            .unwrap();
    }
}
