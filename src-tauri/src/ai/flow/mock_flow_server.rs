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
    EligibilityRequired,
    PublicLandingRedirectToLogin,
    PublicLandingRedirectToWorkspace,
    AvatarOnly,
    NoTransitionAfterClick,
    UnknownPollDom,
    ResultMissingDownload,
    MissingDurationSelector,
    MissingOrientationSelector,
    ReadbackMismatch,
    WrongOutputCount,
    ImageOnlyFileInput,
    UnattachedVideoUpload,
    TrueVideoEditActive,
    StaleCreditEstimate,
    CreditPolicyConflict,
    EditModeReset,
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
            MockScenario::EligibilityRequired => {
                let html = "<html><body><div id='eligibility-gate'><div class='alert'>Account not eligible: Age verification required. Please verify your age with Google.</div></div></body></html>";
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::GenerationError => {
                let html = "<html><body><div id='flow-app'><div class='error-banner'>Generation failed: Inappropriate prompt content detected</div></div></body></html>";
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::PublicLandingRedirectToLogin => {
                if req.contains("GET /signin") {
                    let html = "<html><body><form><input name='identifier' id='identifierId' type='email'/><div class='login-prompt'>Sign in with Google</div></form></body></html>";
                    ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
                } else {
                    let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow - AI Creative Studio for Video, Images & Custom Tools</title></head>
<body>
  <div>AI creative studio built with Google's advanced generative models.</div>
  <button id="cta-btn" onclick="window.location.href='/signin'">Create with Google Flow</button>
</body>
</html>"#;
                    ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
                }
            }
            MockScenario::PublicLandingRedirectToWorkspace => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow - AI Creative Studio for Video, Images & Custom Tools</title></head>
<body>
  <div id="landing">
    <div>AI creative studio built with Google's advanced generative models.</div>
    <button id="cta-btn" onclick="document.getElementById('landing').style.display='none'; document.getElementById('flow-app').style.display='block';">Create with Google Flow</button>
  </div>
  <div id="flow-app" data-authenticated="true" style="display:none;">
    <textarea id="prompt-input" placeholder="Describe video"></textarea>
    <button id="generate-button">Generate</button>
  </div>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::AvatarOnly => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow</title></head>
<body>
  <img src="https://googleusercontent.com/avatar.png" aria-label="Google Account" />
  <button>Create</button>
  <canvas width="100" height="100"></canvas>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::DelayAfterGenerateClick => {
                let html = Self::render_interactive_workspace(
                    r#"<div id="progress-indicator" data-progress="0">Waiting</div>
  <a id="download-link" href="/download" style="display:none;">Download Generated Video</a>"#,
                    r#"document.getElementById('progress-indicator').innerText = 'Generating (delayed)...';
    document.getElementById('progress-indicator').setAttribute('data-progress', '10');"#,
                );
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::NoTransitionAfterClick => {
                let html = Self::render_interactive_workspace(
                    "",
                    "// No DOM transition -> stays ambiguous",
                );
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::UnknownPollDom => {
                let html = "<html><body><div id='unknown-widget-redesigned'>Completely unrecognized page</div></body></html>";
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::ResultMissingDownload => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Result Missing Download</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <button id="generate-button">Generate</button>
  <div id="progress-indicator" data-progress="100" data-status="ready">Ready (No Download)</div>
</div>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::MissingDurationSelector => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Missing Duration</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <button id="settings-button">Video · 720p · 8s crop_16_9 x2</button>
  <div id="settings-popover" role="menu" data-state="closed" style="display:none;">
    <button data-testid="model-select">Omni Flash</button>
    <button role="tab" data-testid="ori-portrait">crop_9_16 9:16</button>
    <button role="tab" data-testid="ori-landscape" data-state="active">crop_16_9 16:9</button>
    <button role="tab" data-testid="count-x1">x1</button>
  </div>
  <button id="generate-button">Generate</button>
</div>
<script>
  document.getElementById('settings-button').addEventListener('click', function() {
    const pop = document.getElementById('settings-popover');
    pop.style.display = 'block';
    pop.setAttribute('data-state', 'open');
  });
</script>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::MissingOrientationSelector => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Missing Orientation</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <button id="settings-button">Video · 720p · 8s crop_16_9 x2</button>
  <div id="settings-popover" role="menu" data-state="closed" style="display:none;">
    <button data-testid="model-select">Omni Flash</button>
    <button role="tab" data-testid="length-10s">10s</button>
    <button role="tab" data-testid="count-x1">x1</button>
  </div>
  <button id="generate-button">Generate</button>
</div>
<script>
  document.getElementById('settings-button').addEventListener('click', function() {
    const pop = document.getElementById('settings-popover');
    pop.style.display = 'block';
    pop.setAttribute('data-state', 'open');
  });
</script>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::ReadbackMismatch => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Readback Mismatch</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <button id="settings-button">Video · 720p · 8s crop_16_9 x2</button>
  <div id="settings-popover" role="menu" data-state="closed" style="display:none;">
    <button data-testid="model-select">Omni Flash</button>
    <button role="tab" data-testid="ori-portrait" data-state="inactive">crop_9_16 9:16</button>
    <button role="tab" data-testid="ori-landscape" data-state="active">crop_16_9 16:9</button>
    <button role="tab" data-testid="length-10s" data-state="inactive">10s</button>
    <button role="tab" data-testid="length-8s" data-state="active">8s</button>
    <button role="tab" data-testid="count-x1" data-state="inactive">x1</button>
    <button role="tab" data-testid="count-x2" data-state="active">x2</button>
  </div>
  <button id="generate-button">Generate</button>
</div>
<script>
  document.getElementById('settings-button').addEventListener('click', function() {
    const pop = document.getElementById('settings-popover');
    pop.style.display = 'block';
    pop.setAttribute('data-state', 'open');
  });
</script>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::ImageOnlyFileInput => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Image Only</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <input type="file" id="image-upload" accept="image/*" />
  <button id="generate-button">Generate</button>
</div>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::UnattachedVideoUpload => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Unattached Video</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <button id="generate-button">Generate</button>
</div>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            MockScenario::TrueVideoEditActive => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - True Video Edit</title></head>
<body>
<div id="flow-app" data-authenticated="true" data-edit-active="true">
  <div id="source-video-chip" data-testid="source-chip">flow_acceptance_01.mp4</div>
  <div class="lf-player-container">00:09:16</div>
  <textarea id="prompt-input" placeholder="Mô tả nội dung bạn muốn chỉnh sửa"></textarea>
  <div id="credit-info">Quá trình tạo sẽ tốn 20 tín dụng</div>
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
            MockScenario::CreditPolicyConflict => {
                let html = r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock - Credit Conflict</title></head>
<body>
<div id="flow-app" data-authenticated="true" data-edit-active="true">
  <div id="source-video-chip" data-testid="source-chip">flow_acceptance_01.mp4</div>
  <textarea id="prompt-input" placeholder="Mô tả nội dung bạn muốn chỉnh sửa"></textarea>
  <div id="credit-info">Quá trình tạo sẽ tốn 15 tín dụng</div>
  <button id="generate-button">Generate</button>
</div>
</body>
</html>"#;
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
            _ => {
                let html = Self::render_interactive_workspace(
                    r#"<div id="progress-indicator" data-progress="100" data-status="ready">Ready</div>
  <a id="download-link" href="/download">Download Generated Video</a>"#,
                    "document.getElementById('download-link').style.display = 'block';",
                );
                ("HTTP/1.1 200 OK", "text/html", html.as_bytes().to_vec())
            }
        }
    }

    fn render_interactive_workspace(extra_html: &str, extra_click_js: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head><title>Google Flow Mock</title></head>
<body>
<div id="flow-app" data-authenticated="true">
  <textarea id="prompt-input" placeholder="Enter prompt"></textarea>
  <input type="file" id="video-upload" />
  <button id="settings-button">Video · 720p · 8s crop_16_9 x2</button>
  <div id="settings-popover" role="menu" data-state="closed" style="display:none;">
    <button data-testid="model-select">Omni Flash</button>
    <button role="tab" data-testid="ori-portrait" data-state="inactive">crop_9_16 9:16</button>
    <button role="tab" data-testid="ori-landscape" data-state="active">crop_16_9 16:9</button>
    <button role="tab" data-testid="length-4s" data-state="inactive">4s</button>
    <button role="tab" data-testid="length-6s" data-state="inactive">6s</button>
    <button role="tab" data-testid="length-8s" data-state="active">8s</button>
    <button role="tab" data-testid="length-10s" data-state="inactive">10s</button>
    <button role="tab" data-testid="count-x1" data-state="inactive">x1</button>
    <button role="tab" data-testid="count-x2" data-state="active">x2</button>
    <button role="tab" data-testid="count-x3" data-state="inactive">x3</button>
    <button role="tab" data-testid="count-x4" data-state="inactive">x4</button>
    <div id="credit-info">Quá trình tạo sẽ tốn 15 tín dụng</div>
  </div>
  <button id="generate-button">Generate</button>
  {}
</div>
<script>
  let curLength = '8s';
  let curOri = '16:9';
  let curCount = 'x2';

  function updateSummary() {{
    document.getElementById('settings-button').innerText = 'Video · 720p · ' + curLength + ' ' + (curOri === '9:16' ? 'crop_9_16' : 'crop_16_9') + ' ' + curCount;
  }}

  document.getElementById('settings-button').addEventListener('click', function() {{
    const pop = document.getElementById('settings-popover');
    pop.style.display = 'block';
    pop.setAttribute('data-state', 'open');
  }});

  document.addEventListener('keydown', function(e) {{
    if (e.key === 'Escape') {{
      const pop = document.getElementById('settings-popover');
      pop.style.display = 'none';
      pop.setAttribute('data-state', 'closed');
    }}
  }});

  const tabs = document.querySelectorAll('#settings-popover button[role="tab"]');
  tabs.forEach(t => {{
    t.addEventListener('click', function() {{
      const testid = t.getAttribute('data-testid');
      if (testid.startsWith('length-')) {{
        document.querySelectorAll('[data-testid^="length-"]').forEach(x => {{
          x.setAttribute('data-state', 'inactive');
          x.classList.remove('active');
        }});
        t.setAttribute('data-state', 'active');
        t.classList.add('active');
        curLength = t.innerText.trim();
      }} else if (testid.startsWith('ori-')) {{
        document.querySelectorAll('[data-testid^="ori-"]').forEach(x => {{
          x.setAttribute('data-state', 'inactive');
          x.classList.remove('active');
        }});
        t.setAttribute('data-state', 'active');
        t.classList.add('active');
        curOri = t.innerText.includes('9:16') ? '9:16' : '16:9';
      }} else if (testid.startsWith('count-')) {{
        document.querySelectorAll('[data-testid^="count-"]').forEach(x => {{
          x.setAttribute('data-state', 'inactive');
          x.classList.remove('active');
        }});
        t.setAttribute('data-state', 'active');
        t.classList.add('active');
        curCount = t.innerText.trim();
      }}
      updateSummary();
    }});
  }});

  document.getElementById('generate-button').addEventListener('click', function() {{
    fetch('/api/click', {{ method: 'POST' }});
    {}
  }});
</script>
</body>
</html>"#,
            extra_html, extra_click_js
        )
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
