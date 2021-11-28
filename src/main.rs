use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Builder};

fn main() {
    let kqpr = Application::builder()
        .application_id("net.senier.kqpr")
        .build();

    kqpr.connect_activate(|application| {
        let window: ApplicationWindow = Builder::from_string(include_str!("ui.glade"))
                                            .object("window_main")
                                            .expect("Window not found");
        window.set_application(Some(application));
        window.show_all();
    });

    kqpr.run();
}
