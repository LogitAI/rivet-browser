//! The operating system layer: every Windows call a feature needs, behind traits.
//!
//! The runtime has three layers. This one owns the Windows calls. The browser access layer in
//! `crate::browser` owns everything that touches Chromium. The features in `crate::features` use
//! traits from these two layers only, so a feature never names `windows_sys`, never writes
//! `unsafe` and never reads a build row. A feature test swaps both layers for small test
//! implementations and runs without a browser. Supporting another operating system means writing
//! this layer again, not the features.
//!
//! `frame.rs` is the composition root. It picks the real adapters, `os::win32::Win32` here and
//! `browser::chromium::Chromium`, and hands them to the features. Nothing else chooses an
//! adapter.
//!
//! The traits grow one moved feature at a time, from what the features call today. The groups
//! the features use, and the trait each group becomes:
//!
//! | Group | Used by today | Trait |
//! |---|---|---|
//! | Command messages to a window | page_bar, tabs | `WindowCommands` |
//! | Posted messages and subclassing | address_bar, control, mini_window, second_view | window messages |
//! | Keyboard and pointer input | address_bar, command_bar, control, mini_window, settings | input |
//! | DirectComposition visuals | address_bar, command_bar, mini_window, settings | composition |
//! | Window shape, cloaking and corners | control, look, window_corners | window shape |
//! | Monitors and DPI | link_preview, mini_window, second_view, split | monitors |
//! | Processes, pipes and files | app_window, control, side_panel | processes |
//!
//! Only `WindowCommands` exists so far. A trait is added when the first feature that needs it
//! moves, never before, so the surface is always one that a feature really uses.

pub(crate) mod win32;

/// A top-level browser window, as the layers above the operating system see it.
///
/// It is a plain value, so a feature can keep and compare it without touching Windows types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowId(usize);

impl WindowId {
    pub(crate) const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    pub(crate) const fn raw(self) -> usize {
        self.0
    }
}

/// Delivers menu command messages to a window.
pub(crate) trait WindowCommands {
    /// Sends command `id` to `window` and returns once the window has handled it.
    fn send_command(&self, window: WindowId, id: usize);
}
