//! What a control request does to a session's window, independent of how the window is reached.
//!
//! The Windows runtime implements [`Browser`] with the browser's own functions, and tests implement
//! it in memory. [`handle`] applies the display policy and runs one command, so the CLI, the
//! runtime and the tests share one meaning for every command.

use rivet_protocol::{
    control_request::Command, control_response, generated, ControlFailure, ControlRequest,
    ControlResponse, DisplayPolicy, FailureKind, Frame, NavStep, Page, ReadField, Visibility,
    Workspace,
};

/// A failed command: what kind of failure, and a sentence a person can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub reason: String,
}

impl Failure {
    pub fn new(kind: FailureKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            reason: reason.into(),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::new(FailureKind::Unavailable, reason)
    }

    pub fn not_found(reason: impl Into<String>) -> Self {
        Self::new(FailureKind::NotFound, reason)
    }

    pub fn refused(reason: impl Into<String>) -> Self {
        Self::new(FailureKind::BrowserRefused, reason)
    }

    fn bad(reason: impl Into<String>) -> Self {
        Self::new(FailureKind::BadRequest, reason)
    }
}

/// A key a person or agent names on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// The Windows virtual-key code.
    pub virtual_key: u16,
    /// The character the key types, for keys that type one.
    pub character: Option<char>,
}

const KEYS: [(&str, u16, Option<char>); 15] = [
    ("enter", 0x0d, Some('\r')),
    ("tab", 0x09, Some('\t')),
    ("escape", 0x1b, None),
    ("backspace", 0x08, Some('\u{8}')),
    ("delete", 0x2e, None),
    ("space", 0x20, Some(' ')),
    ("left", 0x25, None),
    ("up", 0x26, None),
    ("right", 0x27, None),
    ("down", 0x28, None),
    ("home", 0x24, None),
    ("end", 0x23, None),
    ("pageup", 0x21, None),
    ("pagedown", 0x22, None),
    ("f5", 0x74, None),
];

impl Key {
    /// The key of a name such as `enter` or `pagedown`, ignoring case.
    pub fn parse(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        KEYS.iter()
            .find(|(known, _, _)| *known == lower)
            .map(|&(_, virtual_key, character)| Key {
                virtual_key,
                character,
            })
    }

    /// Every name [`Key::parse`] takes.
    pub fn names() -> impl Iterator<Item = &'static str> {
        KEYS.iter().map(|(name, _, _)| *name)
    }
}

/// One session's window as the control channel sees it. Every method runs the browser's own
/// function for the action and never draws or types anything of Rivet's in its place.
pub trait Browser {
    fn pages(&mut self) -> Result<Vec<Page>, Failure>;
    fn open_page(&mut self, url: &str) -> Result<(), Failure>;
    fn close_page(&mut self, index: u32) -> Result<(), Failure>;
    fn select_page(&mut self, index: u32) -> Result<(), Failure>;
    fn navigate(&mut self, page: Option<u32>, url: &str) -> Result<(), Failure>;
    fn step(&mut self, page: Option<u32>, step: NavStep) -> Result<(), Failure>;
    fn type_text(&mut self, page: Option<u32>, text: &str) -> Result<(), Failure>;
    fn click(&mut self, page: Option<u32>, x: i32, y: i32) -> Result<(), Failure>;
    fn key(&mut self, page: Option<u32>, key: Key) -> Result<(), Failure>;
    fn read(&mut self, page: Option<u32>, field: ReadField) -> Result<String, Failure>;
    fn frame(&mut self) -> Result<Frame, Failure>;
    fn workspaces(&mut self) -> Result<Vec<Workspace>, Failure>;
    fn switch_workspace(&mut self, name: &str) -> Result<(), Failure>;
    fn visibility(&self) -> Visibility;
    fn set_visibility(&mut self, visibility: Visibility) -> Result<(), Failure>;
}

/// What the display policy allows for one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Run the command as the window is.
    Run,
    /// Show the hidden window, then run the command.
    Show,
    /// Do not run the command, for this reason.
    Refuse(String),
}

/// Applies `policy` to a command on a window that is `now` shown or hidden. A hidden window keeps
/// navigating, running scripts and reading pages; only a command that needs visual evidence, a
/// capture of what a person would see, needs it shown. `visible` always shows the window, `auto`
/// shows it only for such a command, and `hidden` never shows it and says why it stopped.
pub fn plan(policy: DisplayPolicy, now: Visibility, needs_visual: bool, command: &str) -> Plan {
    if now != Visibility::Hidden {
        return Plan::Run;
    }
    match policy {
        DisplayPolicy::Visible => Plan::Show,
        DisplayPolicy::Hidden if needs_visual => Plan::Refuse(format!(
            "`{command}` needs visual evidence and the session is hidden; run it with --display auto or --display visible to show the window"
        )),
        _ if needs_visual => Plan::Show,
        _ => Plan::Run,
    }
}

fn needs_visual(command: &Command) -> bool {
    matches!(command, Command::ReadFrame(_))
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::PageList(_) => "page list",
        Command::PageOpen(_) => "page open",
        Command::PageClose(_) => "page close",
        Command::PageSelect(_) => "page select",
        Command::Navigate(_) => "nav go",
        Command::NavStep(_) => "nav",
        Command::InputText(_) => "input type",
        Command::InputClick(_) => "input click",
        Command::InputKey(_) => "input key",
        Command::ReadPage(_) => "read",
        Command::ReadFrame(_) => "read frame",
        Command::WorkspaceList(_) => "workspace list",
        Command::WorkspaceSwitch(_) => "workspace switch",
        Command::SetVisibility(_) => "display",
    }
}

fn index_of(page: &Option<generated::PageTarget>) -> Option<u32> {
    page.as_ref().and_then(|target| target.index)
}

fn checked_url(url: &str) -> Result<(), Failure> {
    if url.chars().any(char::is_control) || !crate::is_supported_url(url) {
        return Err(Failure::bad(format!(
            "an address must use https://, http://, file:// or about:, not {url:?}"
        )));
    }
    Ok(())
}

fn run(browser: &mut impl Browser, command: &Command) -> Result<control_response::Result, Failure> {
    use control_response::Result as Out;
    let done = |()| Out::Done(generated::Done {});
    Ok(match command {
        Command::PageList(_) => Out::Pages(generated::Pages {
            pages: browser.pages()?,
        }),
        Command::PageOpen(open) => {
            checked_url(&open.url)?;
            browser.open_page(&open.url).map(done)?
        }
        Command::PageClose(close) => browser.close_page(close.index).map(done)?,
        Command::PageSelect(select) => browser.select_page(select.index).map(done)?,
        Command::Navigate(navigate) => {
            checked_url(&navigate.url)?;
            browser
                .navigate(index_of(&navigate.page), &navigate.url)
                .map(done)?
        }
        Command::NavStep(step) => match step.step() {
            NavStep::Unspecified => {
                return Err(Failure::bad("a navigation step is back, forward or reload"))
            }
            known => browser.step(index_of(&step.page), known).map(done)?,
        },
        Command::InputText(input) => {
            if input.text.is_empty() {
                return Err(Failure::bad("there is no text to type"));
            }
            browser
                .type_text(index_of(&input.page), &input.text)
                .map(done)?
        }
        Command::InputClick(click) => {
            if click.x < 0 || click.y < 0 {
                return Err(Failure::bad("a click point lies inside the window"));
            }
            browser
                .click(index_of(&click.page), click.x, click.y)
                .map(done)?
        }
        Command::InputKey(input) => {
            let key = Key::parse(&input.key).ok_or_else(|| {
                Failure::bad(format!(
                    "{:?} is not a key; use one of {}",
                    input.key,
                    Key::names().collect::<Vec<_>>().join(", ")
                ))
            })?;
            browser.key(index_of(&input.page), key).map(done)?
        }
        Command::ReadPage(read) => match read.field() {
            ReadField::Unspecified => return Err(Failure::bad("read url, title or text")),
            field => Out::Text(generated::Text {
                value: browser.read(index_of(&read.page), field)?,
            }),
        },
        Command::ReadFrame(_) => Out::Frame(browser.frame()?),
        Command::WorkspaceList(_) => Out::Workspaces(generated::Workspaces {
            workspaces: browser.workspaces()?,
        }),
        Command::WorkspaceSwitch(switch) => browser.switch_workspace(&switch.name).map(done)?,
        Command::SetVisibility(set) => match set.visibility() {
            Visibility::Unspecified => return Err(Failure::bad("display is visible or hidden")),
            wanted => browser.set_visibility(wanted).map(done)?,
        },
    })
}

/// Runs one request against `browser` and says what happened, including whether the window was
/// shown for it and how it is shown afterwards.
pub fn handle(browser: &mut impl Browser, request: &ControlRequest) -> ControlResponse {
    let mut promoted = false;
    let result = match &request.command {
        None => Err(Failure::bad("the request names no command")),
        Some(command) => {
            let decided = match command {
                // Showing or hiding is the command itself.
                Command::SetVisibility(_) => Plan::Run,
                _ => plan(
                    request.display(),
                    browser.visibility(),
                    needs_visual(command),
                    command_name(command),
                ),
            };
            match decided {
                Plan::Refuse(reason) => Err(Failure::new(FailureKind::NeedsVisible, reason)),
                Plan::Show => browser.set_visibility(Visibility::Visible).and_then(|()| {
                    promoted = true;
                    run(browser, command)
                }),
                Plan::Run => run(browser, command),
            }
        }
    };
    let mut response = ControlResponse {
        promoted,
        result: Some(result.unwrap_or_else(|failure| {
            let mut out = ControlFailure {
                reason: failure.reason,
                ..Default::default()
            };
            out.set_kind(failure.kind);
            control_response::Result::Failure(out)
        })),
        ..Default::default()
    };
    response.set_visibility(browser.visibility());
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivet_protocol::channel;

    /// A window in memory. Its first page stands for one the person opened from the frame.
    struct MemoryBrowser {
        pages: Vec<Page>,
        spaces: Vec<Workspace>,
        visibility: Visibility,
        typed: String,
        keys: Vec<u16>,
        steps: Vec<NavStep>,
    }

    impl MemoryBrowser {
        fn new(visibility: Visibility) -> Self {
            Self {
                pages: vec![Page {
                    index: 0,
                    active: true,
                    title: "From the frame".into(),
                    url: "https://frame.test/".into(),
                }],
                spaces: vec![
                    Workspace {
                        name: "Work".into(),
                        current: true,
                    },
                    Workspace {
                        name: "Home".into(),
                        current: false,
                    },
                ],
                visibility,
                typed: String::new(),
                keys: Vec::new(),
                steps: Vec::new(),
            }
        }

        fn active(&mut self, page: Option<u32>) -> Result<&mut Page, Failure> {
            match page {
                Some(index) => self
                    .pages
                    .iter_mut()
                    .find(|page| page.index == index)
                    .ok_or_else(|| Failure::not_found(format!("there is no page {index}"))),
                None => self
                    .pages
                    .iter_mut()
                    .find(|page| page.active)
                    .ok_or_else(|| Failure::not_found("there is no active page")),
            }
        }
    }

    impl Browser for MemoryBrowser {
        fn pages(&mut self) -> Result<Vec<Page>, Failure> {
            Ok(self.pages.clone())
        }
        fn open_page(&mut self, url: &str) -> Result<(), Failure> {
            for page in &mut self.pages {
                page.active = false;
            }
            self.pages.push(Page {
                index: self.pages.len() as u32,
                active: true,
                title: String::new(),
                url: url.into(),
            });
            Ok(())
        }
        fn close_page(&mut self, index: u32) -> Result<(), Failure> {
            self.active(Some(index))?;
            self.pages.retain(|page| page.index != index);
            Ok(())
        }
        fn select_page(&mut self, index: u32) -> Result<(), Failure> {
            self.active(Some(index))?;
            for page in &mut self.pages {
                page.active = page.index == index;
            }
            Ok(())
        }
        fn navigate(&mut self, page: Option<u32>, url: &str) -> Result<(), Failure> {
            self.active(page)?.url = url.into();
            Ok(())
        }
        fn step(&mut self, _page: Option<u32>, step: NavStep) -> Result<(), Failure> {
            self.steps.push(step);
            Ok(())
        }
        fn type_text(&mut self, _page: Option<u32>, text: &str) -> Result<(), Failure> {
            self.typed.push_str(text);
            Ok(())
        }
        fn click(&mut self, _page: Option<u32>, _x: i32, _y: i32) -> Result<(), Failure> {
            Ok(())
        }
        fn key(&mut self, _page: Option<u32>, key: Key) -> Result<(), Failure> {
            self.keys.push(key.virtual_key);
            Ok(())
        }
        fn read(&mut self, page: Option<u32>, field: ReadField) -> Result<String, Failure> {
            let page = self.active(page)?;
            match field {
                ReadField::Url => Ok(page.url.clone()),
                ReadField::Title => Ok(page.title.clone()),
                _ => Err(Failure::unavailable("no page text in memory")),
            }
        }
        fn frame(&mut self) -> Result<Frame, Failure> {
            assert_eq!(
                self.visibility,
                Visibility::Visible,
                "a hidden window was captured"
            );
            Ok(Frame {
                width: 1,
                height: 1,
                bgra: vec![0, 0, 0, 255],
                layout_dpi: 96,
            })
        }
        fn workspaces(&mut self) -> Result<Vec<Workspace>, Failure> {
            Ok(self.spaces.clone())
        }
        fn switch_workspace(&mut self, name: &str) -> Result<(), Failure> {
            if !self.spaces.iter().any(|space| space.name == name) {
                return Err(Failure::not_found(format!("there is no space {name:?}")));
            }
            for space in &mut self.spaces {
                space.current = space.name == name;
            }
            Ok(())
        }
        fn visibility(&self) -> Visibility {
            self.visibility
        }
        fn set_visibility(&mut self, visibility: Visibility) -> Result<(), Failure> {
            self.visibility = visibility;
            Ok(())
        }
    }

    fn request(display: DisplayPolicy, command: Command) -> ControlRequest {
        let mut request = ControlRequest {
            command: Some(command),
            ..Default::default()
        };
        request.set_display(display);
        request
    }

    /// Sends a request over the in-memory channel to a server thread that owns the browser.
    fn over_channel(
        browser: MemoryBrowser,
        request: ControlRequest,
    ) -> (ControlResponse, MemoryBrowser) {
        let (mut client, mut server) = channel::memory_pair();
        let serving = std::thread::spawn(move || {
            let mut browser = browser;
            let request = channel::accept(&mut server).unwrap();
            channel::reply(&mut server, handle(&mut browser, &request)).unwrap();
            browser
        });
        let response = channel::call(&mut client, &request).unwrap();
        (response, serving.join().unwrap())
    }

    fn failure(response: &ControlResponse) -> Option<&ControlFailure> {
        match &response.result {
            Some(control_response::Result::Failure(failure)) => Some(failure),
            _ => None,
        }
    }

    #[test]
    fn fr4_a_page_the_cli_opens_shares_the_window_state_with_frame_pages() {
        let browser = MemoryBrowser::new(Visibility::Visible);
        let (opened, browser) = over_channel(
            browser,
            request(
                DisplayPolicy::Auto,
                Command::PageOpen(generated::PageOpen {
                    url: "https://cli.test/".into(),
                }),
            ),
        );
        assert_eq!(failure(&opened), None);
        let (listed, _) = over_channel(
            browser,
            request(
                DisplayPolicy::Auto,
                Command::PageList(generated::PageList {}),
            ),
        );
        let Some(control_response::Result::Pages(pages)) = listed.result else {
            panic!("expected pages");
        };
        let urls: Vec<_> = pages.pages.iter().map(|page| page.url.as_str()).collect();
        assert_eq!(urls, ["https://frame.test/", "https://cli.test/"]);
    }

    #[test]
    fn fr4_navigation_input_read_and_workspace_commands_reach_the_browser() {
        let mut browser = MemoryBrowser::new(Visibility::Visible);
        let commands = [
            Command::Navigate(generated::Navigate {
                page: None,
                url: "https://next.test/".into(),
            }),
            Command::NavStep({
                let mut step = generated::NavigationStep::default();
                step.set_step(NavStep::Back);
                step
            }),
            Command::InputText(generated::InputText {
                page: None,
                text: "hello".into(),
            }),
            Command::InputKey(generated::InputKey {
                page: None,
                key: "Enter".into(),
            }),
            Command::WorkspaceSwitch(generated::WorkspaceSwitch {
                name: "Home".into(),
            }),
        ];
        for command in commands {
            let response = handle(&mut browser, &request(DisplayPolicy::Auto, command));
            assert_eq!(failure(&response), None);
        }
        let mut read = generated::ReadPage::default();
        read.set_field(ReadField::Url);
        let response = handle(
            &mut browser,
            &request(DisplayPolicy::Auto, Command::ReadPage(read)),
        );
        assert!(matches!(
            response.result,
            Some(control_response::Result::Text(ref text)) if text.value == "https://next.test/"
        ));
        assert_eq!(browser.typed, "hello");
        assert_eq!(browser.keys, [0x0d]);
        assert_eq!(browser.steps, [NavStep::Back]);
        assert!(browser
            .spaces
            .iter()
            .any(|space| space.name == "Home" && space.current));
    }

    #[test]
    fn fr4_an_address_that_could_run_a_script_is_refused() {
        let mut browser = MemoryBrowser::new(Visibility::Visible);
        let response = handle(
            &mut browser,
            &request(
                DisplayPolicy::Auto,
                Command::Navigate(generated::Navigate {
                    page: None,
                    url: "javascript:alert(1)".into(),
                }),
            ),
        );
        assert_eq!(failure(&response).unwrap().kind(), FailureKind::BadRequest);
        assert_eq!(browser.pages[0].url, "https://frame.test/");
    }

    #[test]
    fn fr5_hidden_session_navigates_without_showing_the_window() {
        let (response, browser) = over_channel(
            MemoryBrowser::new(Visibility::Hidden),
            request(
                DisplayPolicy::Hidden,
                Command::Navigate(generated::Navigate {
                    page: None,
                    url: "https://hidden.test/".into(),
                }),
            ),
        );
        assert_eq!(failure(&response), None);
        assert!(!response.promoted);
        assert_eq!(response.visibility(), Visibility::Hidden);
        assert_eq!(browser.pages[0].url, "https://hidden.test/");
    }

    #[test]
    fn fr5_hidden_policy_reports_why_a_frame_capture_stops() {
        let (response, browser) = over_channel(
            MemoryBrowser::new(Visibility::Hidden),
            request(
                DisplayPolicy::Hidden,
                Command::ReadFrame(generated::ReadFrame {}),
            ),
        );
        let failure = failure(&response).expect("a hidden capture must fail");
        assert_eq!(failure.kind(), FailureKind::NeedsVisible);
        assert!(failure.reason.contains("--display auto"));
        assert_eq!(browser.visibility, Visibility::Hidden);
        assert!(!response.promoted);
    }

    #[test]
    fn fr5_auto_shows_a_hidden_window_for_visual_evidence() {
        let (response, browser) = over_channel(
            MemoryBrowser::new(Visibility::Hidden),
            request(
                DisplayPolicy::Auto,
                Command::ReadFrame(generated::ReadFrame {}),
            ),
        );
        assert!(matches!(
            response.result,
            Some(control_response::Result::Frame(_))
        ));
        assert!(response.promoted);
        assert_eq!(response.visibility(), Visibility::Visible);
        assert_eq!(browser.visibility, Visibility::Visible);
    }

    #[test]
    fn fr5_auto_keeps_a_hidden_window_hidden_without_visual_need() {
        let mut browser = MemoryBrowser::new(Visibility::Hidden);
        let response = handle(
            &mut browser,
            &request(
                DisplayPolicy::Auto,
                Command::PageList(generated::PageList {}),
            ),
        );
        assert!(!response.promoted);
        assert_eq!(browser.visibility, Visibility::Hidden);
    }

    #[test]
    fn fr5_display_policy_table() {
        use DisplayPolicy::*;
        assert_eq!(plan(Hidden, Visibility::Visible, true, "x"), Plan::Run);
        assert_eq!(plan(Visible, Visibility::Hidden, false, "x"), Plan::Show);
        assert_eq!(plan(Auto, Visibility::Hidden, true, "x"), Plan::Show);
        assert_eq!(plan(Unspecified, Visibility::Hidden, true, "x"), Plan::Show);
        assert_eq!(plan(Auto, Visibility::Hidden, false, "x"), Plan::Run);
        assert!(matches!(
            plan(Hidden, Visibility::Hidden, true, "x"),
            Plan::Refuse(_)
        ));
        assert_eq!(plan(Hidden, Visibility::Hidden, false, "x"), Plan::Run);
    }

    #[test]
    fn fr4_named_keys_carry_their_virtual_key() {
        assert_eq!(
            Key::parse("ENTER"),
            Some(Key {
                virtual_key: 0x0d,
                character: Some('\r')
            })
        );
        assert_eq!(
            Key::parse("pagedown").map(|key| key.virtual_key),
            Some(0x22)
        );
        assert_eq!(Key::parse("hyper"), None);
    }
}
