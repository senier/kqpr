use gdk::Screen;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Builder, Button, CellRendererText, CssProvider, Entry,
    FileChooserAction, FileChooserDialog, Label, ListStore, Popover, ResponseType, Stack,
    StyleContext, TreeView, TreeViewColumn,
};
use keepass::{Database, NodeRef};
use std::fs::File;

struct Element {
    title: String,
    username: String,
    password: String,
}

fn convert_model(model: &Vec<Element>) -> ListStore {
    let result: ListStore = ListStore::new(&[String::static_type()]);
    for Element {
        title,
        username: _,
        password: _,
    } in model
    {
        result.insert_with_values(None, &[(0, title)]);
    }
    result.clone()
}

#[derive(PartialEq)]
enum State {
    Uninitialized,
    Initialized,
    Empty,
    Locked,
    Unlocked,
    Error,
}

struct StateMachine<'a> {
    state: State,
    builder: Builder,
    button_open: Button,
    button_close: Button,
    button_unlock: Button,
    view: TreeView,
    window: ApplicationWindow,
    current_entry_label: Label,
    current_password_label: Label,
    subtitle_label: Label,
    stack: Stack,
    entry_password: Entry,
    popover_incorrect_password: Popover,
    label_incorrect_password: Label,
    stack_entry_no_database: Box,
    stack_entry_database: Box,
    stack_entry_unlock: Box,
    stack_entry_password: Box,
    file: Option<&'a str>,
}

fn create<'a>() -> StateMachine<'a> {
    let builder: Builder = Builder::from_string(include_str!("ui.glade"));
    StateMachine {
        state: State::Uninitialized,
        builder: builder,
        button_open: builder
            .object("button_open")
            .expect("Open button not found"),
        button_close: builder
            .object("button_close")
            .expect("Close button not found"),
        button_unlock: builder
            .object("button_unlock")
            .expect("Unlock button not found"),
        view: builder.object("tree_entries").expect("Tree view not found"),
        window: builder.object("window_main").expect("Window not found"),
        current_entry_label: builder
            .object("current_entry")
            .expect("Current entry label not found"),
        current_password_label: builder
            .object("current_password")
            .expect("Current password label not found"),
        subtitle_label: builder
            .object("label_subtitle")
            .expect("Subtitle label not found"),
        stack: builder.object("stack").expect("Stack not found"),
        stack_entry_no_database: builder
            .object("stack_entry_no_database")
            .expect("No database stack entry not found"),
        stack_entry_database: builder
            .object("stack_entry_database")
            .expect("Database stack entry not found"),
        stack_entry_unlock: builder
            .object("stack_entry_unlock")
            .expect("Unlock stack entry not found"),
        stack_entry_password: builder
            .object("stack_entry_password")
            .expect("Password stack entry not found"),
        popover_incorrect_password: builder
            .object("popover_incorrect_password")
            .expect("Incorrect password popover not found"),
        label_incorrect_password: builder
            .object("label_incorrect_password")
            .expect("Incorrect password label not found"),
        entry_password: builder
            .object("entry_password")
            .expect("Password entry not found"),
        file: None,
    }
}

impl<'a> StateMachine<'a> {
    fn enter_initialized(&self, application: &'a Application) {
        assert!(self.state == State::Uninitialized);

        let css_provider = CssProvider::new();
        let style = include_bytes!("style.css");

        css_provider
            .load_from_data(style)
            .expect("Error loading CSS");
        StyleContext::add_provider_for_screen(
            &Screen::default().expect("Error initializing Gtk CSS provider"),
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        self.window.set_application(Some(application));
        self.window.show_all();

        application.connect_startup(|_| {
            self.enter_empty();
        });

        self.button_open.connect_clicked(|_| {
            let dialog = FileChooserDialog::new(
                Some("Open File"),
                Some(&self.window),
                FileChooserAction::Open,
            );

            dialog.add_buttons(&[
                ("Open", gtk::ResponseType::Ok),
                ("Cancel", gtk::ResponseType::Cancel),
            ]);

            dialog.connect_response(|dialog, response| {
                if response == ResponseType::Ok {
                    self.enter_locked(
                        dialog
                            .filename()
                            .expect("No filename selected")
                            .to_str()
                            .expect("Invalid filename"),
                        None,
                    );
                }
                dialog.close();
            });
            dialog.show_all();
        });

        self.button_close.connect_clicked(|_| self.enter_empty());

        self.button_unlock.connect_clicked(|_| {
            let file = File::open(std::path::Path::new(&self.file.unwrap()));
            match file {
                Err(message) => {
                    self.enter_locked(&self.file.unwrap(), Some(&message.to_string()));
                    return ();
                }
                Ok(_) => {}
            }

            let db = Database::open(& mut file.unwrap(), Some(&self.entry_password.text()), None);

            match db {
                Err(message) => {
                    self.enter_locked(&self.file.unwrap(), Some(&message.to_string()));
                }
                Ok(database) => {
                    let mut result: Vec<Element> = Vec::new();
                    for node in &database.root {
                        match node {
                            NodeRef::Group(_) => {}
                            NodeRef::Entry(e) => {
                                result.push(Element {
                                    title: e.get_title().unwrap().to_string(),
                                    username: e.get_username().unwrap().to_string(),
                                    password: e.get_password().unwrap().to_string(),
                                });
                            }
                        }
                    }
                    self.enter_unlocked(&result);
                }
            }
        });

        self.state = State::Initialized;
    }

    fn enter_empty(&self) {
        assert!(self.state == State::Initialized);

        self.stack.set_visible_child(&self.stack_entry_no_database);
        self.button_open.set_visible(true);
        self.button_close.set_visible(false);
        self.subtitle_label.set_visible(false);

        self.state = State::Empty;
    }

    fn enter_locked(&self, file: &str, error: Option<&str>) {
        assert!(self.state == State::Empty || self.state == State::Locked);

        self.file = Some(file);

        self.stack.set_visible_child(&self.stack_entry_password);
        self.button_open.set_visible(false);
        self.button_close.set_visible(true);
        self.subtitle_label.set_text(file);

        match error {
            Some(message) => {
                self.popover_incorrect_password.set_visible(true);
                self.label_incorrect_password.set_text(message);
            }
            None => {}
        }

        self.state = State::Locked;
    }

    fn enter_unlocked(&self, data: &Vec<Element>) {
        let column = TreeViewColumn::new();
        let cell = CellRendererText::new();

        column.pack_start(&cell, true);
        column.add_attribute(&cell, "text", 0);
        self.view.append_column(&column);
        self.view.set_model(Some(&convert_model(&data)));

        self.view.connect_cursor_changed(move |tree_view| {
            let (path, _) = tree_view.selection().selected_rows();
            let current_entry = &data[path[0].indices()[0] as usize];
            self.current_entry_label
                .set_label(&current_entry.username.clone());
            self.current_password_label
                .set_label(&current_entry.password.clone());
        });
    }
}

fn main() {
    let kqpr = Application::builder()
        .application_id("net.senier.kqpr")
        .build();
    let fsm = create();
    kqpr.connect_activate(move |application| {
        fsm.enter_initialized(application);
    });
    kqpr.run();
}
