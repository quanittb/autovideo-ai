use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MockScenario {
    Ready,
    LoggedOut,
    CreditsRequired,
    GenerationPending,
    GenerationComplete,
    GenerationError,
    UiChanged,
    CorruptDownload,
    WrongDownload,
    DelayAfterGenerateClick,
}

pub struct MockFlowServerHandle {
    pub base_url: String,
    pub scenario: Arc<Mutex<MockScenario>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for MockFlowServerHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

pub struct MockFlowServer;

impl MockFlowServer {
    pub async fn start(scenario: MockScenario) -> Result<MockFlowServerHandle, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind mock server port: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local addr: {}", e))?;

        let base_url = format!("http://127.0.0.1:{}", addr.port());
        let scenario_ref = Arc::new(Mutex::new(scenario));
        let scenario_clone = scenario_ref.clone();

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    accept_res = listener.accept() => {
                        if let Ok((mut socket, _)) = accept_res {
                            let sc_ref = scenario_clone.clone();
                            tokio::spawn(async move {
                                let mut buf = [0u8; 4096];
                                if let Ok(n) = socket.read(&mut buf).await {
                                    let req_str = String::from_utf8_lossy(&buf[..n]);
                                    let sc = { sc_ref.lock().unwrap().clone() };

                                    let (status_line, content_type, body) = Self::handle_request(&req_str, sc);
                                    let resp = format!(
                                        "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                        status_line,
                                        content_type,
                                        body.len()
                                    );
                                    let _ = socket.write_all(resp.as_bytes()).await;
                                    let _ = socket.write_all(&body).await;
                                }
                            });
                        }
                    }
                }
            }
        });

        Ok(MockFlowServerHandle {
            base_url,
            scenario: scenario_ref,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    fn handle_request(req: &str, scenario: MockScenario) -> (&'static str, &'static str, Vec<u8>) {
        if req.contains("GET /download") {
            match scenario {
                MockScenario::CorruptDownload => {
                    return (
                        "HTTP/1.1 200 OK",
                        "video/mp4",
                        b"CORRUPT_NOT_A_REAL_MP4_HEADER".to_vec(),
                    );
                }
                MockScenario::WrongDownload => {
                    return (
                        "HTTP/1.1 200 OK",
                        "text/plain",
                        b"WRONG_DOWNLOAD_PAYLOAD".to_vec(),
                    );
                }
                _ => {
                    // Minimal valid dummy mp4 ftyp box (32 bytes)
                    let valid_header = [
                        0x00, 0x00, 0x00, 0x20, 0x66, 0x74, 0x79, 0x70, 0x69, 0x73, 0x6f, 0x6d,
                        0x00, 0x00, 0x02, 0x00, 0x69, 0x73, 0x6f, 0x6d, 0x69, 0x73, 0x6f, 0x32,
                        0x61, 0x76, 0x63, 0x31, 0x6d, 0x70, 0x34, 0x31,
                    ];
                    return ("HTTP/1.1 200 OK", "video/mp4", valid_header.to_vec());
                }
            }
        }

        // HTML Pages
        match scenario {
            MockScenario::LoggedOut => {
                let html = "<html><body><div class='login-prompt'>Sign in with Google to continue</div></body></html>";
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::CreditsRequired => {
                let html = "<html><body><div id='flow-app'><div class='credits-alert'>0 Credits remaining. Upgrade your plan.</div></div></body></html>";
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::UiChanged => {
                let html = "<html><body><div id='completely-redesigned-layout'>Unknown elements</div></body></html>";
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::GenerationError => {
                let html = "<html><body><div id='flow-app'><div class='error-banner'>Generation failed: Inappropriate prompt content detected</div></div></body></html>";
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            _ => {
                let html = r#"<html><body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <button id="generate-button">Generate</button>
  <div id="progress-indicator" data-progress="100" data-status="ready">Ready</div>
  <a id="download-link" href="/download">Download Generated Video</a>
</div>
</body></html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
        }
    }
}
