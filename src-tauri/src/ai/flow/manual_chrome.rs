use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug)]
pub enum ProcessBackend {
    Real(Child),
    #[cfg(test)]
    Mock {
        initial_alive: bool,
        handed_off_alive: bool,
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
            backend: ProcessBackend::Mock {
                initial_alive: true,
                handed_off_alive: false,
            },
            profile_dir: profile_dir.to_path_buf(),
        }
    }

    #[cfg(test)]
    pub fn mock_with_handoff(
        profile_dir: &Path,
        initial_alive: bool,
        handed_off_alive: bool,
    ) -> Self {
        Self {
            backend: ProcessBackend::Mock {
                initial_alive,
                handed_off_alive,
            },
            profile_dir: profile_dir.to_path_buf(),
        }
    }

    pub fn is_running(&mut self) -> bool {
        match &mut self.backend {
            ProcessBackend::Real(child) => {
                // 1. First check if the initially spawned launcher child process is still alive
                let initial_alive = match child.try_wait() {
                    Ok(None) => true,
                    _ => false,
                };
                if initial_alive {
                    return true;
                }

                // 2. If launcher child exited (e.g. Chrome process handoff to browser master),
                // check if any Chrome process is still running with this app-owned profile directory
                let pids = Self::find_profile_chrome_pids(&self.profile_dir);
                !pids.is_empty()
            }
            #[cfg(test)]
            ProcessBackend::Mock {
                initial_alive,
                handed_off_alive,
            } => *initial_alive || *handed_off_alive,
        }
    }

    pub fn close(&mut self) -> Result<(), String> {
        match &mut self.backend {
            ProcessBackend::Real(child) => {
                // 1. Terminate initial child
                if match child.try_wait() {
                    Ok(None) => true,
                    _ => false,
                } {
                    let _ = child.kill();
                    let _ = child.wait();
                }

                // 2. Terminate any remaining processes strictly belonging to this managed profile
                let pids = Self::find_profile_chrome_pids(&self.profile_dir);
                Self::kill_specific_pids(&pids);

                Ok(())
            }
            #[cfg(test)]
            ProcessBackend::Mock {
                initial_alive,
                handed_off_alive,
            } => {
                *initial_alive = false;
                *handed_off_alive = false;
                Ok(())
            }
        }
    }

    pub fn find_profile_chrome_pids(profile_dir: &Path) -> Vec<u32> {
        let profile_str = profile_dir.to_string_lossy().to_lowercase();
        let mut matching_pids = Vec::new();

        #[cfg(target_os = "windows")]
        {
            if let Ok(output) = Command::new("wmic")
                .args([
                    "process",
                    "where",
                    "name='chrome.exe' or name='chromium.exe'",
                    "get",
                    "ProcessId,CommandLine",
                    "/format:csv",
                ])
                .output()
            {
                if output.status.success() {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    for line in out_str.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with("Node,") {
                            continue;
                        }
                        if let Some(last_comma) = line.rfind(',') {
                            let pid_str = line[last_comma + 1..].trim();
                            let cmd_line = line[..last_comma].to_lowercase();
                            if (cmd_line.contains("user-data-dir")
                                && cmd_line.contains(&profile_str))
                                || cmd_line.contains(&profile_str)
                            {
                                if let Ok(pid) = pid_str.parse::<u32>() {
                                    if pid > 0 && !matching_pids.contains(&pid) {
                                        matching_pids.push(pid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Ok(entries) = std::fs::read_dir("/proc") {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(file_name) = path.file_name() {
                            if let Ok(pid) = file_name.to_string_lossy().parse::<u32>() {
                                let cmdline_path = path.join("cmdline");
                                if let Ok(cmdline) = std::fs::read_to_string(cmdline_path) {
                                    let cmdline_lower = cmdline.to_lowercase();
                                    if (cmdline_lower.contains("chrome")
                                        || cmdline_lower.contains("chromium"))
                                        && cmdline_lower.contains(&profile_str)
                                    {
                                        if !matching_pids.contains(&pid) {
                                            matching_pids.push(pid);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = Command::new("ps").args(["-eo", "pid,command"]).output() {
                if output.status.success() {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    for line in out_str.lines() {
                        let line = line.trim();
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(pid) = parts[0].parse::<u32>() {
                                let cmd = line[parts[0].len()..].trim().to_lowercase();
                                if (cmd.contains("chrome") || cmd.contains("chromium"))
                                    && cmd.contains(&profile_str)
                                {
                                    if !matching_pids.contains(&pid) {
                                        matching_pids.push(pid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        matching_pids
    }

    pub fn kill_specific_pids(pids: &[u32]) {
        for &pid in pids {
            if pid == 0 || pid == std::process::id() {
                continue;
            }
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
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
