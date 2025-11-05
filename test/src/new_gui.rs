use iced::widget::{button, column, text, container, scrollable, checkbox, row, text_input, pick_list};
use iced::{executor, Application, Command, Element, Settings, Theme, Alignment, Length};
use iced::window::{self, Id};
use std::path::PathBuf;

use crate::daemon;
use crate::backup;

pub fn ui() -> iced::Result {
    Backup::run(Settings::default()) 
}

#[derive(Debug, Clone, Copy, Default, PartialEq)] 
enum Page {
    #[default] 
    Menu,
    Edit,
    View,
    Upload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FrequencyUnit {
    Minutes,
    Hours,
    Days,
}

impl FrequencyUnit {
    const ALL: [FrequencyUnit; 3] = [
        FrequencyUnit::Minutes,
        FrequencyUnit::Hours,
        FrequencyUnit::Days,
    ];

    fn as_str(&self) -> &str {
        match self {
            FrequencyUnit::Minutes => "Minute(s)",
            FrequencyUnit::Hours => "Hour(s)",
            FrequencyUnit::Days => "Day(s)",
        }
    }
}

impl std::fmt::Display for FrequencyUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

struct Backup {
    current_page: Page,
    files: Vec<super::backup::FileInfo>,
    selected_file: Option<usize>,
    temp_interval: String,
    temp_frequency: Option<FrequencyUnit>,
}

impl Default for Backup {
    fn default() -> Self {
        Self {
            current_page: Page::Menu,
            files: Vec::new(),
            selected_file: None,
            temp_interval: String::new(),
            temp_frequency: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    ToUpload,
    ToEdit, 
    ToView, 
    ToMenu, 
    ExitToDaemon,
    LoadFiles,
    FilesLoaded(Vec<super::backup::FileInfo>),
    SelectFile(usize),
    ToggleAutoBackup,
    IntervalChanged(String),
    FrequencySelected(FrequencyUnit),
    SaveSettings,
    DeleteFile,
}

impl Application for Backup { 
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        // Auto-start daemon if not already running
        if !super::daemon::is_daemon_running() {
            println!("Starting daemon in background...");
            let _ = super::daemon::start_daemon();
            std::thread::sleep(std::time::Duration::from_millis(500)); // Give daemon time to start
        }
        
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("Iced - FASSBackup")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::ToUpload => {
                self.current_page = Page::Upload;
                if let Some(path) = super::backup::select_folder() {
                    super::backup::backup(&path).unwrap_or_else(|e| {
                        eprintln!("Backup failed: {}", e);
                    });
                } else {
                    println!("No folder selected");
                }
            },
            Message::ToEdit => {
                self.current_page = Page::Edit;
                return Command::perform(async {}, |_| Message::LoadFiles);
            },
            Message::ToView => self.current_page = Page::View,
            Message::ToMenu => {
                self.current_page = Page::Menu;
                self.selected_file = None;
            },
            Message::ExitToDaemon => {
                match super::daemon::start_daemon() {
                    Ok(_) => {
                        println!("Daemon started successfully. GUI closing...");
                    }
                    Err(e) => {
                        if e.contains("already running") {
                            println!("Daemon already running. GUI closing...");
                        } else {
                            eprintln!("Failed to start daemon: {}", e);
                        }
                    }
                }
                return window::close(Id::MAIN);
            }
            Message::LoadFiles => {
                return Command::perform(
                    async { super::backup::load_file_info() },
                    Message::FilesLoaded
                );
            }
            Message::FilesLoaded(files) => {
                self.files = files;
                self.selected_file = None;
            }
            Message::SelectFile(index) => {
                if self.selected_file == Some(index) {
                    self.selected_file = None;
                    self.temp_interval = String::new();
                    self.temp_frequency = None;
                } else {
                    self.selected_file = Some(index);
                    if let Some(file) = self.files.get(index) {
                        // Load the actual backup_time value
                        self.temp_interval = file.backup_time.as_secs().to_string();
                        
                        // Load the frequency
                        self.temp_frequency = Some(match file.backup_frequency.as_str() {
                            "Minute(s)" => FrequencyUnit::Minutes,
                            "Hour(s)" => FrequencyUnit::Hours,
                            "Day(s)" => FrequencyUnit::Days,
                            _ => FrequencyUnit::Minutes,
                        });
                        
                        println!("Selected file: {} (interval: {}s, freq: {})", 
                            file.original_path.display(),
                            file.backup_time.as_secs(),
                            file.backup_frequency);
                    }
                }
            }
            Message::ToggleAutoBackup => {
                if let Some(index) = self.selected_file {
                    if let Some(file) = self.files.get_mut(index) {
                        file.auto_backup = !file.auto_backup;
                    }
                    // Use update_file_info to overwrite the entire JSON
                    let _ = super::backup::update_file_info(self.files.clone());
                }
            }
            Message::IntervalChanged(value) => {
                self.temp_interval = value;
            }
            Message::FrequencySelected(freq) => {
                self.temp_frequency = Some(freq);
            }
            Message::SaveSettings => {
                if let Some(index) = self.selected_file {
                    if let Some(file) = self.files.get_mut(index) {
                        if let Ok(interval) = self.temp_interval.parse::<u64>() {
                            println!("Updating interval from {}s to {}s", file.backup_time.as_secs(), interval);
                            file.backup_time = std::time::Duration::from_secs(interval);
                            
                            if let Some(freq) = &self.temp_frequency {
                                println!("Updating frequency from '{}' to '{}'", file.backup_frequency, freq.as_str());
                                file.backup_frequency = freq.as_str().to_string();
                            }
                            
                            println!("Settings saved for: {} ({}s every {})", 
                                file.original_path.display(),
                                file.backup_time.as_secs(),
                                file.backup_frequency);
                        } else {
                            eprintln!("Failed to parse interval: '{}'", self.temp_interval);
                        }
                    }
                    // Use update_file_info to overwrite the entire JSON
                    match super::backup::update_file_info(self.files.clone()) {
                        Ok(_) => println!("Successfully saved to JSON"),
                        Err(e) => eprintln!("Failed to save JSON: {}", e),
                    }
                }
            }
            Message::DeleteFile => {
                if let Some(index) = self.selected_file {
                    if let Some(file) = self.files.get(index) {
                        let _ = super::backup::delete_selected(file.backup_path.clone());
                        self.files.remove(index);
                        self.selected_file = None;
                        let _ = super::backup::update_file_info(self.files.clone());
                    }
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> { 
        match self.current_page { 
            Page::Menu => self.view_menu(), 
            Page::Edit => self.view_edit(), 
            Page::View => self.view_stub("View"),
            Page::Upload => self.view_stub("Upload"),
        }
    }
}

impl Backup {
    fn view_menu(&self) -> Element<Message> { 
        let upload_button = button("Upload")
            .width(Length::Fill) 
            .on_press(Message::ToUpload);
            
        let edit_button = button("Edit")
            .width(Length::Fill)
            .on_press(Message::ToEdit);
            
        let view_button = button("View")
            .width(Length::Fill)
            .on_press(Message::ToView);
            
        let exit_button = button("Exit") 
            .width(Length::Fill)
            .on_press(Message::ExitToDaemon);

        let daemon_indicator = if super::daemon::is_daemon_running() {
            text("🟢 Daemon: Running").size(14)
        } else {
            text("🔴 Daemon: Stopped").size(14)
        };

        let content = column![
            text("FASS Backup").size(32).font(iced::Font::DEFAULT),
            daemon_indicator,
            upload_button,
            edit_button,
            view_button,
            exit_button,
        ]
        .align_items(Alignment::Center) 
        .spacing(16) 
        .padding(16)
        .width(Length::Shrink) 
        .max_width(300); 
        
        container(content)
            .width(Length::Fill) 
            .height(Length::Fill) 
            .center_x()
            .center_y()
            .into()
    }

    fn view_edit(&self) -> Element<Message> {
        let title = text("Manage Backup Files").size(36);

        // File list
        let file_list: Element<Message> = if self.files.is_empty() {
            column![
                text("No backup files found.").size(16),
                text("Click 'Upload' to add files to backup.").size(14),
            ]
            .spacing(10)
            .align_items(Alignment::Center)
            .into()
        } else {
            self.files.iter().enumerate()
                .fold(column![], |col, (index, file)| {
                    let is_selected = self.selected_file == Some(index);
                    
                    let file_name = file.original_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown");
                    
                    let auto_indicator = if file.auto_backup { "🔄" } else { "" };
                    
                    let file_button = button(
                        text(format!("{} {}", auto_indicator, file_name))
                            .size(14)
                    )
                    .width(Length::Fill)
                    .on_press(Message::SelectFile(index));
                    
                    let mut file_col = column![file_button].spacing(5);
                    
                    // Show details if selected
                    if is_selected {
                        let details = column![
                            text(format!("Path: {}", file.original_path.display())).size(12),
                            text(format!("Type: {}", file.file_type)).size(12),
                            row![
                                text("Auto Backup:").size(12),
                                checkbox("", file.auto_backup)
                                    .on_toggle(|_| Message::ToggleAutoBackup),
                            ].spacing(10).align_items(Alignment::Center),
                            
                            row![
                                text("Interval:").size(12),
                                text_input("", &self.temp_interval)
                                    .on_input(Message::IntervalChanged)
                                    .width(Length::Fixed(80.0)),
                                pick_list(
                                    &FrequencyUnit::ALL[..],
                                    self.temp_frequency.as_ref(),
                                    Message::FrequencySelected
                                )
                                .width(Length::Fixed(120.0)),
                            ].spacing(10).align_items(Alignment::Center),
                            
                            row![
                                button("Save Settings")
                                    .on_press(Message::SaveSettings),
                                button("Delete File")
                                    .on_press(Message::DeleteFile),
                            ].spacing(10),
                        ]
                        .spacing(8)
                        .padding(10);
                        
                        file_col = file_col.push(
                            container(details)
                                .padding(10)
                        );
                    }
                    
                    col.push(
                        container(file_col)
                            .width(Length::Fill)
                            .padding(5)
                    )
                })
                .spacing(5)
                .into()
        };
        
        let back_button = button("Back to Menu")
            .on_press(Message::ToMenu)
            .padding(10);

        let refresh_button = button("Refresh")
            .on_press(Message::LoadFiles)
            .padding(10);

        let content = column![
            title,
            row![
                back_button,
                container(text("")).width(Length::Fill),
                refresh_button,
            ].width(Length::Fill),
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
        let content = column![
            text(format!("{} Page", title)).size(36).font(iced::Font::DEFAULT),
            
            button("Back to Menu").on_press(Message::ToMenu).padding(15),
        ]
        .align_items(Alignment::Center)
        .spacing(20);
        
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}