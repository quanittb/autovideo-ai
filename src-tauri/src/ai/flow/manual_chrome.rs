use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug)]
pub enum ProcessBackend {
    Real(Child),
    #[cfg(test)]
    Mock {
        is_running: bool,
    },
}

#[derive(Debug)]
pub struct ManualChromeProcess {
    backend: ProcessBackend,
    profile_dir: PathBuf,
}

impl ManualChromeProcess {
    pub fn new_real(child: Child, profile_dir: PathBuf) -> Self {
        Self {
            backend: ProcessBackend::Real(child),
            profile_dir,
        }
    }

    #[cfg(test)]
    pub fn mock(profile_dir: &Path) -> Self {
        Self {
            backend: ProcessBackend::Mock { is_running: true },
            profile_dir: profile_dir.to_path_buf(),
        }
    }

    pub fn is_running(&mut self) -> bool {
        match &mut self.backend {
            ProcessBackend::Real(child) => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => false,
                Err(_) => false,
            },
            #[cfg(test)]
            ProcessBackend::Mock { is_running } => *is_running,
        }
    }

    pub fn close(&mut self) -> Result<(), String> {
        match &mut self.backend {
            ProcessBackend::Real(child) => {
                if match child.try_wait() {
                    Ok(None) => true,
                    _ => false,
                } {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                Ok(())
            }
            #[cfg(test)]
            ProcessBackend::Mock { is_running } => {
                *is_running = false;
                Ok(())
            }
        }
    }

    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }
}

impl Drop for ManualChromeProcess {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub struct SystemChromeLauncher;

impl SystemChromeLauncher {
    pub fn find_chrome_executable() -> Result<PathBuf, String> {
        // Windows candidates
        #[cfg(target_os = "windows")]
        {
            let mut candidates = Vec::new();

            if let Ok(program_files) = std::env::var("ProgramFiles") {
                candidates.push(
                    PathBuf::from(program_files)
                        .join("Google")
                        .join("Chrome")
                        .join("Application")
                        .join("chrome.exe"),
                );
            }
            if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
                candidates.push(
                    PathBuf::from(program_files_x86)
                        .join("Google")
                        .join("Chrome")
                        .join("Application")
                        .join("chrome.exe"),
                );
            }
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                candidates.push(
                    PathBuf::from(local_app_data)
                        .join("Google")
                        .join("Chrome")
                        .join("Application")
                        .join("chrome.exe"),
                );
            }

            for candidate in candidates {
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }

            // Also check PATH for chrome.exe
            if let Ok(output) = Command::new("where").arg("chrome.exe").output() {
                if output.status.success() {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_line) = out_str.lines().next() {
                        let path = PathBuf::from(first_line.trim());
                        if path.is_file() {
                            return Ok(path);
                        }
                    }
                }
            }
        }

        // macOS candidates
        #[cfg(target_os = "macos")]
        {
            let mac_path =
                PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
            if mac_path.is_file() {
                return Ok(mac_path);
            }
        }

        // Linux candidates
        #[cfg(target_os = "linux")]
        {
            let linux_candidates = [
                "/usr/bin/google-chrome-stable",
                "/usr/bin/google-chrome",
                "/usr/bin/chromium-browser",
                "/usr/bin/chromium",
            ];
            for cand in linux_candidates {
                let p = PathBuf::from(cand);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }

        Err("CHROME_NOT_INSTALLED: Google Chrome Stable is not installed on this system. Please install Google Chrome to perform manual Google login.".to_string())
    }

    pub fn build_manual_chrome_args(profile_dir: &Path, target_url: &str) -> Vec<String> {
        vec![
            format!("--user-data-dir={}", profile_dir.display()),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            target_url.to_string(),
        ]
    }

    pub fn launch(
        chrome_exe: &Path,
        profile_dir: &Path,
        target_url: &str,
    ) -> Result<ManualChromeProcess, String> {
        if !chrome_exe.is_file() {
            return Err(format!(
                "CHROME_NOT_FOUND: Chrome executable not found at {:?}",
                chrome_exe
            ));
        }

        let args = Self::build_manual_chrome_args(profile_dir, target_url);

        let child = Command::new(chrome_exe)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                format!(
                    "FAILED_TO_SPAWN_CHROME: Failed to launch Google Chrome: {}",
                    e
                )
            })?;

        Ok(ManualChromeProcess::new_real(
            child,
            profile_dir.to_path_buf(),
        ))
    }
}
