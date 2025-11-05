use serde::{Serialize, Deserialize};
use rfd::FileDialog;
use walkdir::WalkDir;
use dirs_next::home_dir;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashSet;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileInfo {
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub file_type: String,
    pub auto_backup: bool,
    #[serde(with = "duration_serde")]
    pub backup_time: Duration,
    pub backup_frequency: String,
}

// Serde helper for Duration
mod duration_serde {
    use serde::{Serialize, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    struct DurationHelper {
        secs: u64,
        nanos: u32,
    }

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let helper = DurationHelper {
            secs: duration.as_secs(),
            nanos: duration.subsec_nanos(),
        };
        helper.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DurationHelper::deserialize(deserializer)?;
        Ok(Duration::new(helper.secs, helper.nanos))
    }
}

// Global state to track running backup threads
lazy_static::lazy_static! {
    static ref RUNNING_BACKUPS: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
}

pub fn get_metadata_path() -> std::path::PathBuf {
    // Try to use the working directory saved by the daemon
    if let Ok(saved_dir) = std::fs::read_to_string("/tmp/fass_backup_workdir.txt") {
        let work_path = std::path::PathBuf::from(saved_dir.trim());
        return work_path.join("backup_metadata.json");
    }
    
    // Fallback to current directory
    std::path::PathBuf::from("backup_metadata.json")
}

pub fn save_info(info: Vec<FileInfo>) -> std::io::Result<()> {
    let path = get_metadata_path();

    let mut existing_info: Vec<FileInfo> = if path.exists() {
        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        if !contents.is_empty() {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    for new_entry in info {
        if !existing_info.iter().any(|f| f.original_path == new_entry.original_path) {
            existing_info.push(new_entry);
        }
    }

    let mut file = File::create(&path)?;
    serde_json::to_writer_pretty(&mut file, &existing_info)?;

    Ok(())
}

pub fn update_file_info(info: Vec<FileInfo>) -> std::io::Result<()> {
    let path = get_metadata_path();
    let mut file = File::create(&path)?;
    serde_json::to_writer_pretty(&mut file, &info)?;
    Ok(())
}

pub fn select_folder() -> Option<PathBuf> {
    if let Some(home) = home_dir() {
        println!("User's home directory: {}", home.display());

        let backup_folder = home.join("Backup");

        if backup_folder.exists() && backup_folder.is_dir() {
            println!("Backup folder exists at: {}", backup_folder.display());
        } else {
            println!("Backup folder doesn't exist. Creating...");
            fs::create_dir_all(&backup_folder).unwrap();
            println!("Backup folder has been created at: {}", backup_folder.display());
        }

        let selected_folder = FileDialog::new()
            .set_directory(&home)
            .pick_folder();

        if let Some(path) = &selected_folder {
            println!("Selected: {}", path.display());
        } else {
            println!("No folder selected");
        }

        selected_folder
        
    } else {
        println!("Could not determine home directory.");
        None
    }
}

pub fn backup(selected_folder: &Path) -> std::io::Result<()> {
    let home = home_dir().expect("Could not determine home directory");
    let backup_folder = home.join("Backup");
    fs::create_dir_all(&backup_folder)?;

    let metadata_path = get_metadata_path();
    let file_info: Vec<FileInfo> = if let Ok(mut f) = File::open(&metadata_path) {
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut new_entries: Vec<FileInfo> = Vec::new();

    for entry in WalkDir::new(selected_folder).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative_path = path.strip_prefix(selected_folder).unwrap();
        let dest_path = backup_folder.join(relative_path);

        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            println!("Copying Folder: {}", dest_path.display());
        } else if path.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dest_path)?;
            println!("Copying File: {}", dest_path.display());

            let file_type = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let info = FileInfo {
                original_path: path.to_path_buf(),
                backup_path: dest_path.to_path_buf(),
                file_type,
                ..Default::default()
            };

            if !file_info.iter().any(|f| f.original_path == info.original_path) {
                new_entries.push(info);
            }
        }
    }

    if !new_entries.is_empty() {
        save_info(new_entries)?;
    }

    Ok(())
}

pub fn delete_selected(selected_file: PathBuf) -> std::io::Result<()> {
    if selected_file.exists() {
        fs::remove_file(&selected_file)?;
        println!("Deleted: {}", selected_file.display());
    }
    Ok(())
}

pub fn load_file_info() -> Vec<FileInfo> {
    let metadata_path = get_metadata_path();
    if let Ok(mut f) = File::open(&metadata_path) {
        let mut contents = String::new();
        if f.read_to_string(&mut contents).is_ok() {
            return serde_json::from_str(&contents).unwrap_or_default();
        }
    }
    Vec::new()
}

pub fn convert_time(info: &FileInfo, frequency: &str) -> Option<u64> {
    let seconds = info.backup_time.as_secs();

    match frequency {
        "Day(s)" => Some(seconds * 60 * 60 * 24),
        "Hour(s)" => Some(seconds * 60 * 60),
        "Minute(s)" => Some(seconds * 60),
        _ => {
            println!("No frequency selected");
            None
        }
    }
}

// Improved auto backup function - only spawns threads for files not already being backed up
pub fn auto_backup() -> std::io::Result<()> {
    let metadata_path = get_metadata_path();
    
    // Check if file exists and log the current directory
    if !metadata_path.exists() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("unknown"));
        eprintln!("ERROR: {} not found. Current directory: {}", metadata_path.display(), cwd.display());
        eprintln!("Expected path: {}", metadata_path.display());
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} not found in {}", metadata_path.display(), cwd.display())
        ));
    }

    // Load metadata
    let file_info: Vec<FileInfo> = {
        let mut f = File::open(&metadata_path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        serde_json::from_str(&contents).unwrap_or_default()
    };

    // Only keep files with auto_backup enabled
    let auto_files: Vec<FileInfo> = file_info.into_iter()
        .filter(|info| info.auto_backup)
        .collect();

    println!("Found {} files with auto-backup enabled", auto_files.len());

    // Get currently running backups
    let mut running = RUNNING_BACKUPS.lock().unwrap();

    for info in auto_files {
        // Skip if already running
        if running.contains(&info.original_path) {
            println!("Already backing up: {}", info.original_path.display());
            continue;
        }

        // Mark as running
        running.insert(info.original_path.clone());
        println!("Starting auto-backup thread for: {}", info.original_path.display());

        let info_clone = info.clone();

        // Spawn thread for this file
        thread::spawn(move || {
            let interval = convert_time(&info_clone, &info_clone.backup_frequency)
                .unwrap_or(3600); // default 1 hour

            println!("Backup interval: {}s for {}", interval, info_clone.original_path.display());

            loop {
                thread::sleep(Duration::from_secs(interval));
                
                // Perform backup
                if let Some(parent) = info_clone.backup_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                
                match fs::copy(&info_clone.original_path, &info_clone.backup_path) {
                    Ok(_) => {
                        println!("[{}] Auto-backed up: {} -> {}", 
                            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                            info_clone.original_path.display(), 
                            info_clone.backup_path.display());
                    }
                    Err(e) => {
                        eprintln!("[{}] Failed to auto-backup {}: {}", 
                            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                            info_clone.original_path.display(), e);
                    }
                }
            }
        });
    }

    Ok(())
}