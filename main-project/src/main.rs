//Test file explorer
use rfd::FileDialog;
use walkdir::WalkDir;
use dirs_next::home_dir;
use std::fs;
use std::path::{Path, PathBuf};

fn main(){
    if let Some(home) = home_dir() {
        println!("User's home directory: {}", home.display());

        let backup_folder = home.join("Backup");

        if backup_folder.exists() && backup_folder.is_dir() {
            println!("Backup folder exists at: {}", backup_folder.display());
        } else {
            println!("Backup folder doesn't exists. Creating...");
            fs::create_dir_all(&backup_folder).unwrap();
            println!("Backup folder has been created at: {}", backup_folder.display());
        }

        let selected_folder = FileDialog::new()
            .set_directory(&home)
            .pick_folder();

        if let Some(to_copy) = &selected_folder {
            println!("Picked folder: {}", to_copy.display());

            for entry in WalkDir::new(to_copy).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let relative_path = path.strip_prefix(&to_copy).unwrap();
                let dest_path = backup_folder.join(relative_path);

                if path.is_dir() {
                    fs::create_dir_all(&dest_path).unwrap();
                    println!("Copying Folder: {}", dest_path.display())
                } else if path.is_file() {
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent).unwrap();
                    }
                    fs::copy(path, &dest_path).unwrap();
                    println!("Copying File: {}", dest_path.display())
                }
            }
        } else {
            println!("No folder selected");
        }
        
    } else {
        println!("Could not determine home directory.");
    }
}