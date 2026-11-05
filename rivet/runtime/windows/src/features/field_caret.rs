//! What a text field's timer needs drawn in the command bar and the settings card. Most of the
//! field's timers only blink its caret, and the caret is a small visual of its own.

/// Whether a field's timer changed only the caret: the field's generation, which counts every
/// change to its text and selection, stayed the same.
pub(crate) fn only_caret(before: u64, after: u64) -> bool {
    before == after
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_command_bar_caret_blink_redraws_only_the_caret() {
        assert!(only_caret(7, 7), "a blink leaves the text and selection as they were");
    }

    #[test]
    fn idle_settings_caret_timer_that_changed_the_field_redraws_the_card() {
        assert!(!only_caret(7, 8), "a timer that scrolled or selected text paints the card");
    }
}
