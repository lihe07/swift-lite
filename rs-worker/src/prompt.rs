//! Windows-only modal that asks for the worker name.
//!
//! Compiled only on Windows (the `mod prompt;` declaration in `main.rs` is
//! gated behind `#[cfg(windows)]`), so the GUI dependencies never reach the
//! Linux build.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_derive::NwgUi;
use native_windows_gui::{self as nwg, NativeUi};

#[derive(Default, NwgUi)]
pub struct NamePrompt {
    #[nwg_control(size: (340, 140), position: (400, 300), title: "rs-worker", flags: "WINDOW|VISIBLE")]
    #[nwg_events( OnWindowClose: [NamePrompt::on_close] )]
    window: nwg::Window,

    #[nwg_control(text: "Enter this worker's name:", size: (300, 22), position: (20, 18))]
    label: nwg::Label,

    #[nwg_control(text: "worker", size: (300, 26), position: (20, 48), focus: true)]
    #[nwg_events( OnKeyEnter: [NamePrompt::on_ok] )]
    input: nwg::TextInput,

    #[nwg_control(text: "OK", size: (90, 32), position: (230, 92))]
    #[nwg_events( OnButtonClick: [NamePrompt::on_ok] )]
    ok_button: nwg::Button,

    /// Holds the confirmed name. Stays `None` if the window is closed without
    /// pressing OK / Enter.
    result: Rc<RefCell<Option<String>>>,
}

impl NamePrompt {
    fn on_ok(&self) {
        *self.result.borrow_mut() = Some(self.input.text());
        nwg::stop_thread_dispatch();
    }

    fn on_close(&self) {
        nwg::stop_thread_dispatch();
    }
}

/// Show a modal asking for the worker name.
///
/// Returns the trimmed input, or `None` if the dialog was closed without
/// confirming or the field was left empty.
pub fn ask_worker_name() -> Option<String> {
    nwg::init().ok()?;
    let _ = nwg::Font::set_global_family("Segoe UI");

    let app = NamePrompt::build_ui(Default::default()).ok()?;
    nwg::dispatch_thread_events();

    let confirmed = app.result.borrow().clone();
    let trimmed = confirmed?.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
