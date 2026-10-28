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

/// What the field shows at rest and the length of its leading title, in UTF-16 units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub text: Vec<u16>,
    pub title_len: u32,
}

/// The title of Rivet's start page.
pub const START_PAGE_TITLE: &str = "New tab";
/// The address Rivet windows show for Rivet's start page.
pub const START_PAGE_ADDRESS: &str = "rivet://start";
const SEPARATOR: &str = " \u{B7} ";

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn lower(unit: u16) -> u16 {
    if (0x41..=0x5A).contains(&unit) {
        unit + 32
    } else {
        unit
    }
}

/// `address` without an `http://` or `https://` scheme.
pub fn without_scheme(address: &[u16]) -> &[u16] {
    for scheme in ["https://", "http://"] {
        let scheme = units(scheme);
        if address.len() >= scheme.len()
            && address.iter().zip(&scheme).all(|(unit, expected)| lower(*unit) == *expected)
        {
            return &address[scheme.len()..];
        }
    }
    address
}

fn trimmed(text: &[u16]) -> &[u16] {
    let is_space = |unit: &u16| matches!(*unit, 0x20 | 0x09 | 0x0A | 0x0D);
    let start = text.iter().position(|unit| !is_space(unit)).unwrap_or(text.len());
    let end = text.iter().rposition(|unit| !is_space(unit)).map_or(start, |last| last + 1);
    &text[start..end]
}

fn bare(text: &[u16]) -> &[u16] {
    text.strip_suffix(&[b'/' as u16]).unwrap_or(text)
}

/// The text of a Rivet window's address field at rest: the page title, a middle dot and the
/// address without its scheme. A page without a title of its own, whose title Chrome fills in
/// with the address, shows the address alone.
pub fn compose(title: &[u16], full_address: &[u16], start_page: bool) -> Display {
    let (title, address) = if start_page {
        (units(START_PAGE_TITLE), units(START_PAGE_ADDRESS))
    } else {
        (trimmed(title).to_vec(), without_scheme(full_address).to_vec())
    };
    if title.is_empty()
        || bare(&title) == bare(&address)
        || bare(without_scheme(&title)) == bare(&address)
    {
        return Display { text: address, title_len: 0 };
    }
    let mut text = title.clone();
    text.extend(units(SEPARATOR));
    text.extend(address);
    Display { text, title_len: title.len() as u32 }
}

/// The capacity in UTF-16 units, null included, that Chrome's `std::u16string` allocates for a
/// text of `length` units longer than the ten that fit inside the string itself.
pub fn heap_capacity(length: usize) -> usize {
    (length & !7) + 8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(text: &str) -> Vec<u16> {
        units(text)
    }

    #[test]
    fn fr12_address_text_is_rivet_only_inside_the_rivet_scope() {
        assert!(applies(0, true));
        assert!(!applies(0, false));
        assert!(!applies(FULL_URL_FLAGS, true));
        assert!(!applies(FULL_URL_FLAGS, false));
    }

    #[test]
    fn fr12_address_shows_title_dot_and_address_without_scheme() {
        let shown = compose(&u("Example Domain"), &u("https://example.com/a/b"), false);
        assert_eq!(shown.text, u("Example Domain \u{B7} example.com/a/b"));
        assert_eq!(shown.title_len, 14);
        let plain = compose(&u("Home"), &u("HTTP://Example.com"), false);
        assert_eq!(plain.text, u("Home \u{B7} Example.com"));
    }

    #[test]
    fn fr12_start_page_shows_new_tab_and_rivet_start() {
        let shown = compose(&u("anything"), &u("file:///D:/rivet/start.html"), true);
        assert_eq!(shown.text, u("New tab \u{B7} rivet://start"));
        assert_eq!(shown.title_len, 7);
    }

    #[test]
    fn fr12_page_without_a_title_shows_the_address_alone() {
        let shown = compose(&u("example.com/a"), &u("https://example.com/a"), false);
        assert_eq!(shown, Display { text: u("example.com/a"), title_len: 0 });
        assert_eq!(compose(&u(""), &u("https://example.com"), false).title_len, 0);
        assert_eq!(compose(&u("  "), &u("https://example.com"), false).text, u("example.com"));
    }

    #[test]
    fn fr12_heap_capacity_leaves_room_for_the_null() {
        for length in 11..200 {
            assert!(heap_capacity(length) > length);
            assert_eq!(heap_capacity(length) % 8, 0);
        }
        assert_eq!(heap_capacity(11), 16);
    }
}
