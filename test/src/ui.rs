use iced::widget::{button, column, row, text};
use iced::{executor, Application, Command, Element, Settings, Theme};

pub fn ui() -> iced::Result {
    Backup::run(Settings::default())
}

#[derive(Default)]
struct Backup {
    data: String,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Select,
    Backup,
}

impl Application for Backup {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("Iced - Backup")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::Select => {
                println!("Select file clicked");
            }
            Message::Backup => {
                println!("Backup file clicked");
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let content = column![
            text("FASS Backup").size(28),
            row![
                button("Select file").on_press(Message::Select),
                button("Backup file").on_press(Message::Backup),
            ]
            .spacing(12),
        ]
        .spacing(16)
        .padding(16);

        content.into()
    }
}
