//! The spaces of one browser window and the tabs each of them keeps.
//!
//! A window shows the tabs of one space. Leaving a space stores its tabs here, and coming back
//! restores them. The store lives in memory while the window does, and a copy in a Rivet-owned file
//! outlives the browser, so a restart or an update of the browser brings the spaces back.
//!
//! The file holds names, colours, addresses and titles only. Passwords, cookies and anything else
//! the browser keeps for a profile stay in the browser's own profile folder.

/// A tab as a space remembers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedTab {
    pub url: String,
    /// Kept with the address so a space can list its tabs without loading them.
    #[allow(dead_code)]
    pub title: String,
    pub active: bool,
    /// The tab group the tab was in. Tabs with the same `key` were one group.
    pub group: Option<SavedGroup>,
}

/// A tab group as a space remembers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedGroup {
    /// Names the group among the tabs of one space, as hex of the browser's id.
    pub key: String,
    pub title: String,
    /// The browser's colour number for the group.
    pub color: i32,
    pub collapsed: bool,
}

/// A Chromium browser a space can be based on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserKind {
    Chrome,
    Edge,
    Whale,
}

impl BrowserKind {
    const ALL: [Self; 3] = [Self::Chrome, Self::Edge, Self::Whale];

    /// The name the file keeps.
    pub fn key(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Whale => "whale",
        }
    }

    /// The name the appbar shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Whale => "Whale",
        }
    }

    /// The App Paths name the browser's installer registers.
    pub fn executable(self) -> &'static str {
        match self {
            Self::Chrome => "chrome.exe",
            Self::Edge => "msedge.exe",
            Self::Whale => "whale.exe",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// The browser a program path is, or `None` for one a space cannot be based on.
    pub fn from_program(path: &str) -> Option<Self> {
        let name = path.rsplit(['\\', '/']).next()?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|kind| kind.executable() == name)
    }
}

/// What a space is called, how it is coloured and which browser and profile it opens in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceInfo {
    pub name: String,
    pub tint: u32,
    pub browser: BrowserKind,
    /// The profile's name, such as `Work`.
    pub profile: String,
}

impl SpaceInfo {
    /// The badge text, such as `Edge · Lab`.
    pub fn badge(&self) -> String {
        format!("{} \u{b7} {}", self.browser.label(), self.profile)
    }
}

/// A switch to a space that lives in another browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandOver {
    pub browser: BrowserKind,
    pub user_data_dir: std::path::PathBuf,
}

/// Whether showing `target` needs another browser than `current`, the browser of this window.
/// A window whose browser is not one a space can be based on (`None`) never hands over. Spaces of
/// the same browser switch inside the window.
pub fn hand_over(
    target: &SpaceInfo,
    current: Option<BrowserKind>,
    data_root: &std::path::Path,
) -> Option<HandOver> {
    let current = current?;
    (target.browser != current).then(|| HandOver {
        browser: target.browser,
        user_data_dir: profile_dir(data_root, target.browser, &target.profile),
    })
}

/// The profile folder Rivet keeps for a browser's profile: `<root>\Rivet\profiles\<browser>\<name>`.
pub fn profile_dir(root: &std::path::Path, browser: BrowserKind, profile: &str) -> std::path::PathBuf {
    let name: String = profile
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let name = name.trim();
    let name = if name.is_empty() { "Default" } else { name };
    root.join("Rivet").join("profiles").join(browser.key()).join(name)
}

/// The arguments of `rivet session start` for a hand-over.
pub fn start_arguments(
    hand_over: &HandOver,
    browser: &std::path::Path,
    runtime: Option<&std::path::Path>,
    url: &str,
) -> Vec<String> {
    let mut arguments: Vec<String> = ["session", "start", "--browser"].map(String::from).into();
    arguments.push(browser.to_string_lossy().into_owned());
    arguments.push("--user-data-dir".into());
    arguments.push(hand_over.user_data_dir.to_string_lossy().into_owned());
    if let Some(runtime) = runtime {
        arguments.push("--runtime".into());
        arguments.push(runtime.to_string_lossy().into_owned());
    }
    arguments.push("--url".into());
    arguments.push(url.into());
    arguments
}

/// Whether the launcher got the other browser's window up. A browser the runtime cannot attach to
/// yet still opens, without the Rivet frame, and that counts: the old window may close.
pub fn handover_started(success: bool, stderr: &str) -> bool {
    success || stderr.contains("could not load the Rivet runtime")
}

/// The most spaces the footer has room for.
pub const MAX_SPACES: usize = 6;

const TINTS: [u32; MAX_SPACES] = [0xd08a52, 0x5b8bab, 0xc0577f, 0x6a9f58, 0x8a6fc4, 0xc9a227];

#[derive(Clone, Debug)]
pub struct Spaces {
    list: Vec<SpaceInfo>,
    current: usize,
    /// The tabs of every space but the current one, in the same order as `list`.
    saved: Vec<Vec<SavedTab>>,
}

impl Spaces {
    pub fn new() -> Self {
        let space = |name: &str, tint: u32, browser: BrowserKind, profile: &str| SpaceInfo {
            name: name.into(),
            tint,
            browser,
            profile: profile.into(),
        };
        Self {
            list: vec![
                space("Work", TINTS[0], BrowserKind::Chrome, "Work"),
                space("Research", TINTS[1], BrowserKind::Edge, "Lab"),
                space("Personal", TINTS[2], BrowserKind::Whale, "Personal"),
            ],
            current: 0,
            saved: vec![Vec::new(); 3],
        }
    }

    pub fn list(&self) -> &[SpaceInfo] {
        &self.list
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn can_add(&self) -> bool {
        self.list.len() < MAX_SPACES
    }

    /// Adds an empty space after the others, and returns its index.
    pub fn add(&mut self) -> Option<usize> {
        if !self.can_add() {
            return None;
        }
        let index = self.list.len();
        self.list.push(SpaceInfo {
            name: format!("Space {}", index + 1),
            tint: TINTS[index],
            browser: BrowserKind::Chrome,
            profile: "Default".into(),
        });
        self.saved.push(Vec::new());
        Some(index)
    }

    /// Makes `index` the current space. `open` are the tabs the window shows now, which this
    /// space keeps. Returns the tabs to show instead: the ones the target kept, or one start page
    /// at `start_url` for a space that has none. Returns `None` when there is nothing to switch.
    pub fn switch_to(
        &mut self,
        index: usize,
        open: Vec<SavedTab>,
        start_url: &str,
    ) -> Option<Vec<SavedTab>> {
        if index >= self.list.len() || index == self.current {
            return None;
        }
        self.saved[self.current] = open;
        self.current = index;
        let mut tabs = std::mem::take(&mut self.saved[index]);
        if tabs.is_empty() {
            tabs.push(SavedTab {
                url: start_url.into(),
                title: String::new(),
                active: true,
                group: None,
            });
        }
        if !tabs.iter().any(|tab| tab.active) {
            tabs[0].active = true;
        }
        Some(tabs)
    }
}

/// Whether a tab shows nothing of the user's own: no address yet, a new tab page or a Rivet start
/// page. A window that shows only such tabs may be given the tabs of the saved spaces.
pub fn is_blank_tab(url: &str) -> bool {
    url.is_empty()
        || url == "about:blank"
        || url.starts_with("chrome://newtab")
        || url.starts_with("chrome-search://")
        || (url.starts_with("file:") && url.contains("/start.html?space="))
}

/// The tabs of one space kept in the file, at most.
const MAX_STORED_TABS: usize = 500;
/// The longest address or title the file keeps.
const MAX_TEXT: usize = 4096;
/// The file format version. A file of another version is ignored rather than guessed at.
const FORMAT: u64 = 1;

/// What the file holds besides the spaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub spaces: Spaces,
    /// The appbar width in device-independent pixels, as it was saved.
    pub width: i32,
    pub hidden: bool,
}

impl Spaces {
    /// The tabs of the current space as the file held them. The window shows them once, and the
    /// current space keeps no copy afterwards because its tabs are the window's own.
    pub fn take_current_tabs(&mut self) -> Vec<SavedTab> {
        std::mem::take(&mut self.saved[self.current])
    }

    /// The file text for these spaces. `open` are the tabs the window shows now, which belong to
    /// the current space.
    pub fn to_json(&self, open: &[SavedTab], width: i32, hidden: bool, user_data_dir: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("{{\n  \"version\": {FORMAT},\n  \"user_data_dir\": "));
        quote(&mut out, user_data_dir);
        out.push_str(&format!(
            ",\n  \"current\": {},\n  \"appbar\": {{\"width\": {width}, \"hidden\": {hidden}}},\n  \"spaces\": [",
            self.current
        ));
        for (index, info) in self.list.iter().enumerate() {
            out.push_str(if index == 0 { "\n    {" } else { ",\n    {" });
            out.push_str("\"name\": ");
            quote(&mut out, &info.name);
            out.push_str(&format!(
                ", \"tint\": {}, \"browser\": \"{}\", \"profile\": ",
                info.tint,
                info.browser.key()
            ));
            quote(&mut out, &info.profile);
            out.push_str(", \"tabs\": [");
            let tabs: &[SavedTab] = if index == self.current { open } else { &self.saved[index] };
            for (n, tab) in tabs.iter().take(MAX_STORED_TABS).enumerate() {
                out.push_str(if n == 0 { "\n      {" } else { ",\n      {" });
                out.push_str("\"url\": ");
                quote(&mut out, &tab.url);
                out.push_str(", \"title\": ");
                quote(&mut out, &tab.title);
                out.push_str(&format!(", \"active\": {}", tab.active));
                if let Some(group) = &tab.group {
                    out.push_str(", \"group\": {\"id\": ");
                    quote(&mut out, &group.key);
                    out.push_str(", \"title\": ");
                    quote(&mut out, &group.title);
                    out.push_str(&format!(
                        ", \"color\": {}, \"collapsed\": {}}}",
                        group.color, group.collapsed
                    ));
                }
                out.push('}');
            }
            out.push_str(if tabs.is_empty() { "]}" } else { "\n    ]}" });
        }
        out.push_str("\n  ]\n}\n");
        out
    }

    /// Reads the file text. Anything that is not a whole, sane file gives `None`, which makes the
    /// window start with the default spaces.
    pub fn from_json(text: &str) -> Option<Stored> {
        let root = json::parse(text)?;
        if root.get("version")?.as_u64()? != FORMAT {
            return None;
        }
        let items = root.get("spaces")?.as_array()?;
        if items.is_empty() || items.len() > MAX_SPACES {
            return None;
        }
        let current = root.get("current")?.as_u64()? as usize;
        if current >= items.len() {
            return None;
        }
        let mut list = Vec::new();
        let mut saved = Vec::new();
        for item in items {
            let name = item.get("name")?.as_str()?;
            let profile = item.get("profile")?.as_str()?;
            let tint = item.get("tint")?.as_u64()?;
            // Files written before spaces had a browser hold the badge text, such as
            // "Chrome \u{b7} Work", as the profile.
            let (browser, profile) = match item.get("browser") {
                None | Some(json::Json::Null) => (
                    BrowserKind::Chrome,
                    profile.strip_prefix("Chrome \u{b7} ").unwrap_or(profile),
                ),
                Some(browser) => (
                    BrowserKind::from_key(browser.as_str()?).unwrap_or(BrowserKind::Chrome),
                    profile,
                ),
            };
            if name.trim().is_empty() || name.len() > MAX_TEXT || profile.len() > MAX_TEXT || tint > 0xff_ffff {
                return None;
            }
            let mut tabs = Vec::new();
            for tab in item.get("tabs")?.as_array()?.iter().take(MAX_STORED_TABS) {
                let url = tab.get("url")?.as_str()?;
                let title = tab.get("title")?.as_str()?;
                if url.len() > MAX_TEXT || title.len() > MAX_TEXT {
                    return None;
                }
                // Files written before groups were kept have no `group`, which is no group.
                let group = match tab.get("group") {
                    None | Some(json::Json::Null) => None,
                    Some(group) => {
                        let (key, name) = (group.get("id")?.as_str()?, group.get("title")?.as_str()?);
                        if key.len() > MAX_TEXT || name.len() > MAX_TEXT {
                            return None;
                        }
                        Some(SavedGroup {
                            key: key.into(),
                            title: name.into(),
                            color: group.get("color")?.as_u64()?.min(i32::MAX as u64) as i32,
                            collapsed: group.get("collapsed")?.as_bool()?,
                        })
                    }
                };
                tabs.push(SavedTab {
                    url: url.into(),
                    title: title.into(),
                    active: tab.get("active")?.as_bool()?,
                    group,
                });
            }
            list.push(SpaceInfo { name: name.into(), tint: tint as u32, browser, profile: profile.into() });
            saved.push(tabs);
        }
        let appbar = root.get("appbar")?;
        let width = appbar.get("width")?.as_u64()?.min(i32::MAX as u64) as i32;
        let hidden = appbar.get("hidden")?.as_bool()?;
        Some(Stored { spaces: Spaces { list, current, saved }, width, hidden })
    }
}

impl PartialEq for Spaces {
    fn eq(&self, other: &Self) -> bool {
        self.list == other.list && self.current == other.current && self.saved == other.saved
    }
}
impl Eq for Spaces {}

fn quote(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Where the file of a browser profile lives: `%LOCALAPPDATA%\Rivet\spaces\<key>.json`, with the
/// key made from the profile's `--user-data-dir`, or `default` when the browser has none.
pub fn store_path(root: &std::path::Path, user_data_dir: Option<&str>) -> std::path::PathBuf {
    let key = match user_data_dir {
        Some(dir) => {
            let normal = dir.trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase();
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in normal.bytes() {
                hash = (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3);
            }
            format!("{hash:016x}")
        }
        None => "default".into(),
    };
    root.join("Rivet").join("spaces").join(format!("{key}.json"))
}

/// The `--user-data-dir` among a process's arguments.
pub fn user_data_dir_of(args: impl IntoIterator<Item = String>) -> Option<String> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--user-data-dir=") {
            return Some(value.trim_matches('"').to_owned());
        }
        if arg == "--user-data-dir" {
            return args.next();
        }
    }
    None
}

/// Writes the file whole or not at all: the text goes to a neighbour file first, which then
/// replaces the old one in one step, so a crash leaves the old file or the new one.
pub fn write_atomic(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)
}

/// A small JSON reader, enough for the file above.
mod json {
    #[derive(Debug)]
    pub enum Json {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<Json>),
        Obj(Vec<(String, Json)>),
    }

    impl Json {
        pub fn get(&self, key: &str) -> Option<&Json> {
            match self {
                Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Json::Str(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_bool(&self) -> Option<bool> {
            match self {
                Json::Bool(b) => Some(*b),
                _ => None,
            }
        }
        pub fn as_u64(&self) -> Option<u64> {
            match self {
                Json::Num(n) if *n >= 0.0 && n.fract() == 0.0 && *n < 1e15 => Some(*n as u64),
                _ => None,
            }
        }
        pub fn as_array(&self) -> Option<&[Json]> {
            match self {
                Json::Arr(items) => Some(items),
                _ => None,
            }
        }
    }

    pub fn parse(text: &str) -> Option<Json> {
        let mut reader = Reader { bytes: text.as_bytes(), at: 0 };
        let value = reader.value(0)?;
        reader.space();
        (reader.at == reader.bytes.len()).then_some(value)
    }

    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl Reader<'_> {
        fn space(&mut self) {
            while matches!(self.bytes.get(self.at), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.at += 1;
            }
        }
        fn eat(&mut self, byte: u8) -> Option<()> {
            self.space();
            (self.bytes.get(self.at) == Some(&byte)).then(|| self.at += 1)
        }
        fn word(&mut self, word: &str, value: Json) -> Option<Json> {
            let end = self.at + word.len();
            (self.bytes.get(self.at..end)? == word.as_bytes()).then(|| {
                self.at = end;
                value
            })
        }
        fn value(&mut self, depth: usize) -> Option<Json> {
            if depth > 8 {
                return None;
            }
            self.space();
            match *self.bytes.get(self.at)? {
                b'{' => {
                    self.at += 1;
                    let mut fields = Vec::new();
                    self.space();
                    if self.bytes.get(self.at) == Some(&b'}') {
                        self.at += 1;
                        return Some(Json::Obj(fields));
                    }
                    loop {
                        self.space();
                        let key = self.string()?;
                        self.eat(b':')?;
                        fields.push((key, self.value(depth + 1)?));
                        self.space();
                        match self.bytes.get(self.at)? {
                            b',' => self.at += 1,
                            b'}' => {
                                self.at += 1;
                                return Some(Json::Obj(fields));
                            }
                            _ => return None,
                        }
                    }
                }
                b'[' => {
                    self.at += 1;
                    let mut items = Vec::new();
                    self.space();
                    if self.bytes.get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Some(Json::Arr(items));
                    }
                    loop {
                        items.push(self.value(depth + 1)?);
                        self.space();
                        match self.bytes.get(self.at)? {
                            b',' => self.at += 1,
                            b']' => {
                                self.at += 1;
                                return Some(Json::Arr(items));
                            }
                            _ => return None,
                        }
                    }
                }
                b'"' => self.string().map(Json::Str),
                b't' => self.word("true", Json::Bool(true)),
                b'f' => self.word("false", Json::Bool(false)),
                b'n' => self.word("null", Json::Null),
                _ => {
                    let start = self.at;
                    while matches!(self.bytes.get(self.at), Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')) {
                        self.at += 1;
                    }
                    std::str::from_utf8(&self.bytes[start..self.at]).ok()?.parse().ok().map(Json::Num)
                }
            }
        }
        fn hex4(&mut self) -> Option<u32> {
            let digits = self.bytes.get(self.at..self.at + 4)?;
            self.at += 4;
            u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
        }
        fn string(&mut self) -> Option<String> {
            if self.bytes.get(self.at) != Some(&b'"') {
                return None;
            }
            self.at += 1;
            let mut out: Vec<u8> = Vec::new();
            loop {
                let byte = *self.bytes.get(self.at)?;
                self.at += 1;
                match byte {
                    b'"' => return String::from_utf8(out).ok(),
                    b'\\' => {
                        let escape = *self.bytes.get(self.at)?;
                        self.at += 1;
                        let c = match escape {
                            b'"' => '"',
                            b'\\' => '\\',
                            b'/' => '/',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'n' => '\n',
                            b'r' => '\r',
                            b't' => '\t',
                            b'u' => {
                                let mut unit = self.hex4()?;
                                if (0xd800..0xdc00).contains(&unit) {
                                    if self.bytes.get(self.at..self.at + 2)? != b"\\u" {
                                        return None;
                                    }
                                    self.at += 2;
                                    let low = self.hex4()?;
                                    if !(0xdc00..0xe000).contains(&low) {
                                        return None;
                                    }
                                    unit = 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
                                }
                                char::from_u32(unit)?
                            }
                            _ => return None,
                        };
                        let mut buffer = [0u8; 4];
                        out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
                    }
                    0..=0x1f => return None,
                    other => out.push(other),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(url: &str, active: bool) -> SavedTab {
        SavedTab {
            url: url.into(),
            title: format!("Title of {url}"),
            active,
            group: None,
        }
    }

    #[test]
    fn fr3_new_window_starts_in_work_with_three_spaces() {
        let spaces = Spaces::new();
        assert_eq!(spaces.current(), 0);
        let names: Vec<_> = spaces.list().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Work", "Research", "Personal"]);
    }

    #[test]
    fn fr3_switching_stores_the_open_tabs_and_shows_a_start_page_for_an_empty_space() {
        let mut spaces = Spaces::new();
        let open = vec![tab("https://a.test/", true), tab("https://b.test/", false)];
        let shown = spaces.switch_to(1, open, "file:///start.html?space=Personal");
        assert_eq!(spaces.current(), 1);
        assert_eq!(
            shown,
            Some(vec![SavedTab {
                url: "file:///start.html?space=Personal".into(),
                title: String::new(),
                active: true,
                group: None,
            }])
        );
    }

    #[test]
    fn fr3_coming_back_restores_urls_titles_order_and_the_active_tab() {
        let mut spaces = Spaces::new();
        let work = vec![
            tab("https://a.test/", false),
            tab("https://b.test/", true),
            tab("https://c.test/", false),
        ];
        spaces.switch_to(2, work.clone(), "start");
        let back = spaces.switch_to(0, vec![tab("start", true)], "start");
        assert_eq!(back, Some(work));
        // The space left in between kept its start page.
        let again = spaces.switch_to(2, vec![tab("https://a.test/", true)], "start");
        assert_eq!(again, Some(vec![tab("start", true)]));
    }

    #[test]
    fn fr3_switching_to_the_current_or_a_missing_space_does_nothing() {
        let mut spaces = Spaces::new();
        assert_eq!(spaces.switch_to(0, vec![tab("x", true)], "start"), None);
        assert_eq!(spaces.switch_to(9, vec![tab("x", true)], "start"), None);
        assert_eq!(spaces.current(), 0);
    }

    #[test]
    fn fr3_a_restored_space_always_has_an_active_tab() {
        let mut spaces = Spaces::new();
        spaces.switch_to(1, vec![tab("a", false), tab("b", false)], "start");
        let shown = spaces
            .switch_to(0, vec![tab("start", true)], "start")
            .unwrap();
        assert_eq!(shown.iter().filter(|t| t.active).count(), 1);
        assert!(shown[0].active);
    }

    #[test]
    fn fr3_new_spaces_stop_at_the_footer_limit_and_start_empty() {
        let mut spaces = Spaces::new();
        assert_eq!(spaces.add(), Some(3));
        assert_eq!(spaces.list()[3].name, "Space 4");
        assert_eq!(spaces.add(), Some(4));
        assert_eq!(spaces.add(), Some(5));
        assert!(!spaces.can_add());
        assert_eq!(spaces.add(), None);
        let tints: std::collections::HashSet<_> = spaces.list().iter().map(|s| s.tint).collect();
        assert_eq!(tints.len(), MAX_SPACES);
        let shown = spaces.switch_to(3, vec![tab("a", true)], "start").unwrap();
        assert_eq!(shown[0].url, "start");
    }

    fn sample() -> Spaces {
        let mut spaces = Spaces::new();
        spaces.list[1].name = "Caf\u{e9} \"two\"\\".into();
        spaces.switch_to(
            1,
            vec![tab("https://a.test/?q=1&r=\u{d55c}", false), tab("https://b.test/", true)],
            "start",
        );
        spaces
    }

    #[test]
    fn fr3_file_keeps_names_tints_tabs_and_the_current_space_across_a_restart() {
        let spaces = sample();
        let open = vec![tab("https://c.test/", true), tab("https://d.test/\ttab", false)];
        let text = spaces.to_json(&open, 300, true, "D:\\Profiles\\Work");
        let stored = Spaces::from_json(&text).expect("a file this module wrote must load");
        assert_eq!((stored.width, stored.hidden), (300, true));
        assert_eq!(stored.spaces.current(), 1);
        assert_eq!(stored.spaces.list(), spaces.list());
        let mut loaded = stored.spaces;
        assert_eq!(loaded.take_current_tabs(), open);
        // The space left earlier keeps its tabs, order and active tab.
        let back = loaded.switch_to(0, vec![tab("x", true)], "start");
        assert_eq!(back, Some(spaces.saved[0].clone()));
        assert_eq!(back.unwrap().len(), 2);
    }

    #[test]
    fn fr3_file_holds_no_more_than_names_tints_profiles_urls_and_titles() {
        let text = sample().to_json(&[], 268, false, "D:\\p");
        for key in [
            "version", "user_data_dir", "current", "appbar", "spaces", "name", "tint", "browser",
            "profile",
            "tabs",
        ] {
            assert!(text.contains(&format!("\"{key}\"")), "{key}");
        }
        for banned in ["password", "cookie", "passkey"] {
            assert!(!text.to_lowercase().contains(banned));
        }
    }

    #[test]
    fn fr3_corrupt_or_foreign_files_fall_back_to_defaults() {
        let good = Spaces::new().to_json(&[], 268, false, "d");
        assert!(Spaces::from_json(&good).is_some());
        let cut = &good[..good.len() / 2];
        let empty = "{\"version\":1,\"current\":0,\"appbar\":{\"width\":1,\"hidden\":false},\"spaces\":[]}";
        for bad in [
            "",
            "not json",
            "{}",
            "[]",
            cut,
            &good.replace("\"version\": 1", "\"version\": 2"),
            &good.replace("\"current\": 0", "\"current\": 7"),
            &good.replace("\"tint\": 13666898", "\"tint\": -1"),
            &good.replace("\"hidden\": false", "\"hidden\": \"no\""),
            &format!("{good} trailing"),
            empty,
        ] {
            assert!(Spaces::from_json(bad).is_none(), "{bad:?}");
        }
        assert!(Spaces::from_json(&"[".repeat(5000)).is_none());
    }

    #[test]
    fn fr3_tab_groups_survive_the_file_and_older_files_have_none() {
        let group = |key: &str, collapsed| {
            Some(SavedGroup { key: key.into(), title: "Read \"later\"".into(), color: 3, collapsed })
        };
        let mut a = tab("https://a.test/", true);
        a.group = group("0a1b", false);
        let mut b = tab("https://b.test/", false);
        b.group = group("0a1b", false);
        let mut c = tab("https://c.test/", false);
        c.group = group("ff", true);
        let open = vec![a, b, c, tab("https://d.test/", false)];
        let text = Spaces::new().to_json(&open, 268, false, "d");
        let mut loaded = Spaces::from_json(&text).unwrap().spaces;
        assert_eq!(loaded.take_current_tabs(), open);
        // A tab line without a group, as the first file version wrote it, still loads.
        let old = text.replace(", \"group\"", ", \"x\"");
        assert!(Spaces::from_json(&old).is_some());
        let broken = text.replace("\"collapsed\": true", "\"collapsed\": 1");
        assert!(Spaces::from_json(&broken).is_none());
    }

    #[test]
    fn fr3_default_spaces_match_the_mockup_in_names_tints_browsers_and_badges() {
        let spaces = Spaces::new();
        let rows: Vec<_> = spaces
            .list()
            .iter()
            .map(|s| (s.name.clone(), s.tint, s.badge()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Work".to_owned(), 0xd08a52, "Chrome \u{b7} Work".to_owned()),
                ("Research".to_owned(), 0x5b8bab, "Edge \u{b7} Lab".to_owned()),
                ("Personal".to_owned(), 0xc0577f, "Whale \u{b7} Personal".to_owned()),
            ]
        );
    }

    #[test]
    fn fr3_browsers_round_trip_through_the_file_and_older_files_default_to_chrome() {
        let spaces = Spaces::new();
        let text = spaces.to_json(&[], 268, false, "d");
        assert!(text.contains("\"browser\": \"edge\""));
        assert_eq!(Spaces::from_json(&text).unwrap().spaces.list(), spaces.list());
        // A file from before spaces had a browser: no field, and the badge text as the profile.
        let old = "{\"version\":1,\"current\":0,\"appbar\":{\"width\":268,\"hidden\":false},\"spaces\":[{\"name\":\"Work\",\"tint\":1,\"profile\":\"Chrome \u{b7} Work\",\"tabs\":[]},{\"name\":\"B\",\"tint\":2,\"profile\":\"Mine\",\"tabs\":[]}]}";
        let loaded = Spaces::from_json(old).unwrap().spaces;
        assert_eq!(loaded.list()[0].browser, BrowserKind::Chrome);
        assert_eq!(loaded.list()[0].badge(), "Chrome \u{b7} Work");
        assert_eq!(loaded.list()[1].badge(), "Chrome \u{b7} Mine");
        // A browser this version does not know is read as Chrome rather than losing the file.
        let future = text.replace("\"edge\"", "\"brave\"");
        assert_eq!(Spaces::from_json(&future).unwrap().spaces.list()[1].browser, BrowserKind::Chrome);
        assert!(Spaces::from_json(&text.replace("\"edge\"", "7")).is_none());
    }

    #[test]
    fn fr3_programs_map_to_their_browser() {
        let kind = |path| BrowserKind::from_program(path);
        assert_eq!(kind("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"), Some(BrowserKind::Chrome));
        assert_eq!(kind("C:/Edge/MSEdge.exe"), Some(BrowserKind::Edge));
        assert_eq!(kind("whale.exe"), Some(BrowserKind::Whale));
        assert_eq!(kind("C:\\Brave\\brave.exe"), None);
    }

    #[test]
    fn fr3_a_space_of_another_browser_hands_over_with_its_own_profile_folder() {
        let spaces = Spaces::new();
        let root = std::path::Path::new("D:\\Data");
        let current = Some(BrowserKind::Chrome);
        assert_eq!(hand_over(&spaces.list()[0], current, root), None);
        let edge = hand_over(&spaces.list()[1], current, root).unwrap();
        assert_eq!(edge.browser, BrowserKind::Edge);
        assert_eq!(edge.user_data_dir, std::path::Path::new("D:\\Data\\Rivet\\profiles\\edge\\Lab"));
        let whale = hand_over(&spaces.list()[2], current, root).unwrap();
        assert_eq!(whale.user_data_dir, std::path::Path::new("D:\\Data\\Rivet\\profiles\\whale\\Personal"));
        // From an Edge window the Research space stays, and the Chrome one hands back.
        assert_eq!(hand_over(&spaces.list()[1], Some(BrowserKind::Edge), root), None);
        assert!(hand_over(&spaces.list()[0], Some(BrowserKind::Edge), root).is_some());
        // A window of a browser no space is based on never hands over.
        assert_eq!(hand_over(&spaces.list()[1], None, root), None);
    }

    #[test]
    fn fr3_profile_names_cannot_leave_the_profile_folder() {
        let root = std::path::Path::new("D:\\Data");
        let dir = profile_dir(root, BrowserKind::Edge, "..\\..\\evil:*");
        assert!(dir.starts_with("D:\\Data\\Rivet\\profiles\\edge"));
        assert_eq!(dir.components().count(), std::path::Path::new("D:\\Data\\Rivet\\profiles\\edge\\x").components().count());
        assert!(profile_dir(root, BrowserKind::Edge, "  ").ends_with("Default"));
    }

    #[test]
    fn fr3_hand_over_starts_the_session_through_the_launcher_with_profile_and_runtime() {
        let hand = HandOver { browser: BrowserKind::Edge, user_data_dir: "D:\\P\\edge\\Lab".into() };
        let args = start_arguments(
            &hand,
            std::path::Path::new("C:\\Edge\\msedge.exe"),
            Some(std::path::Path::new("D:\\R\\rivet_runtime_windows.dll")),
            "file:///start.html?space=Research",
        );
        assert_eq!(
            args,
            [
                "session", "start", "--browser", "C:\\Edge\\msedge.exe", "--user-data-dir",
                "D:\\P\\edge\\Lab", "--runtime", "D:\\R\\rivet_runtime_windows.dll", "--url",
                "file:///start.html?space=Research"
            ]
        );
        assert!(!start_arguments(&hand, std::path::Path::new("e"), None, "u").contains(&"--runtime".into()));
    }

    #[test]
    fn fr3_the_old_window_closes_only_once_the_other_browser_is_up() {
        assert!(handover_started(true, ""));
        // The browser opened but the runtime could not attach yet: it still counts.
        assert!(handover_started(false, "could not load the Rivet runtime into browser task 7: x"));
        assert!(!handover_started(false, "could not start C:\\missing.exe: not found"));
    }

    #[test]
    fn fr3_only_empty_new_tab_and_start_pages_count_as_blank() {
        assert!(is_blank_tab(""));
        assert!(is_blank_tab("chrome://newtab/"));
        assert!(is_blank_tab("file:///C:/Rivet/start.html?space=Work"));
        assert!(!is_blank_tab("https://a.test/"));
        assert!(!is_blank_tab("file:///C:/notes.html"));
    }

    #[test]
    fn fr3_escapes_and_surrogate_pairs_round_trip() {
        let text = "{\"version\":1,\"current\":0,\"appbar\":{\"width\":268,\"hidden\":false},\"spaces\":[{\"name\":\"a\\ud83d\\ude00\\u00e9\",\"tint\":1,\"profile\":\"p\",\"tabs\":[]}]}";
        let stored = Spaces::from_json(text).unwrap();
        assert_eq!(stored.spaces.list()[0].name, "a\u{1f600}\u{e9}");
        assert!(Spaces::from_json(&text.replace("\\ude00", "\\u0041")).is_none());
    }

    #[test]
    fn fr3_store_is_keyed_by_the_profile_directory() {
        let root = std::path::Path::new("C:\\Local");
        let a = store_path(root, Some("D:\\Profiles\\Work\\"));
        assert_eq!(a, store_path(root, Some("d:/profiles/work")));
        assert_ne!(a, store_path(root, Some("D:\\Profiles\\Home")));
        assert!(a.starts_with("C:\\Local\\Rivet\\spaces"));
        assert!(store_path(root, None).ends_with("default.json"));
        let args = |list: &[&str]| user_data_dir_of(list.iter().map(|s| s.to_string()));
        assert_eq!(args(&["c.exe", "--user-data-dir=D:\\x"]), Some("D:\\x".into()));
        assert_eq!(args(&["c.exe", "--user-data-dir", "D:\\y"]), Some("D:\\y".into()));
        assert_eq!(args(&["c.exe"]), None);
    }

    #[test]
    fn fr3_atomic_write_replaces_the_file_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("rivet-spaces-test-{}", std::process::id()));
        let path = dir.join("nested").join("p.json");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["p.json"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
