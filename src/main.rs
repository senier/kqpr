use gdk::Screen;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio::{Cancellable, MemoryInputStream};
use gtk::glib::Bytes;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Builder, Button, CellRendererText, CssProvider, Entry,
    FileChooserAction, FileChooserDialog, FileFilter, Image, Label, ListStore, Popover,
    ResponseType, Stack, StyleContext, TreeView, TreeViewColumn,
};
use keepass::{Database, NodeRef};
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode, Version};
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
    Empty,
    Locked,
    Unlocked,
}

#[derive(Clone)]
struct Context {
    current: State,
    file: Option<PathBuf>,
    window: ApplicationWindow,
    button_open: Button,
    button_close: Button,
    button_unlock: Button,
    view: TreeView,
    current_entry_label: Label,
    image_qr_code: Image,
    subtitle_label: Label,
    stack: Stack,
    stack_entry_no_database: Box,
    stack_entry_database: Box,
    stack_entry_password: Box,
    popover_incorrect_password: Popover,
    label_incorrect_password: Label,
    entry_password: Entry,
}

impl Context {
    fn new() -> Context {
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
        Context {
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
            image_qr_code: builder
                .object("image_qr_code")
                .expect("QR code image not found"),
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
            current: State::Empty,
            file: None,
        }
    }

    fn ui_switch_locked(&mut self, file: PathBuf) {
        self.file = Some(file);
        self.stack.set_visible_child(&self.stack_entry_password);
        self.button_open.set_visible(false);
        self.button_close.set_visible(true);
        self.subtitle_label
            .set_text(self.file.clone().unwrap().to_str().unwrap());
        self.subtitle_label.set_visible(true);
        self.current = State::Locked;
    }
}

fn file_chooser(window: &ApplicationWindow) -> FileChooserDialog {
    let dialog = FileChooserDialog::new(Some("Open File"), Some(window), FileChooserAction::Open);

    dialog.add_buttons(&[
        ("Open", gtk::ResponseType::Ok),
        ("Cancel", gtk::ResponseType::Cancel),
    ]);

    let filter = FileFilter::new();
    filter.add_pattern("*.kdbx");
    filter.set_name(Some("KDBX 4 password database"));
    dialog.add_filter(&filter);

    dialog
}

fn kqpr(application: &Application) {
    let context = Rc::new(RefCell::new(Context::new()));

    context.borrow().window.set_application(Some(application));

    // State::Open
    let open_context = context.clone();
    context.borrow().button_open.connect_clicked(move |_| {
        let context = open_context.borrow_mut();
        assert!(context.current == State::Empty);

        let dialog = file_chooser(&context.window);
        let dialog_context = open_context.clone();
        dialog.connect_response(move |dialog, response| {
            if response == ResponseType::Ok {
                dialog_context
                    .borrow_mut()
                    .ui_switch_locked(dialog.filename().expect("No filename selected"));
            }
            dialog.close();
        });
        dialog.show_all();
    });

    let close_context = context.clone();
    context.borrow().button_close.connect_clicked(move |_| {
        let mut context = close_context.borrow_mut();
        assert!(context.current == State::Locked || context.current == State::Unlocked);

        context
            .stack
            .set_visible_child(&context.stack_entry_no_database);
        context.button_open.set_visible(true);
        context.button_close.set_visible(false);
        context.subtitle_label.set_visible(false);
        context.current = State::Empty;
    });

    let unlock_context = context.clone();
    context.borrow().button_unlock.connect_clicked(move |_| {
        let mut context = unlock_context.borrow_mut();
        assert!(context.current == State::Locked);

        let name = File::open(context.file.clone().unwrap().into_os_string());
        match name {
            Err(message) => {
                context
                    .stack
                    .set_visible_child(&context.stack_entry_password);
                context.button_open.set_visible(false);
                context.button_close.set_visible(true);
                context.subtitle_label.set_visible(true);
                context.popover_incorrect_password.set_visible(true);
                context
                    .label_incorrect_password
                    .set_text(&message.to_string());
                return ();
            }
            Ok(_) => {}
        }

        let db = Database::open(
            &mut name.unwrap(),
            Some(&context.entry_password.text()),
            None,
        );

        match db {
            Err(message) => {
                context
                    .stack
                    .set_visible_child(&context.stack_entry_password);
                context.button_open.set_visible(false);
                context.button_close.set_visible(true);
                context.subtitle_label.set_visible(true);
                context.popover_incorrect_password.set_visible(true);
                context
                    .label_incorrect_password
                    .set_text(&message.to_string());
                return ();
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
                context.view.append_column(&column);
                context.view.set_model(Some(&convert_model(&data)));

                let cursor_changed_context = unlock_context.clone();
                context.view.connect_cursor_changed(move |tree_view| {
                    let context = cursor_changed_context.borrow();
                    let (path, _) = tree_view.selection().selected_rows();
                    let current_entry = &data[path[0].indices()[0] as usize];
                    let qr_data = format!(
                        "WIFI:S:{};T:WPA2;P:{};;",
                        &current_entry.username.clone(),
                        &current_entry.password.clone()
                    );
                    let code =
                        QrCode::with_version(qr_data.as_bytes(), Version::Normal(4), EcLevel::L)
                            .unwrap();
                    let image = code
                        .render()
                        .min_dimensions(200, 200)
                        .dark_color(svg::Color("#000000"))
                        .light_color(svg::Color("rgba(0,0,0,0)"))
                        .build();
                    let s = MemoryInputStream::from_bytes(&Bytes::from(image.as_bytes()));

                    context
                        .current_entry_label
                        .set_label(&current_entry.username.clone());

                    match Pixbuf::from_stream::<MemoryInputStream, Cancellable>(&s, None) {
                        Err(why) => {
                            println!("Error: {}", why);
                        }
                        Ok(pixbuf) => {
                            context.image_qr_code.set_from_pixbuf(Some(&pixbuf));
                        }
                    }
                });
                context
                    .stack
                    .set_visible_child(&context.stack_entry_database);
                context.button_open.set_visible(false);
                context.button_close.set_visible(true);
                context.subtitle_label.set_visible(true);
                context.entry_password.set_text("");

                context.current = State::Unlocked;
            }
        }
    });

    application.connect_activate(move |_| {
        context.borrow().window.show_all();
    });
}

fn main() {
    let app = Application::builder()
        .application_id("net.senier.kqpr")
        .build();
    app.connect_startup(move |application| {
        kqpr(application);
    });
    app.run();
}
