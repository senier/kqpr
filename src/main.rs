use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Builder, CellRendererText, Label, TreeView, TreeViewColumn, ListStore};
use keepass::{Database, NodeRef};
use std::fs::File;
use std::error::Error;

struct Entry {
    title: String,
    username: String,
    password: String,
}

fn create_model(file: &str, password: &str) -> Result<Vec<Entry>, Box<dyn Error>> {
    let db = Database::open(
        &mut File::open(std::path::Path::new(file))?,
        Some(password),
        None
    )?;

    let mut result: Vec<Entry> = Vec::new();

    for node in &db.root {
        match node {
            NodeRef::Group(_) => {},
            NodeRef::Entry(e) => {
                result.push(Entry {
                                title: e.get_title().unwrap().to_string(),
                                username: e.get_username().unwrap().to_string(),
                                password: e.get_password().unwrap().to_string(),
                            });
            }
        }
    }

    Ok(result)
}

fn convert_model(model: &Vec<Entry>) -> ListStore {
    let result: ListStore = ListStore::new(&[String::static_type()]);
    for Entry { title, username: _, password: _ } in model {
        result.insert_with_values(None, &[(0, title)]);
    }
    result.clone()
}

fn build_ui(application: &Application) {

    let model = create_model("tests/data/Passwords.kdbx", "demopass");

    let builder: Builder = Builder::from_string(include_str!("ui.glade"));
    let view: TreeView = builder.object("tree_entries").expect("Tree view not found");

    let window: ApplicationWindow = builder.object("window_main").expect("Window not found");

    window.set_application(Some(application));
    window.show_all();

    let current_entry_label: Label = builder.object("current_entry").expect("Current entry label not found");
    let current_password_label: Label = builder.object("current_password").expect("Current password label not found");
    let column = TreeViewColumn::new();
    let cell = CellRendererText::new();

    column.pack_start(&cell, true);
    column.add_attribute(&cell, "text", 0);
    view.append_column(&column);

    match model {
        Err(why) => panic!("Invalid: {}", why),
        Ok(ref data) => view.set_model(Some(&convert_model(&data))),
    }

    view.connect_cursor_changed(move |tree_view| {
        match &model {
            Ok(ref data) => {
                    let (path, _) = tree_view.selection().selected_rows();
                    let current_entry = &data[path[0].indices()[0] as usize];
                    current_entry_label.set_label(&current_entry.username.clone());
                    current_password_label.set_label(&current_entry.password.clone());
                },
            Err(why) => panic!("List should be disabled when no keys are loaded: {}", why)
        }
    });
}

fn main() -> Result<(), Box<dyn Error>>{
    let kqpr = Application::builder()
        .application_id("net.senier.kqpr")
        .build();
    kqpr.connect_activate(build_ui);
    kqpr.run();
    Ok(())
}
