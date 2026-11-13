mod application;
mod config;
mod settings;
mod theme_injector;
mod webview;
mod window;

fn main() -> glib::ExitCode {
    // Compiled by build.rs from data/resources.gresource.xml.
    // Registering this before the application starts makes the icon available
    // to the icon theme, the About window and the window manager.
    gio::resources_register_include!("pintrust.gresource")
        .expect("Failed to register embedded resources");

    application::run()
}
