use std::path::PathBuf;
use dirs_next::home_dir;
use std::process;
use iced::widget::{
    button, column, text, container, scrollable, row
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
    View,
    Upload,
}

#[derive(Default)]
struct Backup {
    current_page: Page,
    metadata: Option<Arc<Mutex<super::backup::BackupMetadata>>>,
    files: Vec<super::backup::FileInfo>,
    selected_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum Message {
    ToUpload,
    ToEdit,
    ToView,
    ToMenu,
    Exit,
    UpdateNow,
    SelectFile(PathBuf),
    DeleteFile,
    OpenFolder,
    RefreshFiles,
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

        (
            Self {
                current_page: Page::Menu,
                metadata,
                files,
                selected_file: None,
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
            Message::ToView => self.current_page = Page::View,
            Message::ToMenu => {
                self.current_page = Page::Menu;
                self.selected_file = None;
            }
            Message::Exit => return iced::window::close(Id::MAIN),
            Message::SelectFile(path) => {
                if self.selected_file.as_ref().map(|p| p == &path).unwrap_or(false) {
                    self.selected_file = None;
                } else {
                    self.selected_file = Some(path);
                }
            }
            Message::DeleteFile => {
                if let Some(selected_path) = self.selected_file.take() {
                    // Find the entry in the real files vec by path
                    if let Some(pos) = self.files.iter().position(|f| f.original_path == selected_path) {
                        // attempt to delete the backup file (ignore error)
                        let backup_path = self.files[pos].backup_path.clone();
                        let _ = super::backup::delete_selected(backup_path);

                        // remove from the in-memory list
                        self.files.remove(pos);

                        // persist metadata
                        let _ = super::backup::update_file_info(self.files.clone());
                    } else {
                        // stale selection: nothing found
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
            Message::RefreshFiles => {
                if let Ok(meta) = super::backup::BackupMetadata::load_from_file() {
                    self.files = meta.files.values().cloned().collect();
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
        let upload_button = button("Upload").width(Length::Fill).on_press(Message::ToUpload);
        let update_now_button = button("Backup Now").width(Length::Fill).on_press(Message::UpdateNow);
        let edit_button = button("Edit").width(Length::Fill).on_press(Message::ToEdit);
        let view_button = button("View").width(Length::Fill).on_press(Message::ToView);
        let exit_button = button("Exit").width(Length::Fill).on_press(Message::Exit);

        let content = column![
            text("FASS Backup").size(32),
            upload_button,
            update_now_button,
            edit_button,
            view_button,
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

    fn view_edit(&self) -> Element<Message> {
        let title = text("Manage Backup Files").size(36);

        // 🔹 Sort files alphabetically by file name before displaying
        let mut sorted_files = self.files.clone();
        sorted_files.sort_by_key(|file| {
            file.original_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_lowercase()) // case-insensitive
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
            sorted_files.iter().enumerate().fold(column![], |col, (index, file)| {
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