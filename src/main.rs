use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Builder, CellRendererText, Label, TreeView, TreeViewColumn, ListStore};
use keepass::{Database, NodeRef};
use std::fs::File;
use std::error::Error;

fn create_model(file: &str, password: &str) -> Result<ListStore, Box<dyn Error>> {
    let db = Database::open(
        &mut File::open(std::path::Path::new(file))?,
        Some(password),
        None
    )?;

    let result = ListStore::new(&[String::static_type()]);

    for node in &db.root {
        match node {
            NodeRef::Group(_) => {},
            NodeRef::Entry(e) => {
        		result.insert_with_values(None, &[(0, &e.get_title().unwrap())]);
            }
        }
    }

    Ok(result)
}

fn main() -> Result<(), Box<dyn Error>>{
    let kqpr = Application::builder()
        .application_id("net.senier.kqpr")
        .build();

    kqpr.connect_activate(|application| {

        let builder: Builder = Builder::from_string(include_str!("ui.glade"));
        let view: TreeView = builder.object("tree_entries").expect("Tree view not found");

        let model = create_model("tests/data/Passwords.kdbx", "demopass");
        let window: ApplicationWindow = builder.object("window_main").expect("Window not found");

        window.set_application(Some(application));
        window.show_all();

        let current_entry: Label = builder.object("current_entry").expect("Current entry label not found");

        let column = TreeViewColumn::new();
        let cell = CellRendererText::new();
        column.pack_start(&cell, true);
        column.add_attribute(&cell, "text", 0);
        view.append_column(&column);

		view.set_model(Some(&model.expect("Error loading DB")));
        view.connect_cursor_changed(move |tree_view| {
            let selection = tree_view.selection();
            if let Some((model, iter)) = selection.selected() {
                current_entry.set_label(&model.value(&iter, 0).get::<String>().expect("Entry not found"));
            }
        });
    });

    kqpr.run();
    Ok(())
}
