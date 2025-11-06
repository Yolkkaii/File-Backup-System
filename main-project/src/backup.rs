use serde::{Serialize, Deserialize};
use rfd::FileDialog;
use walkdir::WalkDir;
use dirs_next::home_dir;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use std::sync::{Arc, Mutex};
use chrono::Local;
use std::collections::HashMap;

fn calculate_hash(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = file.read(&mut buffer).ok()?;
        if bytes_read == 0 { break; }
        hasher.update(&buffer[..bytes_read]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub file_type: String,
    #[serde(default)]
    pub hash: String,
}

impl Default for FileInfo {
    fn default() -> Self {
        Self {
            original_path: PathBuf::new(),
            backup_path: PathBuf::new(),
            file_type: String::new(),
            hash: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub files: HashMap<PathBuf, FileInfo>,
}

impl BackupMetadata {
    pub fn load_from_file() -> std::io::Result<Self> {
        let path = "backup_metadata.json";
        if let Ok(mut f) = File::open(path) {
            let mut contents = String::new();
            f.read_to_string(&mut contents)?;
            
            // Try to parse as Vec first (old format)
            if let Ok(vec) = serde_json::from_str::<Vec<FileInfo>>(&contents) {
                let mut files = HashMap::new();
                for mut file_info in vec {
                    // Calculate hash if it's empty
                    if file_info.hash.is_empty() {
                        if let Some(hash) = calculate_hash(&file_info.original_path) {
                            file_info.hash = hash;
                        }
                    }
                    files.insert(file_info.original_path.clone(), file_info);
                }
                return Ok(BackupMetadata { files });
            }
            
            // Try new format (HashMap)
            Ok(serde_json::from_str(&contents).unwrap_or_default())
        } else {
            Ok(BackupMetadata::default())
        }
    }

    pub fn save_to_file(&self) -> std::io::Result<()> {
        let path = "backup_metadata.json";
        let file = File::create(path)?;
        serde_json::to_writer_pretty(&file, self)?;
        Ok(())
    }
}

pub fn update_file_info(files: Vec<FileInfo>) -> std::io::Result<()> {
    let mut metadata = BackupMetadata::default();
    for file in files {
        metadata.files.insert(file.original_path.clone(), file);
    }
    metadata.save_to_file()
}

pub fn delete_selected(selected_file: PathBuf) -> std::io::Result<()> {
    if selected_file.exists() {
        fs::remove_file(&selected_file)?;
        println!("Deleted: {}", selected_file.display());
    }
    Ok(())
}

pub fn select_folder() -> Option<PathBuf> {
    if let Some(home) = home_dir() {
        let backup_folder = home.join("Backup");
        fs::create_dir_all(&backup_folder).unwrap();
        FileDialog::new().set_directory(&home).pick_folder()
    } else {
        println!("Could not determine home directory.");
        None
    }
}

pub fn backup(selected_folder: &Path) -> std::io::Result<()> {
    let home = home_dir().expect("Could not determine home directory");
    let backup_folder = home.join("Backup");
    fs::create_dir_all(&backup_folder)?;

    // Load existing metadata
    let mut metadata = BackupMetadata::load_from_file().unwrap_or_default();

    for entry in WalkDir::new(selected_folder).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative_path = path.strip_prefix(selected_folder).unwrap();
        let dest_path = backup_folder.join(relative_path);

        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            continue;
        }

        if path.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let new_hash = calculate_hash(path);
            let existing = metadata.files.get(&path.to_path_buf());

            let should_copy = match existing {
                Some(old) if !old.hash.is_empty() => Some(&old.hash) != new_hash.as_ref(),
                _ => true, // Copy if no existing entry or hash is empty
            };

            if should_copy {
                fs::copy(path, &dest_path)?;
                println!("Copied: {}", dest_path.display());
            } else {
                println!("Skipped (unchanged): {}", path.display());
            }

            let file_type = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            // Update or insert metadata
            if let Some(hash) = new_hash {
                let file_info = FileInfo {
                    original_path: path.to_path_buf(),
                    backup_path: dest_path,
                    file_type,
                    hash,
                };
                metadata.files.insert(path.to_path_buf(), file_info);
            }
        }
    }

    // Save metadata
    metadata.save_to_file()?;
    println!("Metadata updated successfully.");

    Ok(())
}

pub fn backup_now(metadata_arc: Arc<Mutex<BackupMetadata>>) -> Result<usize, String> {
    let mut backed_up_count = 0;
    
    let mut metadata = metadata_arc.lock().map_err(|e| format!("Lock error: {}", e))?;
    println!("[{}] Running immediate backup...", Local::now().format("%Y-%m-%d %H:%M:%S"));

    for info in metadata.files.values_mut() {
        if !info.original_path.exists() {
            println!("Original file missing: {}", info.original_path.display());
            continue;
        }

        // Ensure parent directory exists
        if let Some(parent) = info.backup_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                println!("Failed to create backup directory: {}", e);
                continue;
            }
        }

        match calculate_hash(&info.original_path) {
            Some(current_hash) => {
                // If hash is empty, always backup
                let needs_backup = info.hash.is_empty() || current_hash != info.hash;
                
                if needs_backup {
                    if let Err(e) = fs::copy(&info.original_path, &info.backup_path) {
                        println!("Backup error ({}): {}", info.original_path.display(), e);
                    } else {
                        println!(
                            "[{}] Backed up: {}",
                            Local::now().format("%Y-%m-%d %H:%M:%S"),
                            info.original_path.display()
                        );
                        info.hash = current_hash;
                        backed_up_count += 1;
                    }
                } else {
                    println!("No changes in {}", info.original_path.display());
                }
            }
            None => println!("Hash check failed for {}", info.original_path.display()),
        }
    }

    if backed_up_count > 0 {
        if let Err(e) = metadata.save_to_file() {
            println!("Failed to save updated metadata: {}", e);
            return Err(format!("Failed to save metadata: {}", e));
        }
    }

    println!("Backup complete: {} file(s) backed up", backed_up_count);
    Ok(backed_up_count)
}