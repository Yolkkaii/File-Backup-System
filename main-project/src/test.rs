fn main() -> iced::Result {
    BackupApp::run(Settings::default())
}

#[derive(Default)]
struct BackupApp {
    selected_path: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    SelectFolder,
}

impl Sandbox for BackupApp {
    type Message = Message;

    fn new() -> Self {
        Self::default()
    }

    fn title(&self) -> String {
        "Backup App".into()
    }

    fn update(&mut self, message: Self::Message) {
        match message {
            Message::SelectFolder => {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.selected_path = Some(path.display().to_string());
                    println!("Selected folder: {}", path.display());
                }
            }
        }
    }

    fn view(&self) -> Element<Self::Message> {
        let mut col = Column::new().spacing(20)
            .push(
                Button::new(Text::new("Select Folder"))
                    .on_press(Message::SelectFolder)
            );

        if let Some(path) = &self.selected_path {
            col = col.push(Text::new(format!("Selected: {}", path)));
            let folder = path;
            for entry in WalkDir::new(folder).into_iter().filter_map(|e| e.ok()) {
                println!("{}", entry.path().display());
            }
        }

        col.into()
    }
}