mod runtime;
mod ui;

pub use makepad_widgets;
use makepad_widgets::*;
use ui::app::App;

app_main!(App, font_set: System);
