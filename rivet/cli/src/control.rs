//! `rivet page`, `nav`, `input`, `read`, `workspace` and `display`: commands for the pages of one
//! Rivet session, sent over the window's control channel and answered by the browser.

use std::path::{Path, PathBuf};

use rivet_protocol::{
    control_request::Command, control_response, generated, ControlRequest, ControlResponse,
    DisplayPolicy, NavStep, PageTarget, ReadField, Visibility,
};

/// The first words that name a control command.
pub const GROUPS: [&str; 6] = ["page", "nav", "input", "read", "workspace", "display"];

pub const USAGE: &str = "\
  rivet page list | open <url> | close <index> | select <index>
  rivet nav go <url> | back | forward | reload
  rivet input type <text> | click <x> <y> | key <name>
  rivet read url | title | text | frame --out <file.bmp>
  rivet workspace list | switch <name>
  rivet display visible | hidden
      Each takes [--id <session>] [--state <path>] [--page <index>] [--display visible|hidden|auto] [--json].
      Without --id the only Rivet session is used. --display defaults to auto, which shows a hidden
      window only for a command that needs visual evidence.\n";

/// A parsed control command.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlOptions {
    pub id: Option<String>,
    pub state: Option<PathBuf>,
    pub json: bool,
    /// Where `read frame` writes its capture.
    pub out: Option<PathBuf>,
    pub request: ControlRequest,
}

fn index(word: &str) -> Result<u32, String> {
    word.parse()
        .map_err(|_| format!("{word:?} is not a page index"))
}

fn coordinate(word: &str) -> Result<i32, String> {
    word.parse()
        .map_err(|_| format!("{word:?} is not a whole number"))
}

/// Reads a control command from the whole argument list.
pub fn parse(arguments: &[String]) -> Result<ControlOptions, String> {
    let mut id = None;
    let mut state = None;
    let mut json = false;
    let mut out = None;
    let mut page = None;
    let mut display = DisplayPolicy::Auto;
    let mut words = Vec::new();
    let mut at = 0;
    let value = |at: &mut usize, option: &str| -> Result<String, String> {
        *at += 1;
        arguments
            .get(*at)
            .cloned()
            .ok_or_else(|| format!("{option} needs a value"))
    };
    while at < arguments.len() {
        match arguments[at].as_str() {
            "--id" => id = Some(value(&mut at, "--id")?),
            "--state" => state = Some(PathBuf::from(value(&mut at, "--state")?)),
            "--out" => out = Some(PathBuf::from(value(&mut at, "--out")?)),
            "--page" => page = Some(index(&value(&mut at, "--page")?)?),
            "--display" => {
                display = match value(&mut at, "--display")?.as_str() {
                    "visible" => DisplayPolicy::Visible,
                    "hidden" => DisplayPolicy::Hidden,
                    "auto" => DisplayPolicy::Auto,
                    other => {
                        return Err(format!(
                            "--display must be visible, hidden or auto, not {other:?}"
                        ))
                    }
                }
            }
            "--json" => json = true,
            // A text to type may itself start with dashes after `--`.
            "--" => {
                words.extend(arguments[at + 1..].iter().map(String::as_str));
                break;
            }
            option if option.starts_with("--") => return Err(format!("unknown option {option:?}")),
            word => words.push(word),
        }
        at += 1;
    }
    let target = Some(PageTarget { index: page });
    let command = match words[..] {
        ["page", "list"] => Command::PageList(generated::PageList {}),
        ["page", "open", url] => Command::PageOpen(generated::PageOpen { url: url.into() }),
        ["page", "close", at] => Command::PageClose(generated::PageClose { index: index(at)? }),
        ["page", "select", at] => Command::PageSelect(generated::PageSelect { index: index(at)? }),
        ["nav", "go", url] => Command::Navigate(generated::Navigate {
            page: target,
            url: url.into(),
        }),
        ["nav", step @ ("back" | "forward" | "reload")] => {
            let mut message = generated::NavigationStep {
                page: target,
                ..Default::default()
            };
            message.set_step(match step {
                "back" => NavStep::Back,
                "forward" => NavStep::Forward,
                _ => NavStep::Reload,
            });
            Command::NavStep(message)
        }
        ["input", "type", text] => Command::InputText(generated::InputText {
            page: target,
            text: text.into(),
        }),
        ["input", "click", x, y] => Command::InputClick(generated::InputClick {
            page: target,
            x: coordinate(x)?,
            y: coordinate(y)?,
        }),
        ["input", "key", key] => Command::InputKey(generated::InputKey {
            page: target,
            key: key.into(),
        }),
        ["read", "frame"] => {
            if out.is_none() {
                return Err("read frame needs --out <file.bmp>".into());
            }
            Command::ReadFrame(generated::ReadFrame {})
        }
        ["read", field @ ("url" | "title" | "text")] => {
            let mut message = generated::ReadPage {
                page: target,
                ..Default::default()
            };
            message.set_field(match field {
                "url" => ReadField::Url,
                "title" => ReadField::Title,
                _ => ReadField::Text,
            });
            Command::ReadPage(message)
        }
        ["workspace", "list"] => Command::WorkspaceList(generated::WorkspaceList {}),
        ["workspace", "switch", name] => {
            Command::WorkspaceSwitch(generated::WorkspaceSwitch { name: name.into() })
        }
        ["display", shown @ ("visible" | "hidden")] => {
            let mut message = generated::SetVisibility::default();
            message.set_visibility(if shown == "visible" {
                Visibility::Visible
            } else {
                Visibility::Hidden
            });
            Command::SetVisibility(message)
        }
        _ => return Err(format!("expected one of:\n{USAGE}")),
    };
    let mut request = ControlRequest {
        command: Some(command),
        ..Default::default()
    };
    request.set_display(display);
    Ok(ControlOptions {
        id,
        state,
        json,
        out,
        request,
    })
}

fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Hidden => "hidden",
        _ => "visible",
    }
}

fn failure_name(kind: rivet_protocol::FailureKind) -> &'static str {
    use rivet_protocol::FailureKind::*;
    match kind {
        BadRequest => "bad_request",
        NotFound => "not_found",
        Unavailable => "unavailable",
        NeedsVisible => "needs_visible",
        BrowserRefused => "browser_refused",
        Unspecified => "failed",
    }
}

/// A capture as a 32-bit top-down BMP file, which every image viewer opens.
pub fn bmp(frame: &rivet_protocol::Frame) -> Result<Vec<u8>, String> {
    let size = frame.width as usize * frame.height as usize * 4;
    if frame.bgra.len() != size || size == 0 {
        return Err("the window sent a capture of the wrong size".into());
    }
    let mut out = Vec::with_capacity(54 + size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + size) as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(frame.width as i32).to_le_bytes());
    // A negative height marks rows from the top down.
    out.extend_from_slice(&(-(frame.height as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    out.extend_from_slice(&frame.bgra);
    Ok(out)
}

/// What to print for a response, or the error to report. A capture is written to `out` first.
pub fn render(
    response: &ControlResponse,
    json: bool,
    out: Option<&Path>,
) -> Result<String, String> {
    use control_response::Result as R;
    let visibility = visibility_name(response.visibility());
    let shown_note = if response.promoted {
        " (the hidden window was shown for this command)"
    } else {
        ""
    };
    let body = match &response.result {
        None => return Err("the Rivet window gave no result".into()),
        Some(R::Failure(failure)) => {
            return Err(if json {
                serde_json::json!({
                    "ok": false,
                    "error": failure_name(failure.kind()),
                    "reason": failure.reason,
                    "visibility": visibility,
                })
                .to_string()
            } else {
                failure.reason.clone()
            })
        }
        Some(R::Done(_)) => (serde_json::json!({}), "done".to_owned()),
        Some(R::Text(text)) => (
            serde_json::json!({ "value": text.value }),
            text.value.clone(),
        ),
        Some(R::Pages(pages)) => (
            serde_json::json!({ "pages": pages.pages.iter().map(|page| serde_json::json!({
                "index": page.index, "active": page.active, "title": page.title, "url": page.url,
            })).collect::<Vec<_>>() }),
            pages
                .pages
                .iter()
                .map(|page| {
                    format!(
                        "{}{}\t{}\t{}",
                        if page.active { "*" } else { " " },
                        page.index,
                        page.url,
                        page.title
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Some(R::Workspaces(spaces)) => (
            serde_json::json!({ "workspaces": spaces.workspaces.iter().map(|space| serde_json::json!({
                "name": space.name, "current": space.current,
            })).collect::<Vec<_>>() }),
            spaces
                .workspaces
                .iter()
                .map(|space| format!("{}{}", if space.current { "* " } else { "  " }, space.name))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Some(R::Frame(frame)) => {
            let path = out.ok_or("read frame needs --out <file.bmp>")?;
            std::fs::write(path, bmp(frame)?)
                .map_err(|error| format!("could not write {}: {error}", path.display()))?;
            (
                serde_json::json!({
                    "width": frame.width,
                    "height": frame.height,
                    "layout_dpi": frame.layout_dpi,
                    "path": path.to_string_lossy(),
                }),
                format!(
                    "wrote {}x{} capture at {} DPI to {}",
                    frame.width,
                    frame.height,
                    frame.layout_dpi,
                    path.display()
                ),
            )
        }
    };
    let (mut value, text) = body;
    if json {
        value["ok"] = true.into();
        value["visibility"] = visibility.into();
        value["promoted"] = response.promoted.into();
        Ok(value.to_string())
    } else {
        Ok(format!("{text}{shown_note}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| word.to_string()).collect()
    }

    fn command(line: &[&str]) -> Command {
        parse(&words(line)).unwrap().request.command.unwrap()
    }

    #[test]
    fn fr4_cli_names_page_navigation_input_read_and_workspace_commands() {
        assert!(matches!(command(&["page", "list"]), Command::PageList(_)));
        assert!(
            matches!(command(&["page", "open", "https://a.test/"]), Command::PageOpen(open) if open.url == "https://a.test/")
        );
        assert!(
            matches!(command(&["page", "close", "2"]), Command::PageClose(close) if close.index == 2)
        );
        assert!(matches!(
            command(&["page", "select", "1"]),
            Command::PageSelect(_)
        ));
        assert!(
            matches!(command(&["nav", "go", "https://b.test/", "--page", "3"]),
            Command::Navigate(go) if go.page == Some(PageTarget { index: Some(3) }))
        );
        assert!(
            matches!(command(&["nav", "forward"]), Command::NavStep(step) if step.step() == NavStep::Forward)
        );
        assert!(
            matches!(command(&["input", "type", "--", "--hello"]), Command::InputText(input) if input.text == "--hello")
        );
        assert!(
            matches!(command(&["input", "click", "10", "20"]), Command::InputClick(click) if click.x == 10 && click.y == 20)
        );
        assert!(matches!(
            command(&["input", "key", "enter"]),
            Command::InputKey(_)
        ));
        assert!(
            matches!(command(&["read", "title"]), Command::ReadPage(read) if read.field() == ReadField::Title)
        );
        assert!(matches!(
            command(&["read", "frame", "--out", "a.bmp"]),
            Command::ReadFrame(_)
        ));
        assert!(matches!(
            command(&["workspace", "list"]),
            Command::WorkspaceList(_)
        ));
        assert!(matches!(
            command(&["workspace", "switch", "Home"]),
            Command::WorkspaceSwitch(_)
        ));
        assert!(
            matches!(command(&["display", "hidden"]), Command::SetVisibility(set) if set.visibility() == Visibility::Hidden)
        );
        assert!(parse(&words(&["read", "frame"])).is_err());
        assert!(parse(&words(&["nav", "sideways"])).is_err());
        assert!(parse(&words(&["page", "close", "x"])).is_err());
    }

    #[test]
    fn fr5_every_control_command_takes_a_display_policy_defaulting_to_auto() {
        let parsed = parse(&words(&["read", "url", "--id", "s1", "--json"])).unwrap();
        assert_eq!(parsed.request.display(), DisplayPolicy::Auto);
        assert_eq!(parsed.id.as_deref(), Some("s1"));
        assert!(parsed.json);
        let hidden = parse(&words(&["read", "url", "--display", "hidden"])).unwrap();
        assert_eq!(hidden.request.display(), DisplayPolicy::Hidden);
        assert!(parse(&words(&["read", "url", "--display", "sometimes"])).is_err());
    }

    fn response(
        result: control_response::Result,
        visibility: Visibility,
        promoted: bool,
    ) -> ControlResponse {
        let mut response = ControlResponse {
            promoted,
            result: Some(result),
            ..Default::default()
        };
        response.set_visibility(visibility);
        response
    }

    #[test]
    fn fr4_output_is_human_by_default_and_json_when_asked() {
        let pages = response(
            control_response::Result::Pages(generated::Pages {
                pages: vec![rivet_protocol::Page {
                    index: 0,
                    active: true,
                    title: "A \"quoted\" title".into(),
                    url: "https://a.test/".into(),
                }],
            }),
            Visibility::Visible,
            false,
        );
        assert_eq!(
            render(&pages, false, None).unwrap(),
            "*0\thttps://a.test/\tA \"quoted\" title"
        );
        let json: serde_json::Value =
            serde_json::from_str(&render(&pages, true, None).unwrap()).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["pages"][0]["title"], "A \"quoted\" title");
        assert_eq!(json["visibility"], "visible");
    }

    #[test]
    fn fr5_hidden_failure_reports_its_reason_in_both_outputs() {
        let mut failure = rivet_protocol::ControlFailure {
            reason: "needs visual evidence".into(),
            ..Default::default()
        };
        failure.set_kind(rivet_protocol::FailureKind::NeedsVisible);
        let refused = response(
            control_response::Result::Failure(failure),
            Visibility::Hidden,
            false,
        );
        assert_eq!(
            render(&refused, false, None).unwrap_err(),
            "needs visual evidence"
        );
        let json: serde_json::Value =
            serde_json::from_str(&render(&refused, true, None).unwrap_err()).unwrap();
        assert_eq!(json["error"], "needs_visible");
        assert_eq!(json["visibility"], "hidden");
    }

    #[test]
    fn fr5_a_promotion_is_reported() {
        let done = response(
            control_response::Result::Done(generated::Done {}),
            Visibility::Visible,
            true,
        );
        assert!(render(&done, false, None).unwrap().contains("was shown"));
        let json: serde_json::Value =
            serde_json::from_str(&render(&done, true, None).unwrap()).unwrap();
        assert_eq!(json["promoted"], true);
    }

    #[test]
    fn fr4_read_frame_reports_its_layout_scale_in_json() {
        let dir = rivet_core::paths::TestDir::new("rivet-frame-scale");
        let path = dir.path().join("frame.bmp");
        let frame = response(
            control_response::Result::Frame(rivet_protocol::Frame {
                width: 1,
                height: 1,
                bgra: vec![0, 0, 0, 255],
                layout_dpi: 120,
            }),
            Visibility::Visible,
            false,
        );
        let json: serde_json::Value =
            serde_json::from_str(&render(&frame, true, Some(&path)).unwrap()).unwrap();
        assert_eq!(json["layout_dpi"], 120);
        assert!(render(&frame, false, Some(&path))
            .unwrap()
            .contains("at 120 DPI"));
    }

    #[test]
    fn fr5_a_capture_becomes_a_top_down_bitmap() {
        let frame = rivet_protocol::Frame {
            width: 2,
            height: 1,
            bgra: vec![1, 2, 3, 255, 4, 5, 6, 255],
            layout_dpi: 120,
        };
        let file = bmp(&frame).unwrap();
        assert_eq!(&file[..2], b"BM");
        assert_eq!(file.len(), 54 + 8);
        assert_eq!(i32::from_le_bytes(file[22..26].try_into().unwrap()), -1);
        assert!(bmp(&rivet_protocol::Frame {
            width: 3,
            height: 1,
            bgra: vec![0; 4],
            layout_dpi: 96,
        })
        .is_err());
    }
}
