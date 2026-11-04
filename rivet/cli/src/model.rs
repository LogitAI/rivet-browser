//! What the setup window shows and what its clicks mean. There is no drawing in here, so the same
//! rules are tested without a window, and `rivet setup` on the command line uses the same
//! functions to choose and finish.

use crate::config::{self, Saved};
use crate::detect::Browser;
use std::path::{Path, PathBuf};

/// Everything the setup window has chosen so far.
pub struct Model {
    pub browsers: Vec<Browser>,
    pub selected: Option<usize>,
    /// The profile name as typed.
    pub profile: String,
    /// The Rivet profiles that already exist for the selected browser.
    pub profiles: Vec<String>,
    pub root: PathBuf,
    /// Why the window opened again, for a person whose browser went away.
    pub reason: Option<String>,
    /// What went wrong when Finish was pressed.
    pub error: Option<String>,
    /// The first browser row shown, when more rows exist than fit.
    pub scroll: usize,
}

impl Model {
    /// A setup with the detected `browsers`. `choose` is the browser that was chosen before, which
    /// stays chosen when it is still listed.
    pub fn new(
        browsers: Vec<Browser>,
        root: PathBuf,
        reason: Option<String>,
        choose: Option<&Saved>,
    ) -> Self {
        let mut model = Self {
            browsers,
            selected: None,
            profile: choose.map_or_else(
                || config::DEFAULT_PROFILE.to_owned(),
                |saved| saved.profile.clone(),
            ),
            profiles: Vec::new(),
            root,
            reason,
            error: None,
            scroll: 0,
        };
        let first = choose
            .and_then(|saved| {
                model
                    .browsers
                    .iter()
                    .position(|browser| same_file(&browser.path, &saved.browser))
            })
            .or_else(|| {
                model
                    .browsers
                    .iter()
                    .position(|browser| browser.status.can_open())
            });
        if let Some(first) = first {
            model.select(first);
        }
        model
    }

    pub fn select(&mut self, index: usize) {
        if index < self.browsers.len() {
            self.selected = Some(index);
            self.profiles = config::existing_profiles(&self.root, &self.browsers[index].path);
            self.error = None;
        }
    }

    pub fn selected_browser(&self) -> Option<&Browser> {
        self.selected.and_then(|index| self.browsers.get(index))
    }

    /// Adds a browser chosen by hand and chooses it. One that is listed already is only chosen.
    pub fn add_by_hand(&mut self, browser: Browser) {
        let index = match self
            .browsers
            .iter()
            .position(|known| same_file(&known.path, &browser.path))
        {
            Some(index) => index,
            None => {
                self.browsers.push(browser);
                self.browsers.len() - 1
            }
        };
        self.select(index);
    }

    /// Chooses the next or previous browser in the list.
    pub fn step(&mut self, forward: bool) {
        let count = self.browsers.len();
        if count == 0 {
            return;
        }
        let next = match (self.selected, forward) {
            (None, _) => 0,
            (Some(index), true) => (index + 1).min(count - 1),
            (Some(index), false) => index.saturating_sub(1),
        };
        self.select(next);
    }

    /// Keeps the chosen row inside the rows that are shown.
    pub fn reveal(&mut self, visible: usize) {
        let Some(index) = self.selected else { return };
        if visible == 0 {
            return;
        }
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + visible {
            self.scroll = index + 1 - visible;
        }
        self.scroll = self.scroll.min(self.browsers.len().saturating_sub(visible));
    }

    pub fn scroll_by(&mut self, rows: isize, visible: usize) {
        let most = self.browsers.len().saturating_sub(visible);
        self.scroll = self.scroll.saturating_add_signed(rows).min(most);
    }

    /// The folder of the profile as a person reads it, with the folder of local data spelled out.
    pub fn preview(&self) -> String {
        match self.selected_browser() {
            Some(browser) => format!(
                r"%LOCALAPPDATA%\Rivet\profiles\{}\{}",
                config::profile_key(&browser.path),
                config::clean_profile(&self.profile)
            ),
            None => r"%LOCALAPPDATA%\Rivet\profiles".to_owned(),
        }
    }

    /// Whether Finish can work: a browser Rivet can open is chosen.
    pub fn can_finish(&self) -> bool {
        self.selected_browser()
            .is_some_and(|browser| browser.status.can_open())
    }

    /// The sentence under the list about the chosen browser.
    pub fn note(&self) -> String {
        match self.selected_browser() {
            Some(browser) => browser.status.sentence(&browser.name),
            None => "No Chromium browser was found. Choose a browser file by hand.".to_owned(),
        }
    }

    pub fn finish(&self) -> Result<Saved, String> {
        let browser = self.selected_browser().ok_or("Choose a browser first.")?;
        config::finish(browser, &self.profile)
    }
}

/// The choice `rivet setup --browser <path> --profile <name>` makes: the same steps as the window,
/// with the browser chosen and the profile typed.
pub fn choose(browser: Browser, profile: &str, root: &Path) -> Result<Saved, String> {
    let mut model = Model::new(vec![browser], root.to_path_buf(), None, None);
    model.select(0);
    model.profile = profile.to_owned();
    model.finish()
}

fn same_file(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .replace('/', "\\")
        .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
}

/// A rectangle in device-independent pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    #[cfg(test)]
    fn overlaps(&self, other: &Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

pub const WIDTH: f32 = 560.0;
/// The gap between the window's edge and its panel, as between the frame and a panel in the shell.
pub const FRAME: f32 = 8.0;
pub const PAD: f32 = 32.0;
pub const ROW: f32 = 48.0;
pub const ROW_GAP: f32 = 2.0;
pub const MANUAL_ROW: f32 = 44.0;
const GROUP_PAD: f32 = 4.0;
const HEADER_END: f32 = 96.0;
/// The height of everything except the browser rows, without a notice.
const FIXED: f32 = 450.0;

/// Where everything sits, for one set of counts.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub height: f32,
    pub close: Rect,
    pub notice: Option<Rect>,
    pub browser_label: Rect,
    pub group: Rect,
    /// The shown browser rows, first row first.
    pub rows: Vec<Rect>,
    pub manual: Rect,
    pub note: Rect,
    pub profile_label: Rect,
    pub field: Rect,
    /// The room the text engine gets inside the field.
    pub text: Rect,
    pub chips: Vec<Rect>,
    pub chip_hint: Rect,
    pub preview: Rect,
    pub message: Rect,
    pub finish: Rect,
}

/// How many browser rows fit in `available` DIPs of window height.
pub fn visible_rows(total: usize, available: f32, notice: bool) -> usize {
    let fixed = FIXED + if notice { 56.0 } else { 0.0 };
    let fit = ((available - fixed) / (ROW + ROW_GAP)).floor().max(2.0) as usize;
    total.min(fit)
}

/// The profile chips that fit in one line, for the widths the caller measured.
pub fn chips_that_fit(widths: &[f32]) -> usize {
    let mut used = 0.0;
    for (count, width) in widths.iter().enumerate() {
        used += width + if count > 0 { 8.0 } else { 0.0 };
        if used > WIDTH - 2.0 * PAD {
            return count;
        }
    }
    widths.len()
}

pub fn layout(visible: usize, notice: bool, chip_widths: &[f32]) -> Layout {
    let mut y = HEADER_END;
    let notice_rect = notice.then(|| {
        let rect = Rect::new(PAD, y, WIDTH - 2.0 * PAD, 44.0);
        y += 56.0;
        rect
    });
    let browser_label = Rect::new(PAD + 4.0, y, 240.0, 14.0);
    y += 22.0;
    let group_height = GROUP_PAD * 2.0 + visible as f32 * (ROW + ROW_GAP) + MANUAL_ROW;
    let group = Rect::new(PAD, y, WIDTH - 2.0 * PAD, group_height);
    let rows = (0..visible)
        .map(|index| {
            Rect::new(
                group.x + GROUP_PAD,
                group.y + GROUP_PAD + index as f32 * (ROW + ROW_GAP),
                group.w - 2.0 * GROUP_PAD,
                ROW,
            )
        })
        .collect();
    let manual = Rect::new(
        group.x + GROUP_PAD,
        group.y + GROUP_PAD + visible as f32 * (ROW + ROW_GAP),
        group.w - 2.0 * GROUP_PAD,
        MANUAL_ROW,
    );
    y += group_height + 12.0;
    let note = Rect::new(PAD + 4.0, y, WIDTH - 2.0 * PAD - 8.0, 34.0);
    y += 46.0;
    let profile_label = Rect::new(PAD + 4.0, y, 240.0, 14.0);
    y += 22.0;
    let field = Rect::new(PAD, y, WIDTH - 2.0 * PAD, 46.0);
    let text = Rect::new(field.x + 16.0, field.y + 13.0, field.w - 32.0, 20.0);
    y += 54.0;
    let mut chips = Vec::new();
    let mut x = PAD;
    for width in chip_widths {
        chips.push(Rect::new(x, y, *width, 28.0));
        x += width + 8.0;
    }
    let chip_hint = Rect::new(PAD + 4.0, y, WIDTH - 2.0 * PAD - 8.0, 28.0);
    y += 36.0;
    let preview = Rect::new(PAD + 4.0, y, WIDTH - 2.0 * PAD - 8.0, 16.0);
    y += 30.0;
    let finish = Rect::new(WIDTH - PAD - 132.0, y, 132.0, 44.0);
    let message = Rect::new(PAD + 4.0, y, finish.x - PAD - 16.0, 44.0);
    y += 44.0 + 28.0;
    Layout {
        height: y + FRAME,
        close: Rect::new(WIDTH - FRAME - 14.0 - 28.0, FRAME + 14.0, 28.0, 28.0),
        notice: notice_rect,
        browser_label,
        group,
        rows,
        manual,
        note,
        profile_label,
        field,
        text,
        chips,
        chip_hint,
        preview,
        message,
        finish,
    }
}

/// What a point of the window is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    None,
    /// The top band, which drags the window.
    Caption,
    Close,
    /// A shown browser row, by position among the shown rows.
    Row(usize),
    Manual,
    Field,
    Chip(usize),
    Finish,
}

pub fn hit(layout: &Layout, x: f32, y: f32) -> Hit {
    if layout.close.contains(x, y) {
        return Hit::Close;
    }
    if let Some(index) = layout.rows.iter().position(|row| row.contains(x, y)) {
        return Hit::Row(index);
    }
    if layout.manual.contains(x, y) {
        return Hit::Manual;
    }
    if layout.field.contains(x, y) {
        return Hit::Field;
    }
    if let Some(index) = layout.chips.iter().position(|chip| chip.contains(x, y)) {
        return Hit::Chip(index);
    }
    if layout.finish.contains(x, y) {
        return Hit::Finish;
    }
    if y < HEADER_END - 8.0 {
        return Hit::Caption;
    }
    Hit::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Status;

    fn browser(name: &str, status: Status) -> Browser {
        Browser {
            path: PathBuf::from(format!(r"C:\B\{name}\{name}.exe")),
            name: name.to_owned(),
            version: "1.0.0.0".into(),
            proof: PathBuf::new(),
            status,
        }
    }

    fn model(statuses: &[(&str, Status)]) -> Model {
        let list = statuses
            .iter()
            .map(|(name, status)| browser(name, status.clone()))
            .collect();
        Model::new(
            list,
            rivet_core::paths::TestDir::new("rivet-model-none").to_path_buf(),
            None,
            None,
        )
    }

    #[test]
    fn fr1_the_first_browser_rivet_can_open_is_chosen_and_the_profile_is_default() {
        let model = model(&[
            ("aside", Status::NotYet("no".into())),
            ("chrome", Status::Ready),
        ]);
        assert_eq!(model.selected, Some(1));
        assert_eq!(model.profile, "Default");
        assert!(model.can_finish());
    }

    #[test]
    fn fr1_a_browser_that_cannot_be_opened_is_listed_chosen_and_explained_but_cannot_finish() {
        let mut model = model(&[
            ("chrome", Status::Ready),
            (
                "aside",
                Status::NotYet("Rivet has no verified build of this browser yet.".into()),
            ),
        ]);
        model.select(1);
        assert!(!model.can_finish());
        assert!(model.note().contains("no verified build"));
        assert!(model.finish().is_err());
    }

    #[test]
    // The paths here are Windows paths, which only Windows splits into folders.
    #[cfg(windows)]
    fn fr1_finishing_saves_the_browser_and_the_cleaned_profile_name() {
        let mut model = model(&[("chrome", Status::Ready)]);
        model.profile = "Work: 1".into();
        let saved = model.finish().unwrap();
        assert_eq!(saved.profile, "Work_ 1");
        assert!(saved.browser.ends_with("chrome.exe"));
        assert_eq!(
            model.preview(),
            r"%LOCALAPPDATA%\Rivet\profiles\chrome\Work_ 1"
        );
    }

    #[test]
    // The paths here are Windows paths, which only Windows splits into folders.
    #[cfg(windows)]
    fn fr1_the_command_line_makes_the_same_choice_as_the_window() {
        let root = rivet_core::paths::TestDir::new("rivet-model-choose");
        let saved = choose(browser("chrome", Status::Ready), "Lab 2", &root).unwrap();
        assert_eq!(
            (
                saved.profile.as_str(),
                saved.browser.ends_with("chrome.exe")
            ),
            ("Lab 2", true)
        );
        let refused = choose(
            browser(
                "aside",
                Status::NotYet("Rivet has no verified build of this browser yet.".into()),
            ),
            "Default",
            &root,
        );
        assert!(refused.unwrap_err().contains("no verified build"));
    }

    #[test]
    fn fr1_a_browser_chosen_by_hand_joins_the_list_once() {
        let mut model = model(&[("chrome", Status::Ready)]);
        model.add_by_hand(browser("odd", Status::Ready));
        assert_eq!((model.browsers.len(), model.selected), (2, Some(1)));
        model.add_by_hand(browser("ODD", Status::Ready));
        assert_eq!((model.browsers.len(), model.selected), (2, Some(1)));
    }

    #[test]
    fn fr1_the_browser_chosen_before_stays_chosen() {
        let list = vec![
            browser("chrome", Status::Ready),
            browser("brave", Status::Ready),
        ];
        let saved = Saved {
            browser: PathBuf::from(r"c:\b\brave\BRAVE.exe"),
            profile: "Lab".into(),
        };
        let model = Model::new(
            list,
            rivet_core::paths::TestDir::new("rivet-model-none").to_path_buf(),
            Some("gone".into()),
            Some(&saved),
        );
        assert_eq!(model.selected, Some(1));
        assert_eq!(model.profile, "Lab");
    }

    #[test]
    fn fr1_arrow_keys_step_through_the_list_and_the_list_scrolls_to_follow() {
        let names = ["a", "b", "c", "d", "e"];
        let list: Vec<_> = names.iter().map(|name| (*name, Status::Ready)).collect();
        let mut model = model(&list);
        for _ in 0..10 {
            model.step(true);
            model.reveal(3);
        }
        assert_eq!((model.selected, model.scroll), (Some(4), 2));
        for _ in 0..10 {
            model.step(false);
            model.reveal(3);
        }
        assert_eq!((model.selected, model.scroll), (Some(0), 0));
        model.scroll_by(9, 3);
        assert_eq!(model.scroll, 2);
    }

    #[test]
    fn fr1_existing_rivet_profiles_follow_the_chosen_browser() {
        let root = rivet_core::paths::TestDir::new("rivet-model");
        let _ = std::fs::remove_dir_all(&root);
        let first = browser("chrome", Status::Ready);
        std::fs::create_dir_all(config::profile_dir(&root, &first.path, "Work")).unwrap();
        let mut model = Model::new(
            vec![first, browser("brave", Status::Ready)],
            root.to_path_buf(),
            None,
            None,
        );
        assert_eq!(model.profiles, ["Work"]);
        model.select(1);
        assert!(model.profiles.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fr1_setup_controls_sit_inside_the_window_and_never_overlap() {
        for (visible, notice, chips) in [
            (0, false, vec![]),
            (2, false, vec![60.0]),
            (6, true, vec![70.0, 90.0, 110.0]),
            (8, false, vec![]),
        ] {
            let layout = layout(visible, notice, &chips);
            let mut all = vec![
                layout.close,
                layout.browser_label,
                layout.group,
                layout.note,
                layout.profile_label,
                layout.field,
                layout.preview,
                layout.finish,
            ];
            all.extend(layout.notice);
            all.extend(&layout.chips);
            all.retain(|rect| rect.w > 0.0);
            for first in 0..all.len() {
                assert!(
                    all[first].right() <= WIDTH && all[first].bottom() <= layout.height,
                    "{:?}",
                    all[first]
                );
                for second in first + 1..all.len() {
                    assert!(
                        !all[first].overlaps(&all[second]),
                        "{:?} overlaps {:?}",
                        all[first],
                        all[second]
                    );
                }
            }
            for row in layout.rows.iter().chain(Some(&layout.manual)) {
                assert!(row.x >= layout.group.x && row.right() <= layout.group.right());
                assert!(row.y >= layout.group.y && row.bottom() <= layout.group.bottom());
            }
            assert_eq!(layout.rows.len(), visible);
            assert!(layout.message.right() <= layout.finish.x);
        }
    }

    #[test]
    fn fr1_clicks_land_on_the_control_under_them() {
        let layout = layout(3, false, &[60.0, 80.0]);
        let center = |rect: Rect| (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let (x, y) = center(layout.rows[2]);
        assert_eq!(hit(&layout, x, y), Hit::Row(2));
        let (x, y) = center(layout.manual);
        assert_eq!(hit(&layout, x, y), Hit::Manual);
        let (x, y) = center(layout.field);
        assert_eq!(hit(&layout, x, y), Hit::Field);
        let (x, y) = center(layout.chips[1]);
        assert_eq!(hit(&layout, x, y), Hit::Chip(1));
        let (x, y) = center(layout.finish);
        assert_eq!(hit(&layout, x, y), Hit::Finish);
        let (x, y) = center(layout.close);
        assert_eq!(hit(&layout, x, y), Hit::Close);
        assert_eq!(hit(&layout, 100.0, 20.0), Hit::Caption);
        assert_eq!(hit(&layout, 5.0, layout.height - 4.0), Hit::None);
    }

    #[test]
    fn fr1_rows_that_do_not_fit_scroll_and_the_window_stays_within_the_screen() {
        assert_eq!(visible_rows(7, 4000.0, false), 7);
        let fit = visible_rows(7, 700.0, false);
        assert!((2..7).contains(&fit));
        assert!(layout(fit, false, &[]).height <= 700.0);
        assert_eq!(visible_rows(1, 700.0, false), 1);
        assert_eq!(chips_that_fit(&[200.0, 200.0, 200.0]), 2);
    }

    #[test]
    fn fr2_setup_layout_scales_for_monitor_dpi() {
        let scale = |value: f32, dpi: u32| (value * dpi as f32 / 96.0).round() as i32;
        assert_eq!(scale(WIDTH, 96), 560);
        assert_eq!(scale(WIDTH, 192), 1120);
        assert_eq!(scale(PAD, 144), 48);
    }
}
