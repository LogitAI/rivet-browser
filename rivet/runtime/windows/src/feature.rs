//! The features of a Rivet window, what each needs from the browser, and how each one fared.
//!
//! A feature names the targets it needs. Whether it is available follows from the build's row: the
//! row must mark it checked, which is an explicit policy and not a guess, and every target it
//! needs must be recorded. The outcome of every feature is kept for the life of the process and
//! written to the trace, so a feature that did not install is never silent.

use std::sync::Mutex;

use crate::builds::Build;
use crate::chromium::image;
use crate::targets::{Miss, Target};

/// The user-visible features that attach to a browser window, in the order they are installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feature {
    /// Chromium's frame leaves the appbar's strip free.
    FrameLayout,
    /// The title bar takes the colour of the space.
    FrameColour,
    /// Caption buttons are painted in the window's colours.
    CaptionPaint,
    /// Caption buttons sit in the panel's top row.
    CaptionLayout,
    /// The height of the top row.
    CaptionHeight,
    /// The space above the top row.
    TopInset,
    /// Pages of a split view are panels.
    Split,
    /// The layout of the pages of a split view.
    SplitLayout,
    /// The browser's own vertical tab strip, reported to the appbar.
    VerticalTabs,
    /// The browser's tab strip is left out of the layout.
    TabStrip,
}

impl Feature {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Feature::FrameLayout => "frame_layout",
            Feature::FrameColour => "frame_colour",
            Feature::CaptionPaint => "caption_paint",
            Feature::CaptionLayout => "caption_layout",
            Feature::CaptionHeight => "caption_height",
            Feature::TopInset => "top_inset",
            Feature::Split => "split",
            Feature::SplitLayout => "split_layout",
            Feature::VerticalTabs => "vertical_tabs",
            Feature::TabStrip => "tab_strip",
        }
    }
}

/// What happened to one feature in this process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Installed,
    /// The build does not have what the feature needs, or its row switches the feature off.
    NotInThisBuild(String),
    /// Everything the feature needs is there, and installing it still failed.
    Failed(String),
}

impl Outcome {
    pub(crate) fn describe(&self) -> String {
        match self {
            Outcome::Installed => "installed".to_owned(),
            Outcome::NotInThisBuild(reason) => format!("not in this build ({reason})"),
            Outcome::Failed(reason) => format!("failed ({reason})"),
        }
    }
}

static OUTCOMES: Mutex<Vec<(Feature, Outcome)>> = Mutex::new(Vec::new());

/// Keeps the outcome of a feature and writes it to the trace.
pub(crate) fn record(feature: Feature, outcome: Outcome) {
    crate::trace::trace(&format!(
        "feature {}: {}",
        feature.name(),
        outcome.describe()
    ));
    let mut all = OUTCOMES.lock().unwrap_or_else(|error| error.into_inner());
    all.retain(|(known, _)| *known != feature);
    all.push((feature, outcome));
}

/// The outcome recorded for a feature, if it has been tried.
#[allow(dead_code)]
pub(crate) fn outcome(feature: Feature) -> Option<Outcome> {
    let all = OUTCOMES.lock().unwrap_or_else(|error| error.into_inner());
    all.iter()
        .find(|(known, _)| *known == feature)
        .map(|(_, outcome)| outcome.clone())
}

/// Trace-only switches in `rivet-toolbar-skip.txt`, which let one change be left out while a
/// capture is compared.
pub(crate) fn debug_switch(token: &str) -> bool {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        crate::frame::debug_directory()
            .and_then(|dir| std::fs::read_to_string(dir.join("rivet-toolbar-skip.txt")).ok())
            .unwrap_or_default()
    })
    .contains(token)
}

/// What a feature needs from the build.
pub(crate) struct Needs {
    /// Whether the row marks the feature checked. `Err` carries the reason it is switched off.
    pub policy: Result<(), &'static str>,
    pub targets: Vec<Target>,
}

impl Needs {
    /// A feature whose row carries a flag that it has been checked on this build.
    pub(crate) fn checked(checked: bool, reason: &'static str, targets: Vec<Target>) -> Self {
        Self {
            policy: if checked { Ok(()) } else { Err(reason) },
            targets,
        }
    }

    /// A feature that is present whenever its targets are.
    pub(crate) fn targets(targets: Vec<Target>) -> Self {
        Self {
            policy: Ok(()),
            targets,
        }
    }

    /// Why the feature is not in this build, derived from the row alone.
    pub(crate) fn unavailable(&self) -> Option<String> {
        if let Err(reason) = self.policy {
            return Some(
                match self.targets.iter().find(|target| !target.recorded()) {
                    Some(target) => format!("{reason}; {} is not recorded", target.name),
                    None => reason.to_owned(),
                },
            );
        }
        self.targets
            .iter()
            .find(|target| !target.recorded())
            .map(|target| Miss::Absent(target.name).describe())
    }
}

impl Needs {
    /// The needs of a process whose browser has no row.
    #[cfg_attr(test, allow(dead_code))]
    fn no_row() -> Self {
        Self {
            policy: Err("this browser build has no row"),
            targets: Vec::new(),
        }
    }

    /// Checks the policy and every target against the loaded module.
    pub(crate) unsafe fn verify(&self, image: &image::Image) -> Result<(), String> {
        if let Some(reason) = self.unavailable() {
            return Err(reason);
        }
        match Target::first_miss(&self.targets, image) {
            Some(miss) => Err(miss.describe()),
            None => Ok(()),
        }
    }
}

/// The needs of a feature in the build loaded in this process.
pub(crate) fn needs_here(needs: fn(&Build) -> Needs) -> Needs {
    #[cfg(test)]
    return needs(crate::builds::get());
    #[cfg(not(test))]
    crate::builds::current().map_or_else(Needs::no_row, needs)
}

/// Whether the loaded build's row has the feature: checked, with every target recorded.
pub(crate) fn available_here(needs: fn(&Build) -> Needs) -> bool {
    needs_here(needs).unavailable().is_none()
}

/// How an installation that fails ends the attach.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnFailure {
    /// The window stays a plain browser window.
    Abort,
    Continue,
}

/// One feature in the attach sequence.
pub(crate) struct FeatureStep<C> {
    pub feature: Feature,
    pub needs: fn(&Build) -> Needs,
    pub install: fn(&mut C) -> Result<(), String>,
    pub on_failure: OnFailure,
    /// The installer is not run at all when the row says the feature is not in this build.
    /// Otherwise it runs and does its own checks, as it always has.
    pub skip_when_unavailable: bool,
    /// Written to the log, not only to the trace, when installing fails.
    pub log_failure: Option<&'static str>,
}

/// An entry of the attach sequence.
pub(crate) enum Step<C> {
    Feature(FeatureStep<C>),
    /// Work that is not a feature, kept in its place in the sequence.
    /// Returns false to end the attach.
    Setup(unsafe fn(&mut C) -> bool),
}

/// Runs a sequence. Returns false when a feature that must install did not.
pub(crate) unsafe fn run<C>(steps: &[Step<C>], context: &mut C, log: fn(&str)) -> bool {
    for step in steps {
        match step {
            Step::Setup(setup) => {
                if !setup(context) {
                    return false;
                }
            }
            Step::Feature(step) => {
                let (outcome, error) = try_install(step, context);
                // Only an installer that said it failed ends the attach. A feature the row leaves
                // out is not a failure.
                let failed = error.is_some();
                if let (Some(message), Some(error)) = (step.log_failure, error) {
                    log(&format!("{message}: {error}"));
                }
                record(step.feature, outcome);
                if failed && step.on_failure == OnFailure::Abort {
                    return false;
                }
            }
        }
    }
    true
}

/// Runs one installer. Also returns what the installer itself said when it failed.
unsafe fn try_install<C>(step: &FeatureStep<C>, context: &mut C) -> (Outcome, Option<String>) {
    let needs = needs_here(step.needs);
    let reason = needs.unavailable();
    if step.skip_when_unavailable {
        if let Some(reason) = reason {
            return (Outcome::NotInThisBuild(reason), None);
        }
    }
    match (step.install)(context) {
        // An installer that has nothing to do for a build the row leaves the feature out of reports
        // success, and the row's policy says what happened.
        Ok(()) => match needs.policy {
            Err(reason) => (Outcome::NotInThisBuild(reason.to_owned()), None),
            Ok(()) => (Outcome::Installed, None),
        },
        Err(error) => {
            let outcome = match reason {
                Some(reason) => Outcome::NotInThisBuild(reason),
                None => match image::recorded()
                    .and_then(|image| Target::first_miss(&needs.targets, &image))
                {
                    Some(miss) => Outcome::NotInThisBuild(miss.describe()),
                    None => Outcome::Failed(error.clone()),
                },
            };
            (outcome, Some(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builds;

    type Pick = (&'static str, fn(&Build) -> Needs, fn(&Build) -> bool);

    fn picks() -> [Pick; 5] {
        [
            ("navigation", crate::features::navigation::needs, |b| {
                b.navigation.supported
            }),
            ("tabs", crate::features::tabs::needs, |b| b.tabs.supported),
            ("split", crate::features::split::needs, |b| {
                b.split.supported
            }),
            ("sidebar", crate::features::side_panel::needs, |b| {
                b.side_panel.supported
            }),
            (
                "vertical tabs",
                crate::features::vertical_tabs::needs,
                |b| b.vertical.supported,
            ),
        ]
    }

    /// The row flag is a policy, and the targets decide the rest: a checked feature has every
    /// target recorded, so the flag never claims what the row cannot do.
    #[test]
    fn nfr3_a_checked_feature_has_every_target_recorded() {
        for build in builds::all() {
            for (name, needs, checked) in picks() {
                let needs = needs(build);
                if checked(build) {
                    assert_eq!(
                        needs.unavailable(),
                        None,
                        "{} {} {:?} is checked but a target is not recorded",
                        build.browser,
                        build.version.map(|part| part.to_string()).join("."),
                        name
                    );
                } else {
                    assert!(needs.unavailable().is_some(), "{} {name}", build.browser);
                }
            }
        }
    }

    /// A row that records a feature's addresses and still switches it off says why.
    #[test]
    fn nfr3_a_feature_switched_off_with_addresses_names_its_reason() {
        let edge = &builds::EDGE;
        let vertical = crate::features::vertical_tabs::needs(edge);
        assert!(vertical.targets.iter().all(Target::recorded));
        assert_eq!(
            vertical.unavailable().as_deref(),
            Some("vertical tabs are not checked on Edge yet")
        );
        let sidebar = crate::features::side_panel::needs(edge);
        assert!(sidebar.targets.iter().any(Target::recorded));
        assert!(sidebar
            .unavailable()
            .unwrap()
            .starts_with("the only panel Edge registers has no page behind it"));
    }

    /// The Sidebar is on where a real window showed its rail and an entry's panel, and off where a
    /// build was not checked or has no panel to show.
    #[test]
    fn fr10_the_sidebar_is_offered_on_chrome_only() {
        let offered = |build: &Build| {
            crate::features::side_panel::needs(build)
                .unavailable()
                .is_none()
        };
        assert!(offered(&builds::CHROME) && offered(&builds::CHROME_156_74));
        assert!(
            !offered(&builds::BRAVE_63),
            "the Sidebar of this build has no Chromium row"
        );
        assert!(
            !offered(&builds::EDGE),
            "Edge's only entry cannot be opened"
        );
        assert!(
            !offered(&builds::BRAVE),
            "Brave .62 is not installed, so no byte of its row was compared"
        );
        assert!(
            !offered(&builds::WHALE) && !offered(&builds::SAMSUNG) && !offered(&builds::CHROME_150)
        );
    }

    type NeedsOf = fn(&Build) -> Needs;

    fn all_needs() -> [(Feature, NeedsOf); 7] {
        use crate::features::{frame_layout, split, tab_strip};
        [
            (Feature::FrameLayout, frame_layout::needs_layout),
            (Feature::FrameColour, frame_layout::needs_colour),
            (Feature::CaptionPaint, frame_layout::needs_caption_paint),
            (Feature::CaptionLayout, frame_layout::needs_caption_layout),
            (Feature::Split, split::needs),
            (Feature::TabStrip, tab_strip::needs),
            (Feature::VerticalTabs, crate::features::vertical_tabs::needs),
        ]
    }

    fn names(build: &Build) -> Vec<&'static str> {
        all_needs()
            .iter()
            .filter(|(_, needs)| needs(build).unavailable().is_none())
            .map(|(feature, _)| feature.name())
            .collect()
    }

    /// What each row offers, as the attach of that browser was seen to install it.
    #[test]
    fn nfr3_each_row_offers_the_features_its_browser_installs() {
        let chromium = [
            "frame_layout",
            "frame_colour",
            "caption_paint",
            "caption_layout",
            "split",
        ];
        assert_eq!(
            names(&builds::CHROME),
            [&chromium[..], &["vertical_tabs"]].concat()
        );
        assert_eq!(
            names(&builds::BRAVE),
            [
                "frame_layout",
                "caption_paint",
                "caption_layout",
                "vertical_tabs"
            ]
        );
        assert_eq!(names(&builds::EDGE), chromium);
        assert_eq!(
            names(&builds::CHROME_150),
            [
                "frame_layout",
                "frame_colour",
                "caption_paint",
                "caption_layout"
            ]
        );
        assert_eq!(
            names(&builds::WHALE),
            ["frame_layout", "frame_colour", "tab_strip"]
        );
        assert_eq!(names(&builds::SAMSUNG), ["frame_layout", "tab_strip"]);
    }

    fn nothing(_: &Build) -> Needs {
        Needs::targets(Vec::new())
    }

    fn left_out(_: &Build) -> Needs {
        Needs {
            policy: Err("the row leaves it out"),
            targets: Vec::new(),
        }
    }

    fn step(
        feature: Feature,
        needs: fn(&Build) -> Needs,
        install: fn(&mut Vec<&'static str>) -> Result<(), String>,
        on_failure: OnFailure,
    ) -> Step<Vec<&'static str>> {
        Step::Feature(FeatureStep {
            feature,
            needs,
            install,
            on_failure,
            skip_when_unavailable: false,
            log_failure: None,
        })
    }

    /// The attach keeps its order, goes on past a feature the row leaves out or a failure it can
    /// live without, and stops at a failure it cannot.
    #[test]
    fn nfr3_the_attach_runs_in_order_and_stops_only_at_a_failure_that_must_stop_it() {
        let steps = vec![
            step(
                Feature::FrameLayout,
                nothing,
                |ran| {
                    let _: () = ran.push("layout");
                    Ok(())
                },
                OnFailure::Abort,
            ),
            step(
                Feature::CaptionHeight,
                left_out,
                |ran| {
                    let _: () = ran.push("caption");
                    Ok(())
                },
                OnFailure::Abort,
            ),
            step(
                Feature::Split,
                nothing,
                |ran| {
                    Err({
                        ran.push("split");
                        "it failed".to_owned()
                    })
                },
                OnFailure::Continue,
            ),
            step(
                Feature::TopInset,
                nothing,
                |ran| {
                    let _: () = ran.push("inset");
                    Ok(())
                },
                OnFailure::Continue,
            ),
        ];
        let mut ran = Vec::new();
        assert!(unsafe { run(&steps, &mut ran, |_| {}) });
        assert_eq!(ran, ["layout", "caption", "split", "inset"]);
        assert_eq!(
            outcome(Feature::CaptionHeight),
            Some(Outcome::NotInThisBuild("the row leaves it out".to_owned()))
        );
        assert_eq!(
            outcome(Feature::Split),
            Some(Outcome::Failed("it failed".to_owned()))
        );
        assert_eq!(outcome(Feature::TopInset), Some(Outcome::Installed));

        let steps = vec![
            step(
                Feature::FrameLayout,
                nothing,
                |ran| {
                    Err({
                        ran.push("layout");
                        "no".to_owned()
                    })
                },
                OnFailure::Abort,
            ),
            step(
                Feature::CaptionHeight,
                nothing,
                |ran| {
                    let _: () = ran.push("caption");
                    Ok(())
                },
                OnFailure::Abort,
            ),
        ];
        let mut ran = Vec::new();
        assert!(!unsafe { run(&steps, &mut ran, |_| {}) });
        assert_eq!(ran, ["layout"]);
    }
}
