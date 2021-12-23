use gdk::Screen;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio::{Cancellable, MemoryInputStream};
use gtk::glib::{signal::SignalHandlerId, Bytes, MainContext, PRIORITY_DEFAULT};
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Builder, Button, CellRendererText, CssProvider, Entry,
    FileChooserAction, FileChooserDialog, FileFilter, Image, Label, ListStore, Popover,
    ResponseType, Stack, StyleContext, ToggleButton, TreeModel, TreeView, TreeViewColumn,
};
use keepass::{Database, NodeRef};
use qrcode::{render::svg, QrCode};
use std::{cell::RefCell, fs::File, path::PathBuf, rc::Rc, thread};

struct Element {
    title: String,
    username: String,
    password: String,
}

#[derive(PartialEq, Clone, Debug)]
enum State {
    Initialized,
    Empty,
    Locked,
    Unlocked,
}

struct Context {
    current: State,
    view_signal_id: RefCell<Option<SignalHandlerId>>,
    file: Option<PathBuf>,
}

#[derive(glib::Downgrade)]
pub struct UI {
    context: Rc<RefCell<Context>>,
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
    stack_entry_loading_database: Box,
    popover_incorrect_password: Popover,
    label_incorrect_password: Label,
    entry_password: Entry,
    toggle_show_password: ToggleButton,
    image_icon_no_database: Image,
}

impl UI {
    fn new() -> UI {
        let css_provider = CssProvider::new();
        let style = include_bytes!("../res/style.css");
        css_provider
            .load_from_data(style)
            .expect("Error loading CSS");
        StyleContext::add_provider_for_screen(
            &Screen::default().expect("Error initializing Gtk CSS provider"),
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let builder: Builder = Builder::from_string(include_str!("../res/ui.glade"));
        UI {
            context: Rc::new(RefCell::new(Context {
                current: State::Initialized,
                view_signal_id: RefCell::new(None),
                file: None,
            })),
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
            stack_entry_loading_database: builder
                .object("stack_entry_loading_database")
                .expect("No database loading stack entry not found"),
            popover_incorrect_password: builder
                .object("popover_incorrect_password")
                .expect("Incorrect password popover not found"),
            label_incorrect_password: builder
                .object("label_incorrect_password")
                .expect("Incorrect password label not found"),
            entry_password: builder
                .object("entry_password")
                .expect("Password entry not found"),
            toggle_show_password: builder
                .object("toggle_show_password")
                .expect("Show password toggle button not found"),
            image_icon_no_database: builder
                .object("image_icon_no_database")
                .expect("Icon image not found"),
        }
    }

    fn initialize(&self) {
        let column = TreeViewColumn::new();
        let cell = CellRendererText::new();
        column.pack_start(&cell, true);
        column.add_attribute(&cell, "text", 0);

        self.view.append_column(&column);

        self.ui_switch_empty();

        self.button_open
            .connect_clicked(glib::clone!(@weak self as ui => move |_| {
                ui.ui_switch_locked();
            }));

        self.button_close
            .connect_clicked(glib::clone!(@weak self as ui => move |_| {
                ui.ui_switch_empty();
            }));

        self.button_unlock
            .connect_clicked(glib::clone!(@weak self as ui => move |_| {
                ui.ui_switch_unlocked();
            }));

        self.entry_password
            .connect_activate(glib::clone!(@weak self as ui => move |_| {
                ui.ui_switch_unlocked();
            }));

        self.toggle_show_password
            .connect_clicked(glib::clone!(@weak self as ui => move |_| {
                ui.entry_password.set_visibility(ui.toggle_show_password.is_active());
            }));

        if let Ok(pixbuf) = Pixbuf::from_stream::<MemoryInputStream, Cancellable>(
            &MemoryInputStream::from_bytes(&Bytes::from(include_bytes!("../res/icon.svg"))),
            None,
        ) {
            self.image_icon_no_database.set_from_pixbuf(Some(&pixbuf));
        }

        self.window.show_all();
    }

    fn file_chooser(&self) -> FileChooserDialog {
        let dialog = FileChooserDialog::new(
            Some("Open File"),
            Some(&self.window),
            FileChooserAction::Open,
        );

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

    fn ui_switch_locked(self) {
        assert!(self.context.borrow().current == State::Empty);
        let dialog = self.file_chooser();
        dialog.connect_response(glib::clone!(@weak self as ui => move |dialog, response| {
            let mut context = self.context.borrow_mut();
            if response == ResponseType::Ok {
                context.file = Some(dialog.filename().expect("No filename selected"));
                ui.stack.set_visible_child(&ui.stack_entry_password);
                ui.button_open.set_visible(false);
                ui.button_close.set_visible(true);
                ui.button_unlock.set_sensitive(true);
                ui.subtitle_label.set_text(context.file.clone().unwrap().to_str().unwrap());
                ui.subtitle_label.set_visible(true);
                context.current = State::Locked;
            }
            dialog.close();
        }));
        dialog.show_all();
    }

    fn ui_switch_empty(&self) {
        let mut context = self.context.borrow_mut();
        assert!(context.current != State::Empty);
        self.stack.set_visible_child(&self.stack_entry_no_database);
        self.button_open.set_visible(true);
        self.button_close.set_visible(false);
        self.subtitle_label.set_visible(false);
        self.view.set_model::<TreeModel>(None);
        if let Some(id) = context.view_signal_id.borrow_mut().take() {
            self.view.disconnect(id);
        }
        context.current = State::Empty;
    }

    fn set_model(&self, model: &Vec<Element>) {
        let data: ListStore = ListStore::new(&[String::static_type()]);

        for Element {
            title,
            username: _,
            password: _,
        } in model
        {
            data.set(&data.append(), &[(0, title)]);
        }
        self.view.set_model(Some(&data));
    }

    fn wifi_qr_code(&self, username: &str, password: &str) -> MemoryInputStream {
        let qr_data = format!("WIFI:S:{};T:WPA2;P:{};;", &username, &password);
        let code =
            QrCode::new(qr_data.as_bytes()).unwrap();
        let image = code
            .render()
            .min_dimensions(200, 200)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("rgba(0,0,0,0)"))
            .build();
        MemoryInputStream::from_bytes(&Bytes::from(image.as_bytes()))
    }

    fn display_database(&self, database: Database) -> SignalHandlerId {
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
        self.set_model(&data);

        return self.view
            .connect_cursor_changed(glib::clone!(@weak self as ui => move |tree_view| {
                if let Ok(context) = ui.context.try_borrow() {
                    assert!(context.current == State::Unlocked);
                    let (path, _) = tree_view.selection().selected_rows();
                    let entry = &data[path[0].indices()[0] as usize];
                    let qr_code = ui.wifi_qr_code(&entry.username.clone(), &entry.password.clone());
                    if let Ok(pixbuf) = Pixbuf::from_stream::<MemoryInputStream, Cancellable>(&qr_code, None) {
                        ui.image_qr_code.set_from_pixbuf(Some(&pixbuf));
                    }
                    ui.current_entry_label.set_label(&entry.username.clone());
                    ui.image_qr_code.set_visible(true);
                };
            }));
    }

    fn ui_show_error(&self, message: &str) {
        self.stack.set_visible_child(&self.stack_entry_password);
        self.button_open.set_visible(false);
        self.button_close.set_visible(true);
        self.subtitle_label.set_visible(true);
        self.popover_incorrect_password.set_visible(true);
        self.label_incorrect_password.set_text(message);
    }

    fn ui_switch_unlocked(self) {
        let context = self.context.borrow();
        let filename = context.file.clone().unwrap().into_os_string();
        let password = self.entry_password.text().as_str().to_string();
        assert!(context.current == State::Locked);

        let (sender, receiver) = MainContext::channel(PRIORITY_DEFAULT);

        self.stack
            .set_visible_child(&self.stack_entry_loading_database);
        self.button_open.set_visible(false);
        self.button_close.set_visible(false);
        self.button_unlock.set_sensitive(false);
        self.entry_password.set_sensitive(false);

        thread::spawn(move || match File::open(filename) {
            Ok(mut file) => match Database::open(&mut file, Some(password.as_str()), None) {
                Ok(database) => {
                    let _ = sender.send(Ok(database));
                }
                Err(message) => {
                    let _ = sender.send(Err(message.to_string()));
                }
            },
            Err(message) => {
                let _ = sender.send(Err(message.to_string()));
            }
        });

        receiver.attach(None, glib::clone!(@weak self as ui => @default-return glib::Continue(false), move |database| {
            let mut context = ui.context.borrow_mut();
            match database {
                Ok(db) => {
                    ui.stack.set_visible_child(&ui.stack_entry_database);
                    ui.button_open.set_visible(false);
                    ui.button_close.set_visible(true);
                    ui.subtitle_label.set_visible(true);
                    ui.image_qr_code.set_visible(false);
                    ui.entry_password.set_text("");
                    context.view_signal_id = RefCell::new(Some(ui.display_database(db)));
                    context.current = State::Unlocked;
                }
                Err(message) => {
                    ui.ui_show_error(&message.to_string());
                }
            }
            ui.button_unlock.set_sensitive(true);
            ui.entry_password.set_sensitive(true);
            glib::Continue(true)
        }));
    }
}

fn main() {
    gtk::init().expect("Failed to init Gtk");
    let app = Application::builder()
        .application_id("net.senier.kqpr")
        .build();
    app.connect_startup(move |application| {
        let ui = UI::new();
        ui.window.set_application(Some(application));
        application.connect_activate(move |_| {
            ui.initialize();
        });
    });
    app.run();
}
