use gdk::Screen;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Builder, Button, CellRendererText, CssProvider, Entry,
    FileChooserAction, FileChooserDialog, Label, ListStore, Popover, ResponseType, Stack,
    StyleContext, TreeView, TreeViewColumn,
};
use keepass::{Database, NodeRef};
use std::cell::RefCell;
use std::fs::File;
use std::path::PathBuf;
use std::rc::Rc;

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

#[derive(PartialEq, Clone)]
enum State {
    Uninitialized,
    Empty,
    Locked,
    Unlocked,
}

#[derive(Clone)]
struct Context {
    current: State,
    file: Option<PathBuf>,
}

#[derive(Clone)]
struct UI {
    window: ApplicationWindow,
    button_open: Button,
    button_close: Button,
    button_unlock: Button,
    view: TreeView,
    current_entry_label: Label,
    current_password_label: Label,
    subtitle_label: Label,
    stack: Stack,
    stack_entry_no_database: Box,
    stack_entry_database: Box,
    stack_entry_unlock: Box,
    stack_entry_password: Box,
    popover_incorrect_password: Popover,
    label_incorrect_password: Label,
    entry_password: Entry,
}

impl UI {
    fn new() -> UI {
        let builder: Builder = Builder::from_string(include_str!("ui.glade"));
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
        UI {
            window: builder.object("window_main").expect("Window not found"),
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
        }
    }
}

fn fsm(application: &Application) {
    let ui = Rc::new(UI::new());
    let context = RefCell::new(Context {
        current: State::Uninitialized,
        file: None,
    });

    ui.window.set_application(Some(application));

    // Open
    let open_context = context.clone();
    ui.button_open
        .connect_clicked(glib::clone!(@weak ui => move |_| {
            assert!(open_context.borrow().current == State::Empty);

            let dialog = FileChooserDialog::new(
                Some("Open File"),
                Some(&ui.window),
                FileChooserAction::Open,
            );

            dialog.add_buttons(&[
                ("Open", gtk::ResponseType::Ok),
                ("Cancel", gtk::ResponseType::Cancel),
            ]);

            let dialog_context = open_context.clone();
            dialog.connect_response(move |dialog, response| {
                if response == ResponseType::Ok {
                    dialog_context.borrow_mut().file = Some(dialog.filename().expect("No filename selected"));
                    dialog_context.borrow_mut().current = State::Locked;
                }
                dialog.close();
            });
            dialog.show_all();
        }));

    let close_context = context.clone();
    ui.button_close
        .connect_clicked(glib::clone!(@weak ui => move |_| {
            assert!(close_context.borrow().current == State::Locked || close_context.borrow().current == State::Unlocked);

            ui.stack.set_visible_child(&ui.stack_entry_no_database);
            ui.button_open.set_visible(true);
            ui.button_close.set_visible(false);
            ui.subtitle_label.set_visible(false);
            close_context.borrow_mut().current = State::Empty;
        }));

    ui.button_unlock
        .connect_clicked(glib::clone!(@weak ui => move |_| {
            assert!(context.borrow().current == State::Locked);

            let name = File::open(context.borrow().file.clone().unwrap().into_os_string());
            match name {
                Err(message) => {
                    ui.stack.set_visible_child(&ui.stack_entry_password);
                    ui.button_open.set_visible(false);
                    ui.button_close.set_visible(true);
                    ui.subtitle_label.set_visible(false);
                    ui.popover_incorrect_password.set_visible(true);
                    ui.label_incorrect_password.set_text(&message.to_string());
                    return ();
                }
                Ok(_) => {}
            }

            let db = Database::open(& mut name.unwrap(), Some(&ui.entry_password.text()), None);

            match db {
                Err(message) => {
                    ui.stack.set_visible_child(&ui.stack_entry_password);
                    ui.button_open.set_visible(false);
                    ui.button_close.set_visible(true);
                    ui.subtitle_label.set_visible(false);
                    ui.popover_incorrect_password.set_visible(true);
                    ui.label_incorrect_password.set_text(&message.to_string());
                }
                Ok(database) => {
                    let mut data: Vec<Element> = Vec::new();
                    for node in &database.root {
                        match node {
                            NodeRef::Group(_) => {}
                            NodeRef::Entry(e) => {
                                data.push(Element {
                                    title: e.get_title().unwrap().to_string(),
                                    username: e.get_username().unwrap().to_string(),
                                    password: e.get_password().unwrap().to_string(),
                                });
                            }
                        }
                    }
                    let column = TreeViewColumn::new();
                    let cell = CellRendererText::new();

                    column.pack_start(&cell, true);
                    column.add_attribute(&cell, "text", 0);
                    ui.view.append_column(&column);
                    ui.view.set_model(Some(&convert_model(&data)));

                    ui.view.connect_cursor_changed(glib::clone!(@weak ui => move |tree_view| {
                        let (path, _) = tree_view.selection().selected_rows();
                        let current_entry = &data[path[0].indices()[0] as usize];
                        ui.current_entry_label
                            .set_label(&current_entry.username.clone());
                        ui.current_password_label
                            .set_label(&current_entry.password.clone());
                    }));
                }
            }
        }));
    ui.window.show_all();
}

fn main() {
    let kqpr = Application::builder()
        .application_id("net.senier.kqpr")
        .build();
    kqpr.connect_activate(move |application| {
        fsm(application);
    });
    kqpr.run();
}
