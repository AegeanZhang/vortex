use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::JoinHandle,
};
use tracing_subscriber::{EnvFilter, fmt::MakeWriter};

pub const WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
pub struct Capture {
    bytes: Arc<Mutex<Vec<u8>>>,
    changed: Arc<Notify>,
}

impl io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        self.changed.notify_one();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

impl Capture {
    pub fn dispatch(&self, level: &str) -> tracing::Dispatch {
        tracing::Dispatch::new(
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::new(format!("off,vortex_wire={level}")))
                .without_time()
                .with_ansi(false)
                .json()
                .with_writer(self.clone())
                .finish(),
        )
    }
    pub fn events(&self) -> Vec<Value> {
        let bytes = self.bytes.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    pub async fn wait_phase(&self, phase: &str) -> Value {
        tokio::time::timeout(WAIT, async {
            loop {
                let changed = self.changed.notified();
                if let Some(event) = self
                    .events()
                    .into_iter()
                    .find(|event| event["fields"]["phase"] == phase)
                {
                    return event;
                }
                changed.await;
            }
        })
        .await
        .expect("diagnostic event timed out")
    }
}

pub struct Server(pub JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub async fn serve<F, Fut>(handler: F) -> (String, Server)
where
    F: FnOnce(TcpStream, Value) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let header_end = loop {
            if let Some(index) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                break index + 4;
            }
            assert!(request.len() < 16 * 1024);
            let mut bytes = [0; 1024];
            let count = socket.read(&mut bytes).await.unwrap();
            assert!(count > 0);
            request.extend_from_slice(&bytes[..count]);
        };
        let headers = std::str::from_utf8(&request[..header_end]).unwrap();
        assert!(headers.starts_with("POST /search HTTP/1.1\r\n"));
        let length: usize = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        assert!(length < 64 * 1024);
        assert!(
            headers
                .lines()
                .any(|line| line.eq_ignore_ascii_case("authorization: Bearer fixture-secret"))
        );
        while request.len() < header_end + length {
            let mut bytes = [0; 1024];
            let count = socket.read(&mut bytes).await.unwrap();
            assert!(count > 0);
            request.extend_from_slice(&bytes[..count]);
        }
        let body = serde_json::from_slice(&request[header_end..header_end + length]).unwrap();
        handler(socket, body).await;
        // 一个 fixture 只允许一次请求，也不允许重试后另建连接。
        assert!(
            matches!(listener.into_std().unwrap().accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
    });
    (format!("http://{address}"), Server(task))
}

pub async fn response(socket: &mut TcpStream, status: u16, mime: &str, body: &str) {
    let headers = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(body.as_bytes()).await.unwrap();
    socket.shutdown().await.unwrap();
}
