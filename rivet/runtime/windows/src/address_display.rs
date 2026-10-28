//! What the address field of a Rivet window shows while nobody edits it.
//!
//! Chrome asks its location bar model for two texts: the one it puts in the field at rest, and the
//! full address it switches to when a person starts editing (`FULL_URL_FLAGS`). Only the first is
//! replaced, and only for the window that is being laid out, painted or refreshed, so ordinary
//! windows of the same browser process keep Chrome's own text.

/// The flags `GetFormattedFullURL` passes: the text Chrome puts in the field for editing.
pub const FULL_URL_FLAGS: u32 = 7;

/// Whether the text for `flags` is Rivet's own: it is a resting text and the UI thread works for a
/// Rivet window.
pub fn applies(flags: u32, in_rivet_scope: bool) -> bool {
    in_rivet_scope && flags != FULL_URL_FLAGS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_address_text_is_rivet_only_inside_the_rivet_scope() {
        assert!(applies(0, true));
        assert!(!applies(0, false));
        assert!(!applies(FULL_URL_FLAGS, true));
        assert!(!applies(FULL_URL_FLAGS, false));
    }
}
