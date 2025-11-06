use std::path::PathBuf;
use dirs_next::home_dir;
use std::process;
use iced::widget::{
    button, column, text, container, scrollable, row, text_input, toggler
};
use iced::{executor, Application, Command, Element, Settings, Theme, Alignment, Length};
use iced::window::Id;
use std::sync::{Arc, Mutex};

pub fn ui() -> iced::Result {
    Backup::run(Settings::default()) 
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Page {
    #[default]
    Menu,
    Edit,
    Upload,
    Settings,
}

#[derive(Default)]
struct Backup {
    current_page: Page,
    metadata: Option<Arc<Mutex<super::backup::BackupMetadata>>>,
    files: Vec<super::backup::FileInfo>,
    selected_file: Option<PathBuf>,
    settings: super::backup::BackupSettings,
    interval_input: String,
    daemon_status: String,
}

#[derive(Debug, Clone)]
enum Message {
    ToUpload,
    ToEdit,
    ToMenu,
    ToSettings,
    Exit,
    UpdateNow,
    SelectFile(PathBuf),
    DeleteFile,
    OpenFolder,
    Restore,
    RefreshFiles,
    // Settings messages
    ToggleAutoBackup(bool),
    IntervalInputChanged(String),
    SaveSettings,
    StartDaemon,
    StopDaemon,
    RestartDaemon,
    RefreshDaemonStatus,
}

impl Application for Backup {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let metadata = super::backup::BackupMetadata::load_from_file()
            .ok()
            .map(|m| Arc::new(Mutex::new(m)));

        let files = if let Some(meta) = &metadata {
            meta.lock().unwrap().files.values().cloned().collect()
        } else {
            Vec::new()
        };

        let settings = super::backup::BackupSettings::load_from_file()
            .unwrap_or_default();

        let daemon_status = super::daemon::daemon_status();

        (
            Self {
                current_page: Page::Menu,
                metadata,
                files,
                selected_file: None,
                interval_input: settings.interval_minutes.to_string(),
                settings,
                daemon_status,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Iced - FASSBackup")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::ToUpload => {
                self.current_page = Page::Upload;
                if let Some(path) = super::backup::select_folder() {
                    if let Err(e) = super::backup::backup(&path) {
                        println!("Backup error: {}", e);
                    } else if let Ok(meta) = super::backup::BackupMetadata::load_from_file() {
                        self.metadata = Some(Arc::new(Mutex::new(meta.clone())));
                        self.files = meta.files.values().cloned().collect();
                    }
                }
            }
            Message::UpdateNow => {
                if let Some(metadata_arc) = &self.metadata {
                    match super::backup::backup_now(Arc::clone(metadata_arc)) {
                        Ok(count) => println!("Successfully backed up {} file(s)", count),
                        Err(e) => println!("Update now error: {}", e),
                    }
                } else {
                    println!("No metadata available. Perform initial backup first.");
                }
            }
            Message::ToEdit => self.current_page = Page::Edit,
            Message::ToSettings => self.current_page = Page::Settings,
            Message::ToMenu => {
                self.current_page = Page::Menu;
                self.selected_file = None;
            }
            Message::Exit => {
                let daemon_status_clone = Arc::new(Mutex::new(self.daemon_status.clone()));
                let daemon_status_ref = Arc::clone(&daemon_status_clone);

                std::thread::spawn(move || {
                    match super::daemon::start_daemon() {
                        Ok(_) => {
                            println!("Daemon started successfully");
                            let mut status = daemon_status_ref.lock().unwrap();
                            *status = super::daemon::daemon_status();
                        }
                        Err(e) => eprintln!("Failed to start daemon: {}", e),
                    }
                });
                return iced::window::close(Id::MAIN)
            },
            Message::SelectFile(path) => {
                if self.selected_file.as_ref().map(|p| p == &path).unwrap_or(false) {
                    self.selected_file = None;
                } else {
                    self.selected_file = Some(path);
                }
            }
            Message::DeleteFile => {
                if let Some(selected_path) = self.selected_file.take() {
                    if let Some(pos) = self.files.iter().position(|f| f.original_path == selected_path) {
                        let backup_path = self.files[pos].backup_path.clone();
                        let _ = super::backup::delete_selected(backup_path);
                        self.files.remove(pos);
                        let _ = super::backup::update_file_info(self.files.clone());
                    } else {
                        eprintln!("DeleteFile: selected file not found in files list");
                    }
                }
            }
            Message::OpenFolder => {
                if let Some(home) = home_dir() {
                    let backup_folder = home.join("Backup");
                    let _ = process::Command::new("open")
                        .arg(backup_folder)
                        .status();
                }
            }
            Message::Restore => {
                if let Some(selected_path) = &self.selected_file {
                    if let Some(file) = self.files.iter().find(|f| f.original_path == *selected_path) {
                        let source = &file.backup_path;
                        let destination = &file.original_path;
                        
                        if let Some(parent) = destination.parent() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                eprintln!("Failed to create directory {}: {}", parent.display(), e);
                                return Command::none();
                            }
                        }

                        if destination.exists() {
                            eprintln!("Skipped restore: destination already exists ({})", destination.display());
                        } else {
                            match std::fs::copy(source, destination) {
                                Ok(_) => println!("Restored: {}", destination.display()),
                                Err(e) => eprintln!(
                                    "Failed to restore {} from {}: {}",
                                    destination.display(),
                                    source.display(),
                                    e
                                ),
                            }
                        }
                    } else {
                        eprintln!("RestoreFile: selected file not found in metadata");
                    }
                }
            }
            Message::RefreshFiles => {
                if let Ok(meta) = super::backup::BackupMetadata::load_from_file() {
                    self.files = meta.files.values().cloned().collect();
                }
            }
            Message::ToggleAutoBackup(enabled) => {
                self.settings.auto_backup_enabled = enabled;
            }
            Message::IntervalInputChanged(value) => {
                self.interval_input = value;
            }
            Message::SaveSettings => {
                if let Ok(interval) = self.interval_input.parse::<u64>() {
                    if interval > 0 {
                        self.settings.interval_minutes = interval;
                        if let Err(e) = self.settings.save_to_file() {
                            eprintln!("Failed to save settings: {}", e);
                        } else {
                            println!("Settings saved successfully");
                            // Restart daemon if it's running to apply new settings
                            if super::daemon::is_daemon_running() {
                                let _ = super::daemon::restart_daemon();
                            }
                        }
                    } else {
                        eprintln!("Interval must be greater than 0");
                    }
                } else {
                    eprintln!("Invalid interval value");
                }
            }
            Message::StartDaemon => {
                let daemon_status_clone = Arc::new(Mutex::new(self.daemon_status.clone()));
                let daemon_status_ref = Arc::clone(&daemon_status_clone);

                std::thread::spawn(move || {
                    match super::daemon::start_daemon() {
                        Ok(_) => {
                            println!("Daemon started successfully");
                            let mut status = daemon_status_ref.lock().unwrap();
                            *status = super::daemon::daemon_status();
                        }
                        Err(e) => eprintln!("Failed to start daemon: {}", e),
                    }
                });

                // Immediately update the UI status (optional)
                self.daemon_status = super::daemon::daemon_status();
            }
            Message::StopDaemon => {
                match super::daemon::stop_daemon() {
                    Ok(_) => {
                        println!("Daemon stopped successfully");
                        self.daemon_status = super::daemon::daemon_status();
                    }
                    Err(e) => eprintln!("Failed to stop daemon: {}", e),
                }
            }
            Message::RestartDaemon => {
                match super::daemon::restart_daemon() {
                    Ok(_) => {
                        println!("Daemon restarted successfully");
                        self.daemon_status = super::daemon::daemon_status();
                    }
                    Err(e) => eprintln!("Failed to restart daemon: {}", e),
                }
            }
            Message::RefreshDaemonStatus => {
                self.daemon_status = super::daemon::daemon_status();
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        match self.current_page {
            Page::Menu => self.view_menu(),
            Page::Edit => self.view_edit(),
            Page::Upload => self.view_stub("Upload"),
            Page::Settings => self.view_settings(),
        }
    }
}

impl Backup {
    fn view_menu(&self) -> Element<Message> {
        let upload_button = button("Upload").width(Length::Fill).on_press(Message::ToUpload);
        let update_now_button = button("Backup Now").width(Length::Fill).on_press(Message::UpdateNow);
        let edit_button = button("Manage Files").width(Length::Fill).on_press(Message::ToEdit);
        let settings_button = button("Settings").width(Length::Fill).on_press(Message::ToSettings);
        let exit_button = button("Exit").width(Length::Fill).on_press(Message::Exit);

        let content = column![
            text("FASS Backup").size(32),
            upload_button,
            update_now_button,
            edit_button,
            settings_button,
            exit_button,
        ]
        .align_items(Alignment::Center)
        .spacing(16)
        .padding(16)
        .max_width(300);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }

    fn view_settings(&self) -> Element<Message> {
        let title = text("Backup Settings").size(36);

        let auto_backup_toggle = row![
            text("Enable Automatic Backup:").size(16),
            toggler(
                String::new(),
                self.settings.auto_backup_enabled,
                Message::ToggleAutoBackup
            ),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let interval_input = row![
            text("Backup Interval (minutes):").size(16),
            text_input("60", &self.interval_input)
                .on_input(Message::IntervalInputChanged)
                .width(Length::Fixed(100.0)),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let save_button = button("Save Settings")
            .on_press(Message::SaveSettings)
            .style(iced::theme::Button::Primary);

        let daemon_section = column![
            text("Daemon Control").size(24),
            text(&self.daemon_status).size(14),
            row![
                button("Start Daemon").on_press(Message::StartDaemon),
                button("Stop Daemon").on_press(Message::StopDaemon),
                button("Restart Daemon").on_press(Message::RestartDaemon),
            ]
            .spacing(10),
            button("Refresh Status")
                .on_press(Message::RefreshDaemonStatus)
                .style(iced::theme::Button::Secondary),
        ]
        .spacing(10)
        .align_items(Alignment::Start);

        let info_text = text(
            "Note: The daemon runs in the background and automatically backs up \
            your files at the specified interval. You can close this application \
            and backups will continue running."
        )
        .size(12);

        let back_button = button("Back to Menu").on_press(Message::ToMenu);

        let content = column![
            title,
            auto_backup_toggle,
            interval_input,
            save_button,
            container(text("")).height(Length::Fixed(20.0)),
            daemon_section,
            container(text("")).height(Length::Fixed(20.0)),
            info_text,
            container(text("")).height(Length::Fixed(20.0)),
            back_button,
        ]
        .spacing(15)
        .padding(20)
        .max_width(600)
        .align_items(Alignment::Start);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }

    fn view_edit(&self) -> Element<Message> {
        let title = text("Manage Backup Files").size(36);

        // Sort files alphabetically by file name before displaying
        let mut sorted_files = self.files.clone();
        sorted_files.sort_by_key(|file| {
            file.original_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_else(|| String::from(""))
        });

        let file_list: Element<Message> = if sorted_files.is_empty() {
            column![
                text("No files found. Perform a backup first.").size(16),
                text("Click 'Upload' to add files to backup.").size(14),
            ]
            .spacing(10)
            .align_items(Alignment::Center)
            .into()
        } else {
            sorted_files.iter().enumerate().fold(column![], |col, (_index, file)| {
                let is_selected = self
                    .selected_file
                    .as_ref()
                    .map(|p| p == &file.original_path)
                    .unwrap_or(false);

                let file_name = file
                    .original_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let file_button = {
                    let path_clone = file.original_path.clone();
                    button(text(file_name))
                        .width(Length::Fill)
                        .on_press(Message::SelectFile(path_clone))
                };

                let mut entry = column![file_button];

                if is_selected {
                    let details = column![
                        text(format!("Path: {}", file.original_path.display())).size(12),
                        text(format!("Type: {}", file.file_type)).size(12),
                        row![
                            button("Delete File")
                                .on_press(Message::DeleteFile)
                                .style(iced::theme::Button::Destructive),
                            button("Restore")
                                .on_press(Message::Restore),
                            button("Open File Directory")
                                .on_press(Message::OpenFolder)
                        ]
                        .spacing(10),
                    ]
                    .spacing(8)
                    .padding(10);

                    entry = entry.push(container(details).padding(10));
                }

                col.push(container(entry).width(Length::Fill).padding(5))
            })
            .spacing(5)
            .into()
        };

        let back_button = button("Back to Menu").on_press(Message::ToMenu);
        let refresh_button = button("Refresh").on_press(Message::RefreshFiles);

        let content = column![
            title,
            row![back_button, container(text("")).width(Length::Fill), refresh_button]
                .width(Length::Fill),
            scrollable(file_list).height(Length::Fill),
        ]
        .spacing(20)
        .padding(20)
        .max_width(700);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }

    fn view_stub(&self, title: &str) -> Element<Message> {
        container(
            column![
                text(format!("{} Page", title)).size(36),
                button("Back to Menu").on_press(Message::ToMenu)
            ]
            .align_items(Alignment::Center)
            .spacing(20)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x()
        .center_y()
        .into()
    }
}