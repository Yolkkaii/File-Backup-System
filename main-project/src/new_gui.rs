use iced::widget::{button, column, text, container, scrollable, checkbox, row};
use iced::{executor, Application, Command, Element, Settings, Theme, Alignment, Length};
use iced::window::{Id};
use std::sync::{Arc, Mutex};

pub fn ui() -> iced::Result {
    Backup::run(Settings::default()) 
}

#[derive(Debug, Clone, Copy, Default)] 
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
    files: Vec<(String, bool)>,
    metadata: Option<Arc<Mutex<super::backup::BackupMetadata>>>, // Add this field
}

#[derive(Debug, Clone, Copy)]
enum Message {
    ToUpload,
    ToEdit, 
    ToView, 
    ToMenu, 
    Exit, 
    ToggleFileCheck(usize), 
    FilterOptions,         
    DeleteConfirmed,
    UpdateNow, // Add this new message
}

impl Application for Backup { 
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let initial_files = vec![
            (String::from("Project_A_20250101.zip"), false),
            (String::from("Data_Client_XYZ.bak"), false),
            (String::from("System_Config_Backup.cfg"), false),
            (String::from("Old_Files_Archive.tar"), false),
            (String::from("Daily_Log_20251010.txt"), false),
            (String::from("Image_Assets_v2.tar.gz"), false),
            (String::from("Important_Database_Dump.sql"), false),
        ];
        
        // Load metadata if available
        let metadata = super::backup::BackupMetadata::load_from_file()
            .ok()
            .map(|m| Arc::new(Mutex::new(m)));
        
        (
            Self { 
                current_page: Page::Menu, 
                files: initial_files,
                metadata,
            }, 
            Command::none()
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
                    } else {
                        // Reload metadata after backup
                        if let Ok(meta) = super::backup::BackupMetadata::load_from_file() {
                            self.metadata = Some(Arc::new(Mutex::new(meta)));
                        }
                    }
                } else {
                    println!("No folder selected");
                }
            },
            Message::UpdateNow => {
                if let Some(metadata_arc) = &self.metadata {
                    match super::backup::backup_now(Arc::clone(metadata_arc)) {
                        Ok(count) => {
                            println!("Successfully backed up {} file(s)", count);
                        }
                        Err(e) => {
                            println!("Update now error: {}", e);
                        }
                    }
                } else {
                    println!("No metadata available. Please perform an initial backup first.");
                }
            },
            Message::ToEdit => self.current_page = Page::Edit, 
            Message::ToView => self.current_page = Page::View,
            Message::ToMenu => self.current_page = Page::Menu, 
            Message::Exit => { 
                return iced::window::close(Id::MAIN);
            }
            Message::ToggleFileCheck(index) => {
                if let Some((_, is_checked)) = self.files.get_mut(index) {
                    *is_checked = !*is_checked;
                }
            }
            Message::FilterOptions => {
                println!("Filter/Sort Options button clicked."); 
            }
            Message::DeleteConfirmed => {
                let files_to_delete_count = self.files.iter().filter(|(_, is_checked)| *is_checked).count();
                println!("Deleting {} selected files.", files_to_delete_count);

                //backup.rs
                //super::backup::delete_selected(files_to_delete)
                
                self.files.retain(|(_, is_checked)| !*is_checked);
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
    fn view_menu<'a>(&self) -> Element<'a, Message> { 
        let upload_button = button("Upload")
            .width(Length::Fill) 
            .on_press(Message::ToUpload);
        
        let update_now_button = button("Update Now")
            .width(Length::Fill)
            .on_press(Message::UpdateNow);
            
        let edit_button = button("Edit")
            .width(Length::Fill)
            .on_press(Message::ToEdit);
            
        let view_button = button("View")
            .width(Length::Fill)
            .on_press(Message::ToView);
            
        let exit_button = button("Exit") 
            .width(Length::Fill)
            .on_press(Message::Exit);

        let content = column![
            text("FASS Backup").size(32).font(iced::Font::DEFAULT),
            upload_button,
            update_now_button, // Add the new button here
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

    fn view_edit<'a>(&self) -> Element<'a, Message> {
        let title = text("Edit & Manage Backup Files").size(36);
        
        let filter_button = button(text("...").size(20))
            .on_press(Message::FilterOptions);

        let header_row = row![
            text("File List").size(24).width(Length::Fill),
            filter_button,
        ]
        .align_items(Alignment::Center);
        
        let file_list: Element<Message> = self.files.iter().enumerate()
            .fold(column![], |col, (index, (name, is_checked))| {
                let check_btn = checkbox("", *is_checked)
                    .on_toggle(move |_| Message::ToggleFileCheck(index));
                    
                let file_name = text(name).width(Length::Fill);
                
                let file_row = row![
                    check_btn,
                    file_name,
                ]
                .spacing(15)
                .align_items(Alignment::Center)
                .padding(10)
                .width(Length::Fill);
                
                col.push(file_row)
            })
            .spacing(5)
            .into();
        
        let back_button = button("Back to Menu")
            .width(Length::Shrink)
            .on_press(Message::ToMenu); 

        let delete_button = button(text("Delete Selected").size(16))
            .width(Length::Shrink)
            .on_press(Message::DeleteConfirmed)
            .style(iced::theme::Button::Destructive);

        let footer_row = row![
            back_button,
            container(text("")).width(Length::Fill), 
            delete_button,
        ]
        .align_items(Alignment::End)
        .width(Length::Fill);

        let content = column![
            title,
            header_row,
            scrollable(file_list).height(Length::FillPortion(1)),
            footer_row, 
        ]
        .align_items(Alignment::Center) 
        .spacing(20)
        .padding(20)
        .max_width(600);
        
        container(content)
            .width(Length::Fill) 
            .height(Length::Fill) 
            .center_x()
            .center_y()
            .into()
    }

    fn view_stub<'a>(&self, title: &str) -> Element<'a, Message> {
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