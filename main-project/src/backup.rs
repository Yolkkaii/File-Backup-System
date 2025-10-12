use serde::{Serialize, Deserialize};
use rfd::FileDialog;
use walkdir::WalkDir;
use dirs_next::home_dir;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::thread;
//use chrono::{DateTime, Local};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileInfo {
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub file_type: String,
    //pub last_backup: DateTime<Local>,
    pub auto_backup: bool,
    pub backup_time: Duration,
    pub backup_frequency: String,
}

pub fn select_folder() -> Option<PathBuf> {
    if let Some(home) = home_dir() {
        println!("User's home directory: {}", home.display());

        let backup_folder = home.join("Backup");

        // Check if Backup folder exists else create it
        if backup_folder.exists() && backup_folder.is_dir() {
            println!("Backup folder exists at: {}", backup_folder.display());
        } else {
            println!("Backup folder doesn't exist. Creating...");
            fs::create_dir_all(&backup_folder).unwrap();
            println!("Backup folder has been created at: {}", backup_folder.display());
        }

        // Open file explorer for user to pick a folder
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

    // Load existing metadata if available else create new vector
    let mut file_info: Vec<FileInfo> = if let Ok(mut f) = File::open("backup_metadata.json") {
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Iterate over all files and folders in the selected folder
    for entry in WalkDir::new(selected_folder).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative_path = path.strip_prefix(selected_folder).unwrap();
        let dest_path = backup_folder.join(relative_path);

        if path.is_dir() {
            // Create folders in backup
            fs::create_dir_all(&dest_path)?;
            println!("Copying Folder: {}", dest_path.display());
        } else if path.is_file() {
            // Ensure parent folder exists before copying file
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dest_path)?;
            println!("Copying File: {}", dest_path.display());

            // Get file type (extension)
            let file_type = path.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()); //displays "unknown" if type if not able to be unwrapped

            // Create FileInfo struct
            let info = FileInfo {
                original_path: path.to_path_buf(),
                backup_path: dest_path.to_path_buf(),
                file_type,
                ..Default::default() // auto_backup, backup_time, etc. set later
            };

            // Save to vector
            file_info.push(info);
        }
    }

    // Save updated metadata to JSON
    let mut file = File::create("backup_metadata.json")?;
    serde_json::to_writer_pretty(&mut file, &file_info)?;

    Ok(())
}

pub fn delete_selected(selected_file: PathBuf) -> std::io::Result<()> {
    // Delete the selected file if it exists
    if selected_file.exists() {
        fs::remove_file(&selected_file)?;
        println!("Deleted: {}", selected_file.display());
    }
    Ok(())
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

// Auto backup function for files with auto_backup = true
pub fn auto_backup() -> std::io::Result<()> {
    // Load metadata
    let mut file_info: Vec<FileInfo> = {
        let mut f = File::open("backup_metadata.json")?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        serde_json::from_str(&contents).unwrap_or_default()
    };

    // Only keep files with auto_backup enabled
    let auto_files: Vec<FileInfo> = file_info.into_iter()
        .filter(|info| info.auto_backup)
        .collect();

    for info in auto_files {
        let info_clone = info.clone();
        //Makes new threa
        std::thread::spawn(move || {
            let interval = convert_time(&info_clone, &info_clone.backup_frequency)
                .unwrap_or(3600); // default 1 hour
            loop {
                std::thread::sleep(std::time::Duration::from_secs(interval));
                if let Some(parent) = info_clone.backup_path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                if let Err(e) = std::fs::copy(&info_clone.original_path, &info_clone.backup_path) {
                    println!("Failed to auto-backup {}: {}", info_clone.original_path.display(), e);
                } else {
                    println!("Auto-backed up: {}", info_clone.backup_path.display());
                }
            }
        });
    }

    Ok(())
}

// Usage of convert_time
// let info = FileInfo {
//     backup_time: Duration::from_secs(1),
//     ..Default::default()
// };

// if let Some(seconds) = convert_time(&info, "Hour(s)") {
//     println!("Converted time in seconds: {}", seconds);
// } else {
//     println!("Frequency was invalid");
// }