use daemonize_me::Daemon;
use signal_hook::consts::signal::*;
use signal_hook::flag;
use nix::unistd::Pid;
use nix::sys::signal as nix_signal;
use std::fs::{File, OpenOptions, remove_file};
use std::io::{Write, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::path::PathBuf;
use std::env;

fn get_project_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn pid_file() -> PathBuf { get_project_dir().join("fass_backup_daemon.pid") }
fn log_file() -> PathBuf { get_project_dir().join("fass_backup_daemon.log") }
fn err_file() -> PathBuf { get_project_dir().join("fass_backup_daemon.err") }

pub struct DaemonManager {
    pid_path: PathBuf,
}

impl DaemonManager {
    pub fn new() -> Self {
        Self { pid_path: pid_file() }
    }

    /// Check if daemon is running by checking both PID file and process
    pub fn is_running(&self) -> bool {
        if let Some(pid) = self.get_pid() {
            // Check if process exists
            match nix_signal::kill(Pid::from_raw(pid), None) {
                Ok(_) => true,
                Err(_) => {
                    // Process doesn't exist, remove stale PID file
                    let _ = remove_file(&self.pid_path);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Get PID of running daemon
    fn get_pid(&self) -> Option<i32> {
        let mut file = File::open(&self.pid_path).ok()?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).ok()?;
        contents.trim().parse::<i32>().ok()
    }

    /// Stop daemon gracefully
    pub fn stop(&self) -> Result<(), String> {
        if !self.is_running() {
            // Clean up stale PID file if it exists
            let _ = remove_file(&self.pid_path);
            return Err("Daemon is not running".to_string());
        }

        let pid = self.get_pid().ok_or("Failed to read PID")?;
        
        println!("Sending SIGTERM to PID {}...", pid);
        
        // Send SIGTERM
        nix_signal::kill(Pid::from_raw(pid), nix_signal::Signal::SIGTERM)
            .map_err(|e| format!("Failed to send SIGTERM: {}", e))?;

        // Wait up to 10 seconds for graceful shutdown
        for i in 0..20 {
            thread::sleep(Duration::from_millis(500));
            if !self.is_running() {
                println!("Daemon stopped gracefully");
                let _ = remove_file(&self.pid_path);
                return Ok(());
            }
            if i % 4 == 0 {
                println!("Waiting for daemon to stop...");
            }
        }

        // If still running, try SIGKILL
        println!("Daemon didn't stop gracefully, sending SIGKILL...");
        if self.is_running() {
            nix_signal::kill(Pid::from_raw(pid), nix_signal::Signal::SIGKILL)
                .map_err(|e| format!("Failed to send SIGKILL: {}", e))?;
            thread::sleep(Duration::from_millis(500));
            let _ = remove_file(&self.pid_path);
            
            if self.is_running() {
                return Err("Failed to kill daemon process".to_string());
            }
        }
        
        Ok(())
    }

    /// Force kill
    pub fn kill(&self) -> Result<(), String> {
        if let Some(pid) = self.get_pid() {
            nix_signal::kill(Pid::from_raw(pid), nix_signal::Signal::SIGKILL)
                .map_err(|e| format!("Failed to send SIGKILL: {}", e))?;
            thread::sleep(Duration::from_millis(500));
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
                // Load settings to show interval
                if let Ok(settings) = crate::backup::BackupSettings::load_from_file() {
                    if settings.auto_backup_enabled {
                        format!("✓ Daemon is running (PID: {}, Interval: {} min)", 
                            pid, settings.interval_minutes)
                    } else {
                        format!("⚠ Daemon is running (PID: {}) but auto-backup is disabled", pid)
                    }
                } else {
                    format!("✓ Daemon is running (PID: {})", pid)
                }
            } else {
                "⚠ Stale PID file found (daemon not running)".to_string()
            }
        } else {
            "✗ Daemon is not running".to_string()
        }
    }

    /// Start daemon
    pub fn start(&self) -> Result<(), String> {
        // Clean up any stale PID files first
        if self.is_running() {
            return Err("Daemon is already running. Use 'restart' to restart it.".to_string());
        } else {
            // Remove stale PID file
            let _ = remove_file(&self.pid_path);
        }

        // Check if auto-backup is enabled
        let settings = crate::backup::BackupSettings::load_from_file()
            .unwrap_or_default();
        
        if !settings.auto_backup_enabled {
            return Err("Auto-backup is disabled in settings. Please enable it first.".to_string());
        }

        // Check if there are files to backup
        let metadata = crate::backup::BackupMetadata::load_from_file()
            .map_err(|e| format!("Failed to load metadata: {}", e))?;
        
        if metadata.files.is_empty() {
            return Err("No files to backup. Please perform an initial backup first.".to_string());
        }

        println!("Starting FASS Backup daemon...");
        println!("Working directory: {}", get_project_dir().display());
        println!("Backup interval: {} minutes", settings.interval_minutes);

        let stdout = File::create(log_file())
            .map_err(|e| format!("Failed to create log file: {}", e))?;
        let stderr = File::create(err_file())
            .map_err(|e| format!("Failed to create error file: {}", e))?;

        let work_dir = get_project_dir();

        let daemon = Daemon::new()
            .pid_file(&self.pid_path, Some(false))
            .work_dir(&work_dir)
            .stdout(stdout)
            .stderr(stderr)
            .umask(0o027);

        match daemon.start() {
            Ok(_) => {
                // This code runs in the daemon process
                run_daemon(&self.pid_path);
                Ok(())
            }
            Err(e) => {
                Err(format!("Failed to daemonize: {}", e))
            }
        }
    }

    /// Restart
    pub fn restart(&self) -> Result<(), String> {
        println!("Restarting daemon...");
        
        if self.is_running() {
            println!("Stopping existing daemon...");
            self.stop()?;
            thread::sleep(Duration::from_secs(2));
        }
        
        println!("Starting daemon...");
        self.start()
    }
}

fn run_daemon(pid_path: &PathBuf) {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let r2 = running.clone();
    
    // Register signal handlers
    let _ = flag::register(SIGINT, r);
    let _ = flag::register(SIGTERM, r2);

    let mut log = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_file())
        .expect("Failed to open log file");

    writeln!(log, "\n{:═<60}", "").unwrap();  // using '═' for visual line, or just '='
    writeln!(log, "[{}] Daemon started with PID {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        std::process::id()).unwrap();
    writeln!(log, "{:=<60}\n", "").unwrap();
    log.flush().unwrap();

    // Main daemon loop
    while running.load(Ordering::Relaxed) {
    let settings = crate::backup::BackupSettings::load_from_file()
        .unwrap_or_default();

    if settings.auto_backup_enabled {
        writeln!(log, "[{}] Running auto-backup...", chrono::Local::now()).unwrap();
        let _ = crate::backup::auto_backup();
    } else {
        writeln!(log, "[{}] Auto-backup disabled; sleeping...", chrono::Local::now()).unwrap();
    }

    log.flush().unwrap();

    // sleep loop
    for _ in 0..(settings.interval_minutes * 60) {
        if !running.load(Ordering::Relaxed) { break; }
        thread::sleep(Duration::from_secs(1));
    }
}

    writeln!(log, "\n[{}] Daemon shutting down gracefully...",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).unwrap();
    log.flush().unwrap();

    // Clean up PID file
    match remove_file(pid_path) {
        Ok(_) => {
            writeln!(log, "[{}] PID file removed successfully",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).unwrap();
        }
        Err(e) => {
            writeln!(log, "[{}] Failed to remove PID file: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), e).unwrap();
        }
    }

    writeln!(log, "[{}] Daemon stopped", 
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).unwrap();
    writeln!(log, "{:=<60}\n", "").unwrap();
    log.flush().unwrap();
}

pub fn start_daemon() -> Result<(), String> { DaemonManager::new().start() }
pub fn stop_daemon() -> Result<(), String> { DaemonManager::new().stop() }
pub fn restart_daemon() -> Result<(), String> { DaemonManager::new().restart() }
pub fn daemon_status() -> String { DaemonManager::new().status() }
pub fn is_daemon_running() -> bool { DaemonManager::new().is_running() }