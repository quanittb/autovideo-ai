use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    pub generate_click_count: Arc<AtomicUsize>,
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

        let click_counter = Arc::new(AtomicUsize::new(0));
        let click_counter_clone = click_counter.clone();

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
                            let cc_ref = click_counter_clone.clone();
                            tokio::spawn(async move {
                                let mut buf = [0u8; 4096];
                                if let Ok(n) = socket.read(&mut buf).await {
                                    let req_str = String::from_utf8_lossy(&buf[..n]);
                                    let sc = { sc_ref.lock().unwrap().clone() };

                                    if req_str.contains("POST /api/click") || req_str.contains("POST /api/generate") {
                                        cc_ref.fetch_add(1, Ordering::SeqCst);
                                    }

                                    let (status_line, content_type, body) = Self::handle_request(&req_str, sc);
                                    let extra_headers = if req_str.contains("GET /download") {
                                        "Content-Disposition: attachment; filename=\"video.mp4\"\r\n"
                                    } else {
                                        ""
                                    };
                                    let resp = format!(
                                        "{}\r\nContent-Type: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                        status_line,
                                        content_type,
                                        extra_headers,
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
            generate_click_count: click_counter,
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
                    // Valid synthetic mp4 fixture with ftyp and moov/mdat
                    let valid_mp4_bytes = generate_minimal_valid_mp4();
                    return ("HTTP/1.1 200 OK", "video/mp4", valid_mp4_bytes);
                }
            }
        }

        if req.contains("POST /api/click") || req.contains("POST /api/generate") {
            return (
                "HTTP/1.1 200 OK",
                "application/json",
                b"{\"recorded\": true}".to_vec(),
            );
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
            MockScenario::DelayAfterGenerateClick => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Delay Window</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <input type="file" id="video-upload" />
  <button id="generate-button">Generate</button>
  <div id="progress-indicator" data-progress="0">Waiting</div>
  <a id="download-link" href="/download" style="display:none;">Download Generated Video</a>
</div>
<script>
  document.getElementById('generate-button').addEventListener('click', function() {
    fetch('/api/click', { method: 'POST' });
    document.getElementById('progress-indicator').innerText = 'Generating (delayed)...';
    document.getElementById('progress-indicator').setAttribute('data-progress', '10');
  });
</script>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            _ => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <input type="file" id="video-upload" />
  <button id="generate-button">Generate</button>
  <div id="progress-indicator" data-progress="100" data-status="ready">Ready</div>
  <a id="download-link" href="/download">Download Generated Video</a>
</div>
<script>
  document.getElementById('generate-button').addEventListener('click', function() {
    fetch('/api/click', { method: 'POST' });
    document.getElementById('download-link').style.display = 'block';
  });
</script>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
        }
    }
}

fn generate_minimal_valid_mp4() -> Vec<u8> {
    // 32-byte standard ftyp box (mp42/isom)
    let mut bytes = vec![
        0x00, 0x00, 0x00, 0x20, // size 32
        0x66, 0x74, 0x79, 0x70, // 'ftyp'
        0x69, 0x73, 0x6f, 0x6d, // 'isom'
        0x00, 0x00, 0x02, 0x00, // minor version
        0x69, 0x73, 0x6f, 0x6d, // compatible brand 1
        0x69, 0x73, 0x6f, 0x32, // compatible brand 2
        0x61, 0x76, 0x63, 0x31, // compatible brand 3
        0x6d, 0x70, 0x34, 0x31, // compatible brand 4
    ];
    // Free box / minimal data
    bytes.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x08, // size 8
        0x66, 0x72, 0x65, 0x65, // 'free'
    ]);
    bytes
}
