// Copyright 2021, Alexander Senier
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use gdk::Screen;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio::{Cancellable, MemoryInputStream};
use gtk::glib::{signal::SignalHandlerId, Bytes, MainContext, PRIORITY_DEFAULT};
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Builder, Button, CellRendererText, CssProvider, Entry,
    FileChooserAction, FileChooserDialog, FileFilter, Image, Label, ListStore, Popover,
    ResponseType, SearchEntry, Stack, StyleContext, ToggleButton, TreeModelFilter, TreeView,
    TreeViewColumn,
};
use keepass::{Database, NodeRef};
use qrcode::{render::svg, QrCode};
use std::{cell::RefCell, fs::File, path::PathBuf, rc::Rc, thread};

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
    view_filter: Option<TreeModelFilter>,
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
    label_version: Label,
    entry_password: Entry,
    toggle_show_password: ToggleButton,
    image_icon_no_database: Image,
    entry_search: SearchEntry,
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
                view_filter: None,
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
            label_version: builder
                .object("label_version")
                .expect("Version label not found"),
            entry_password: builder
                .object("entry_password")
                .expect("Password entry not found"),
            toggle_show_password: builder
                .object("toggle_show_password")
                .expect("Show password toggle button not found"),
            image_icon_no_database: builder
                .object("image_icon_no_database")
                .expect("Icon image not found"),
            entry_search: builder
                .object("entry_search")
                .expect("Search entry not found"),
        }
    }

    fn initialize(&self) {
        let column = TreeViewColumn::new();
        let cell = CellRendererText::new();
        let name = option_env!("CARGO_PKG_NAME").unwrap_or("KQPR");
        let version = option_env!("CARGO_PKG_VERSION").unwrap_or("version unknown");
        column.pack_start(&cell, true);
        column.add_attribute(&cell, "text", 0);

        self.view.append_column(&column);
        self.label_version
            .set_text(format!("{} ({})", name, version).as_str());

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

        self.entry_search
            .connect_text_notify(glib::clone!(@weak self as ui => move |_| {
                ui.context.borrow().view_filter.as_ref().unwrap().refilter();
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
                ui.entry_password.grab_focus();
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
        self.view.set_model::<TreeModelFilter>(None);
        if let Some(id) = context.view_signal_id.borrow_mut().take() {
            self.view.disconnect(id);
        }
        context.current = State::Empty;
    }

    fn set_model(&self, database: Database) {
        let mut context = self.context.borrow_mut();
        let list_store: ListStore = ListStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
        ]);

        for node in &database.root {
            match node {
                NodeRef::Group(_) => {}
                NodeRef::Entry(e) => {
                    let title = e.get_title().unwrap().to_string();
                    let username = e.get_username().unwrap().to_string();
                    for data in [&title, &username] {
                        for pattern in ["wifi", "wi-fi", "wlan", "wireless", "wpa"] {
                            if data.to_lowercase().contains(pattern) {
                                list_store.set(
                                    &list_store.append(),
                                    &[
                                        (0, &title),
                                        (1, &username),
                                        (2, &e.get_password().unwrap().to_string()),
                                    ],
                                );
                            }
                        }
                    }
                }
            }
        }
        context.view_filter = Some(TreeModelFilter::new(&list_store, None));
        context.view_filter.as_ref().unwrap().set_visible_func(
            glib::clone!(@weak self as ui => @default-return true, move |model, iter| {
                let search_text = ui.entry_search.text().as_str().to_string();
                let title = model.value(&iter, 0).get::<String>().expect("Invalid title");
                let username = model.value(&iter, 1).get::<String>().expect("Invalid username");
                title.contains(&search_text) || username.contains(&search_text)
            }),
        );
        self.view
            .set_model::<TreeModelFilter>(context.view_filter.as_ref());
    }

    fn wifi_qr_code(&self, title: &str, username: &str, password: &str) -> MemoryInputStream {
        let wifi_type = {
            let title = title.to_lowercase();
            if title.contains("[wpa3]") {
                "WPA3"
            } else if title.contains("[wpa2]") {
                "WPA2"
            } else if title.contains("[wpa]") {
                "WPA"
            } else if title.contains("[wep]") {
                "WEP"
            } else {
                "WPA2"
            }
        };
        let qr_data = format!("WIFI:S:{};T:{};P:{};;", &username, &wifi_type, &password);
        let code = QrCode::new(qr_data.as_bytes()).unwrap();
        let image = code
            .render()
            .min_dimensions(200, 200)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("rgba(0,0,0,0)"))
            .build();
        MemoryInputStream::from_bytes(&Bytes::from(image.as_bytes()))
    }

    fn display_database(&self, database: Database) -> SignalHandlerId {
        self.set_model(database);
        return self.view
            .connect_cursor_changed(glib::clone!(@weak self as ui => move |tree_view| {
                if let Ok(context) = ui.context.try_borrow() {
                    assert!(context.current == State::Unlocked);
                    if let Some((model, iter)) = tree_view.selection().selected() {
                        let title = model.value(&iter, 0).get::<String>().expect("Invalid title");
                        let username = model.value(&iter, 1).get::<String>().expect("Invalid username");
                        let password = model.value(&iter, 2).get::<String>().expect("Invalid password");
                        let qr_code = ui.wifi_qr_code(&title, &username, &password);
                        if let Ok(pixbuf) = Pixbuf::from_stream::<MemoryInputStream, Cancellable>(&qr_code, None) {
                            ui.image_qr_code.set_from_pixbuf(Some(&pixbuf));
                        }
                        ui.current_entry_label.set_label(&username);
                        ui.image_qr_code.set_visible(true);
                    } else {
                        ui.current_entry_label.set_label("");
                        ui.image_qr_code.set_visible(false);
                    }
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
            match database {
                Ok(db) => {
                    ui.stack.set_visible_child(&ui.stack_entry_database);
                    ui.button_open.set_visible(false);
                    ui.button_close.set_visible(true);
                    ui.subtitle_label.set_visible(true);
                    ui.image_qr_code.set_visible(false);
                    ui.entry_password.set_text("");
                    let database = ui.display_database(db);
                    {
                        let mut context = ui.context.borrow_mut();
                        context.view_signal_id = RefCell::new(Some(database));
                        context.current = State::Unlocked;
                    }
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
