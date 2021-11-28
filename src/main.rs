use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Builder, CellRendererText, Label, TreeView, TreeViewColumn, ListStore};

fn create_and_fill_model() -> ListStore {
    let model = ListStore::new(&[String::static_type()]);

    let entries = &["e1", "e2", "e3", "e4", "e5", "e6"];
    for entry in entries.iter() {
        model.insert_with_values(None, &[(0, &entry)]);
    }
    model
}

fn main() {
    let kqpr = Application::builder()
        .application_id("net.senier.kqpr")
        .build();

    kqpr.connect_activate(|application| {

        let builder: Builder = Builder::from_string(include_str!("ui.glade"));
        let view: TreeView = builder.object("tree_entries").expect("Tree view not found");
        let model: ListStore = create_and_fill_model();
        let window: ApplicationWindow = builder.object("window_main").expect("Window not found");

        window.set_application(Some(application));
        window.show_all();

        let current_entry: Label = builder.object("current_entry").expect("Current entry label not found");

        let column = TreeViewColumn::new();
        let cell = CellRendererText::new();
        column.pack_start(&cell, true);
        column.add_attribute(&cell, "text", 0);
        view.append_column(&column);

		view.set_model(Some(&model));
        view.connect_cursor_changed(move |tree_view| {
            let selection = tree_view.selection();
            if let Some((model, iter)) = selection.selected() {
                current_entry.set_label(&model.value(&iter, 0).get::<String>().expect("Entry not found"));
            }
        });
    });

    kqpr.run();
}
