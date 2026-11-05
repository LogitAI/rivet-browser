//! The top bar's caret on a small visual of its own, inside the top bar's visual. A blink shows or
//! hides it and a caret move moves it, without painting the bar again.

use super::*;

/// The caret's width in DIPs, as the field's text engine would draw it.
const CARET_WIDTH: f32 = 1.2;

pub(super) struct AddressCaret {
    visual: IDCompositionVisual2,
    /// The caret's surface and its size in pixels, kept while the size stays.
    surface: Option<(IDCompositionSurface, u32, u32)>,
    /// Where the caret was last placed, relative to the top bar, and its size in pixels.
    at: Option<CaretBox>,
    /// The top bar's offset in client pixels.
    origin: (f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CaretBox {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The caret's box relative to the top bar, from the caret in client pixels.
pub(super) fn caret_box(
    caret: Option<windows_sys::Win32::Foundation::RECT>,
    origin: (f32, f32),
    scale: f32,
) -> Option<CaretBox> {
    let caret = caret?;
    let height = caret.bottom - caret.top;
    if height <= 0 {
        return None;
    }
    Some(CaretBox {
        x: (caret.left as f32 - origin.0).round() as i32,
        y: (caret.top as f32 - origin.1).round() as i32,
        width: (CARET_WIDTH * scale).round().max(1.0) as u32,
        height: height as u32,
    })
}

/// Whether the top bar must be painted again: only when what it shows changed. An edited bar is
/// kept too, since its view carries the field's generation and the caret is drawn apart.
pub(super) fn address_needs_paint(
    painted: Option<&(crate::features::address_bar::View, u32)>,
    bar: &crate::features::address_bar::View,
    dpi: u32,
) -> bool {
    painted.is_none_or(|(view, at)| view != bar || *at != dpi)
}

impl AddressCaret {
    pub(super) unsafe fn new(
        device: &IDCompositionDesktopDevice,
        parent: &IDCompositionVisual2,
    ) -> Result<Self> {
        let visual = device.CreateVisual()?;
        parent.AddVisual(&visual, true, None::<&IDCompositionVisual>)?;
        Ok(Self {
            visual,
            surface: None,
            at: None,
            origin: (0.0, 0.0),
        })
    }

    pub(super) fn set_origin(&mut self, origin: (f32, f32)) {
        self.origin = origin;
    }
}

impl AddressCaret {
    /// Places the caret, or hides it for `None`. Returns whether anything was sent to the
    /// compositor, which is nothing when the caret is where it was.
    unsafe fn place(
        &mut self,
        device: &IDCompositionDesktopDevice,
        caret: Option<windows_sys::Win32::Foundation::RECT>,
        dpi: u32,
    ) -> Result<bool> {
        let scale = dpi as f32 / 96.0;
        let wanted = caret_box(caret, self.origin, scale);
        if wanted == self.at {
            return Ok(false);
        }
        match wanted {
            None => self.visual.SetContent(None)?,
            Some(place) => {
                let fits = self
                    .surface
                    .as_ref()
                    .is_some_and(|(_, w, h)| (*w, *h) == (place.width, place.height));
                if !fits {
                    let surface = device.CreateSurface(
                        place.width,
                        place.height,
                        DXGI_FORMAT_B8G8R8A8_UNORM,
                        DXGI_ALPHA_MODE_PREMULTIPLIED,
                    )?;
                    let mut offset = POINT::default();
                    let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
                    context.Clear(Some(&rgba(INK, 1.0)));
                    surface.EndDraw()?;
                    self.surface = Some((surface, place.width, place.height));
                }
                if self.at.is_none() || !fits {
                    if let Some((surface, _, _)) = self.surface.as_ref() {
                        self.visual.SetContent(surface)?;
                    }
                }
                self.visual.SetOffsetX2(place.x as f32)?;
                self.visual.SetOffsetY2(place.y as f32)?;
            }
        }
        self.at = wanted;
        crate::features::activity::count(crate::features::activity::Counter::CaretMove);
        Ok(true)
    }

    /// Hides the caret.
    unsafe fn hide(&mut self) -> Result<()> {
        if self.at.take().is_some() {
            self.visual.SetContent(None)?;
        }
        Ok(())
    }
}

/// Which field's caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    Address,
    Command,
    Settings,
}

impl Appbar {
    fn caret_mut(&mut self, which: Which) -> &mut AddressCaret {
        match which {
            Which::Address => &mut self.address_caret,
            Which::Command => &mut self.command_caret,
            Which::Settings => &mut self.settings_caret,
        }
    }

    /// Places a field's caret, or hides it for `None`. Nothing is sent to the compositor when the
    /// caret is where it was. `commit` sends the change at once.
    pub(crate) unsafe fn place_caret(
        &mut self,
        which: Which,
        caret: Option<windows_sys::Win32::Foundation::RECT>,
        dpi: u32,
        commit: bool,
    ) -> Result<()> {
        let device = self.device.clone();
        let moved = self.caret_mut(which).place(&device, caret, dpi)?;
        if moved && commit {
            device.Commit()?;
        }
        Ok(())
    }

    /// Places the top bar's caret, or hides it for `None`.
    pub(crate) unsafe fn place_address_caret(
        &mut self,
        caret: Option<windows_sys::Win32::Foundation::RECT>,
        dpi: u32,
        commit: bool,
    ) -> Result<()> {
        self.place_caret(Which::Address, caret, dpi, commit)
    }

    /// Hides a field's caret, for a field that is not edited or not shown.
    pub(super) unsafe fn hide_caret(&mut self, which: Which) -> Result<()> {
        self.caret_mut(which).hide()
    }

    /// Hides the top bar's caret.
    pub(super) unsafe fn hide_address_caret(&mut self) -> Result<()> {
        self.hide_caret(Which::Address)
    }
}

/// Whether a card over the page must be painted again: only when what it shows changed. Its
/// field's text and selection count in the view's generation, and its caret is drawn apart.
pub(super) fn card_needs_paint<V: PartialEq>(painted: Option<&(V, u32)>, view: &V, dpi: u32) -> bool {
    painted.is_none_or(|(old, at)| old != view || *at != dpi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::RECT;

    fn editing_view(generation: u64) -> crate::features::address_bar::View {
        crate::features::address_bar::View {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 40.0,
            toggle: false,
            pane: false,
            title: String::new(),
            rest: String::new(),
            dev: false,
            url: "https://example.com/".into(),
            reserve: 0.0,
            security: crate::features::address_bar::Security::Secure,
            hover: None,
            can_back: true,
            can_forward: true,
            caption_hover: None,
            zoomed: false,
            editing: true,
            suggestions: Vec::new(),
            selected: None,
            generation,
        }
    }

    #[test]
    fn idle_edited_bar_is_painted_only_when_the_field_changed() {
        let painted = (editing_view(3), 96);
        assert!(!address_needs_paint(Some(&painted), &editing_view(3), 96));
        assert!(address_needs_paint(Some(&painted), &editing_view(4), 96));
        assert!(address_needs_paint(Some(&painted), &editing_view(3), 144));
        assert!(address_needs_paint(None, &editing_view(3), 96));
    }

    #[test]
    fn idle_command_and_settings_cards_are_painted_only_when_they_changed() {
        let painted = (("rows", 3u64), 96);
        assert!(!card_needs_paint(Some(&painted), &("rows", 3u64), 96), "a blink paints nothing");
        assert!(card_needs_paint(Some(&painted), &("rows", 4u64), 96), "typed text paints");
        assert!(card_needs_paint(Some(&painted), &("other", 3u64), 96));
        assert!(card_needs_paint(Some(&painted), &("rows", 3u64), 120));
        assert!(card_needs_paint::<(&str, u64)>(None, &("rows", 3u64), 96));
    }

    #[test]
    fn idle_caret_box_is_relative_to_the_top_bar() {
        let caret = RECT {
            left: 110,
            top: 20,
            right: 111,
            bottom: 38,
        };
        let placed = caret_box(Some(caret), (100.0, 10.0), 1.5).unwrap();
        assert_eq!(
            placed,
            CaretBox {
                x: 10,
                y: 10,
                width: 2,
                height: 18
            }
        );
        assert_eq!(
            caret_box(None, (0.0, 0.0), 1.0),
            None,
            "a hidden caret has no box"
        );
    }

    #[test]
    fn idle_caret_blink_keeps_the_same_box_so_nothing_is_redrawn() {
        let caret = RECT {
            left: 50,
            top: 5,
            right: 51,
            bottom: 25,
        };
        assert_eq!(
            caret_box(Some(caret), (0.0, 0.0), 1.0),
            caret_box(Some(caret), (0.0, 0.0), 1.0)
        );
    }
}
