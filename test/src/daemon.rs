use daemonize_me::Daemon;
use signal_hook::consts::signal::*;
use signal_hook::flag;
use nix::unistd::Pid;
use nix::sys::signal as nix_signal;
use std::fs::{File, OpenOptions, remove_file};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::path::{PathBuf};
use std::env;

fn get_project_dir() -> PathBuf {
    env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn pid_file() -> PathBuf { get_project_dir().join("fass_backup_daemon.pid") }
fn log_file() -> PathBuf { get_project_dir().join("fass_backup_daemon.log") }
fn err_file() -> PathBuf { get_project_dir().join("fass_backup_daemon.err") }

#[derive(Debug, Clone, Copy)]
pub enum DaemonCommand {
    Start,
    Stop,
    Restart,
    Status,
}

pub struct DaemonManager {
    pid_path: PathBuf,
}

impl DaemonManager {
    pub fn new() -> Self {
        Self { pid_path: pid_file() }
    }

    /// Check if daemon is running
    pub fn is_running(&self) -> bool {
        if let Some(pid) = self.get_pid() {
            nix_signal::kill(Pid::from_raw(pid), None).is_ok()
        } else {
            false
        }
    }

    /// Get PID of running daemon
    fn get_pid(&self) -> Option<i32> {
        std::fs::read_to_string(&self.pid_path)
            .ok()?
            .trim()
            .parse::<i32>()
            .ok()
    }

    /// Stop daemon gracefully
    pub fn stop(&self) -> Result<(), String> {
        if let Some(pid) = self.get_pid() {
            nix_signal::kill(Pid::from_raw(pid), nix_signal::Signal::SIGTERM)
                .map_err(|e| format!("Failed to send SIGTERM: {}", e))?;

            // wait max 5 sec
            for _ in 0..10 {
                thread::sleep(Duration::from_millis(500));
                if !self.is_running() {
                    let _ = remove_file(&self.pid_path);
                    return Ok(());
                }
            }
            Err("Daemon did not stop within timeout".to_string())
        } else {
            Err("Daemon is not running".to_string())
        }
    }

    /// Force kill
    pub fn kill(&self) -> Result<(), String> {
        if let Some(pid) = self.get_pid() {
            nix_signal::kill(Pid::from_raw(pid), nix_signal::Signal::SIGKILL)
                .map_err(|e| format!("Failed to send SIGKILL: {}", e))?;
            let _ = remove_file(&self.pid_path);
            Ok(())
        } else {
            Err("Daemon is not running".to_string())
        }
    }

    /// Status
    pub fn status(&self) -> String {
        if let Some(pid) = self.get_pid() {
            if self.is_running() {
                format!("Daemon is running (PID: {})", pid)
            } else {
                "Daemon PID file exists but process is not running (stale)".to_string()
            }
        } else {
            "Daemon is not running".to_string()
        }
    }

    /// Start daemon
    pub fn start(&self) -> Result<(), String> {
        if self.is_running() {
            return Err("Daemon is already running".to_string());
        }

        // remove stale pid if exists
        if let Ok(pid_str) = std::fs::read_to_string(&self.pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                if nix_signal::kill(Pid::from_raw(pid), None).is_err() {
                    let _ = remove_file(&self.pid_path);
                }
            }
        }

        println!("Starting FASS Backup daemon...");

        let stdout = File::create(log_file())
            .map_err(|e| format!("Failed to create log file: {}", e))?;
        let stderr = File::create(err_file())
            .map_err(|e| format!("Failed to create error file: {}", e))?;

        let work_dir = get_project_dir();

        let daemon = Daemon::new()
            .pid_file(&self.pid_path, Some(false))
            .work_dir(&work_dir)
            .stdout(stdout)
            .stderr(stderr);

        daemon.start()
            .map_err(|e| format!("Failed to daemonize: {}", e))?;

        // this runs inside daemonized process
        run_daemon(&self.pid_path);

        Ok(())
    }

    /// Restart
    pub fn restart(&self) -> Result<(), String> {
        if self.is_running() {
            println!("Stopping daemon...");
            self.stop()?;
            thread::sleep(Duration::from_secs(1));
        }
        println!("Starting daemon...");
        self.start()
    }
}

fn run_daemon(pid_path: &PathBuf) {
    let running = Arc::new(AtomicBool::new(true));
    flag::register(SIGINT, running.clone()).unwrap();
    flag::register(SIGTERM, running.clone()).unwrap();

    let mut log = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_file())
        .unwrap();

    writeln!(log, "[{}] Daemon started with PID {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        std::process::id()).unwrap();

    log.flush().unwrap();

    while running.load(Ordering::Relaxed) {
        writeln!(log, "[{}] Running auto-backup check...",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).unwrap();
        log.flush().unwrap();

        if let Err(e) = crate::backup::auto_backup() {
            writeln!(log, "[{}] Auto-backup error: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), e).unwrap();
            log.flush().unwrap();
        }

        for _ in 0..60 {
            if !running.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }

    writeln!(log, "[{}] Daemon shutting down...",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).unwrap();

    if let Err(e) = remove_file(pid_path) {
        writeln!(log, "[{}] Failed to remove PID file: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), e).unwrap();
    } else {
        writeln!(log, "[{}] PID file removed successfully.",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).unwrap();
    }
}

pub fn start_daemon() -> Result<(), String> { DaemonManager::new().start() }
pub fn stop_daemon() -> Result<(), String> { DaemonManager::new().stop() }
pub fn restart_daemon() -> Result<(), String> { DaemonManager::new().restart() }
pub fn daemon_status() -> String { DaemonManager::new().status() }
pub fn is_daemon_running() -> bool { DaemonManager::new().is_running() }