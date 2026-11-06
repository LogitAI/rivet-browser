//! Commands the CLI and test tools post to a Rivet window as registered window messages.
//!
//! A window message carries two numbers, so each command is encoded into `wparam` and `lparam`
//! here, once, and both the sender and the runtime use these functions.

/// The environment variable Rivet sets for a browser it starts hidden. The runtime cloaks the first
/// window it attaches to in such a process, before the window is ever shown.
pub const HIDDEN_START_VARIABLE: &str = "RIVET_START_HIDDEN";

/// The registered window message that carries a Mini Window command.
pub const MINI_WINDOW_MESSAGE: &str = "Rivet.MiniWindow";

/// The edge of the page a Mini Window returns to as a split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

/// What a Mini Window command asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniWindowCommand {
    /// The focused page becomes a Mini Window: a page of a split leaves the split, and a single
    /// page leaves the window's main view.
    PopOut,
    /// The Mini Window's top left corner moves to (`x`, `y`) in client DIPs.
    Move { x: i16, y: i16 },
    /// The Mini Window takes `width` by `height` DIPs.
    Resize { width: u16, height: u16 },
    /// The Mini Window goes back to its tab as a split on `edge` of the page.
    Return(Edge),
    /// The Mini Window closes and its page stays in its tab.
    Close,
}

const POP_OUT: usize = 1;
const MOVE: usize = 2;
const RESIZE: usize = 3;
const RETURN: usize = 4;
const CLOSE: usize = 5;

fn pack(low: u16, high: u16) -> isize {
    (low as u32 | (high as u32) << 16) as i32 as isize
}

fn unpack(value: isize) -> (u16, u16) {
    let value = value as u32;
    ((value & 0xffff) as u16, (value >> 16) as u16)
}

impl MiniWindowCommand {
    /// The `wparam` and `lparam` of the window message.
    pub fn encode(self) -> (usize, isize) {
        match self {
            Self::PopOut => (POP_OUT, 0),
            Self::Move { x, y } => (MOVE, pack(x as u16, y as u16)),
            Self::Resize { width, height } => (RESIZE, pack(width, height)),
            Self::Return(edge) => (
                RETURN,
                match edge {
                    Edge::Left => 0,
                    Edge::Right => 1,
                    Edge::Top => 2,
                    Edge::Bottom => 3,
                },
            ),
            Self::Close => (CLOSE, 0),
        }
    }

    /// The command of a window message, or none for one this protocol does not define.
    pub fn decode(wparam: usize, lparam: isize) -> Option<Self> {
        let (low, high) = unpack(lparam);
        Some(match wparam {
            POP_OUT => Self::PopOut,
            MOVE => Self::Move {
                x: low as i16,
                y: high as i16,
            },
            RESIZE => Self::Resize {
                width: low,
                height: high,
            },
            RETURN => Self::Return(match lparam {
                0 => Edge::Left,
                1 => Edge::Right,
                2 => Edge::Top,
                3 => Edge::Bottom,
                _ => return None,
            }),
            CLOSE => Self::Close,
            _ => return None,
        })
    }

    /// The command a person or agent names on the command line: `pop-out`, `move <x> <y>`,
    /// `resize <width> <height>`, `return left|right|top|bottom` or `close`.
    pub fn parse(words: &[&str]) -> Result<Self, String> {
        let number = |at: usize| -> Result<i64, String> {
            words
                .get(at)
                .ok_or_else(|| format!("`{}` needs two numbers", words[0]))?
                .parse::<i64>()
                .map_err(|_| format!("{:?} is not a whole number", words[at]))
        };
        match words.first().copied() {
            Some("pop-out") if words.len() == 1 => Ok(Self::PopOut),
            Some("close") if words.len() == 1 => Ok(Self::Close),
            Some("move") if words.len() == 3 => {
                let (x, y) = (number(1)?, number(2)?);
                let fit = |value: i64| i16::try_from(value).map_err(|_| format!("{value} is out of range"));
                Ok(Self::Move { x: fit(x)?, y: fit(y)? })
            }
            Some("resize") if words.len() == 3 => {
                let (width, height) = (number(1)?, number(2)?);
                let fit = |value: i64| u16::try_from(value).map_err(|_| format!("{value} is out of range"));
                Ok(Self::Resize { width: fit(width)?, height: fit(height)? })
            }
            Some("return") if words.len() == 2 => Ok(Self::Return(match words[1] {
                "left" => Edge::Left,
                "right" => Edge::Right,
                "top" => Edge::Top,
                "bottom" => Edge::Bottom,
                other => return Err(format!("{other:?} is not left, right, top or bottom")),
            })),
            _ => Err("expected `pop-out`, `move <x> <y>`, `resize <width> <height>`, `return left|right|top|bottom` or `close`".into()),
        }
    }
}

/// The `dwData` of a `WM_COPYDATA` message that asks a Rivet window to open a link. An address
/// does not fit in two numbers, so it travels as copied data: the bytes `RVL1` and then the
/// address in UTF-8. The window routes it by its link rules.
pub const OPEN_LINK_DATA: usize = 0x5256_4c31;
const OPEN_LINK_MAGIC: &[u8; 4] = b"RVL1";
/// The longest address a request may carry. Chromium shows at most this much of an address.
pub const MAX_LINK_BYTES: usize = 32 * 1024;

/// The copied data of a request to open `url`. Only `http` and `https` addresses and local
/// `.htm` or `.html` files are sent, so a request can never run a script or open another file.
pub fn encode_open_link(url: &str) -> Result<Vec<u8>, String> {
    check_link(url)?;
    let mut bytes = OPEN_LINK_MAGIC.to_vec();
    bytes.extend_from_slice(url.as_bytes());
    Ok(bytes)
}

/// The address of a request, or none for data this protocol does not define.
pub fn decode_open_link(data: usize, bytes: &[u8]) -> Option<String> {
    if data != OPEN_LINK_DATA {
        return None;
    }
    let url = std::str::from_utf8(bytes.strip_prefix(OPEN_LINK_MAGIC)?).ok()?;
    check_link(url).ok()?;
    Some(url.to_owned())
}

fn check_link(url: &str) -> Result<(), String> {
    if url.len() > MAX_LINK_BYTES {
        return Err(format!("an address is at most {MAX_LINK_BYTES} bytes"));
    }
    let lower = url.to_ascii_lowercase();
    let web = lower.starts_with("https://") || lower.starts_with("http://");
    // A page file a person opens from Explorer, when Rivet is the app for `.htm` and `.html`.
    let page_file = lower.starts_with("file:///")
        && (lower.ends_with(".html") || lower.ends_with(".htm"))
        && !lower.contains(['?', '#']);
    if !(web || page_file) {
        return Err(format!(
            "{url:?} is not an http or https address or an HTML file"
        ));
    }
    if url.chars().any(char::is_control) {
        return Err("an address has no control characters".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr3_an_open_link_request_survives_the_copied_data() {
        let bytes = encode_open_link("https://github.com/LogitAI").unwrap();
        assert_eq!(&bytes[..4], b"RVL1");
        assert_eq!(&bytes[4..], b"https://github.com/LogitAI");
        assert_eq!(
            decode_open_link(OPEN_LINK_DATA, &bytes).as_deref(),
            Some("https://github.com/LogitAI")
        );
        assert_eq!(decode_open_link(OPEN_LINK_DATA + 1, &bytes), None);
        assert_eq!(
            decode_open_link(OPEN_LINK_DATA, b"RVL0https://a.example/"),
            None
        );
        assert_eq!(decode_open_link(OPEN_LINK_DATA, b"RVL1\xff"), None);
    }

    #[test]
    fn fr3_an_open_link_request_carries_only_web_addresses() {
        assert!(encode_open_link("HTTP://example.com/").is_ok());
        assert!(encode_open_link("javascript:alert(1)").is_err());
        assert!(encode_open_link("file:///C:/Windows/").is_err());
        assert!(encode_open_link("file:///C:/Users/a/page.HTML").is_ok());
        assert!(encode_open_link("file:///C:/Windows/system.ini").is_err());
        assert!(encode_open_link("file:///C:/a.html?x=.html").is_err());
        assert!(
            encode_open_link("file:///C:/Users/a/page.HTML").is_ok(),
            "an HTML file from Explorer"
        );
        assert!(encode_open_link("file:///C:/Windows/system.ini").is_err());
        assert!(encode_open_link("https://a.example/\n").is_err());
        assert!(
            encode_open_link(&format!("https://a.example/{}", "x".repeat(MAX_LINK_BYTES))).is_err()
        );
        assert_eq!(
            decode_open_link(OPEN_LINK_DATA, b"RVL1chrome://settings"),
            None
        );
    }

    #[test]
    fn fr9_every_mini_window_command_survives_the_window_message() {
        let all = [
            MiniWindowCommand::PopOut,
            MiniWindowCommand::Move { x: -40, y: 300 },
            MiniWindowCommand::Move { x: 1200, y: -1 },
            MiniWindowCommand::Resize {
                width: 360,
                height: 270,
            },
            MiniWindowCommand::Return(Edge::Left),
            MiniWindowCommand::Return(Edge::Right),
            MiniWindowCommand::Return(Edge::Top),
            MiniWindowCommand::Return(Edge::Bottom),
            MiniWindowCommand::Close,
        ];
        for command in all {
            let (wparam, lparam) = command.encode();
            assert_eq!(MiniWindowCommand::decode(wparam, lparam), Some(command));
        }
    }

    #[test]
    fn fr9_an_unknown_mini_window_message_is_ignored() {
        assert_eq!(MiniWindowCommand::decode(0, 0), None);
        assert_eq!(MiniWindowCommand::decode(99, 0), None);
        assert_eq!(MiniWindowCommand::decode(RETURN, 7), None);
    }

    #[test]
    fn fr9_mini_window_commands_parse_from_the_command_line() {
        assert_eq!(
            MiniWindowCommand::parse(&["pop-out"]),
            Ok(MiniWindowCommand::PopOut)
        );
        assert_eq!(
            MiniWindowCommand::parse(&["move", "10", "-20"]),
            Ok(MiniWindowCommand::Move { x: 10, y: -20 })
        );
        assert_eq!(
            MiniWindowCommand::parse(&["return", "top"]),
            Ok(MiniWindowCommand::Return(Edge::Top))
        );
        assert_eq!(
            MiniWindowCommand::parse(&["close"]),
            Ok(MiniWindowCommand::Close)
        );
        assert!(MiniWindowCommand::parse(&["return", "middle"]).is_err());
        assert!(MiniWindowCommand::parse(&["move", "1"]).is_err());
        assert!(MiniWindowCommand::parse(&["resize", "-1", "4"]).is_err());
        assert!(MiniWindowCommand::parse(&[]).is_err());
    }
}
