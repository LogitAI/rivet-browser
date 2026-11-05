//! The Rivet settings card, drawn on a visual of its own over the window in the study's style: the
//! command bar's paper card and scrim, the link rules panel's serif title and hairline rows, and
//! the command bar's dark chosen button.

use super::*;
use crate::features::settings::{self, Hit, Kind, Row};

const TITLE: Type = style("Newsreader", 18.0, 500, 0.0);
const SUBTITLE: Type = style("Manrope", 12.0, 400, 0.0);
const LABEL: Type = style("Manrope", 13.0, 400, 0.0);
const LABEL_ON: Type = style("Manrope", 13.0, 600, 0.0);
const NOTE: Type = style("Manrope", 11.0, 400, 0.0);
const VALUE: Type = style("Manrope", 12.0, 500, 0.0);
const BUTTON: Type = style("Manrope", 11.0, 400, 0.0);
const BUTTON_ON: Type = style("Manrope", 11.0, 600, 0.0);
const FOOTER: Type = style("Manrope", 11.0, 400, 0.0);
const WARNING: u32 = 0xb3412e;

fn area([x, y, w, h]: [f32; 4]) -> D2D_RECT_F {
    rect(x, y, w, h)
}

impl Appbar {
    /// Shows the Rivet settings card over the page. `None` removes it.
    pub(crate) unsafe fn update_settings(
        &mut self,
        window: isize,
        card: Option<&settings::View>,
        dpi: u32,
    ) -> Result<()> {
        let Some(view) = card else {
            if self.settings_painted.take().is_some() {
                self.settings.SetContent(None)?;
                return self.device.Commit();
            }
            return Ok(());
        };
        self.settings.SetOffsetX2(view.x.round())?;
        self.settings.SetOffsetY2(view.y.round())?;
        let (width, height) = (view.width.round() as i32, view.height.round() as i32);
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        let scale = dpi as f32 / 96.0;
        let surface = self.device.CreateSurface(
            width as u32,
            height as u32,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_ALPHA_MODE_PREMULTIPLIED,
        )?;
        let mut offset = POINT::default();
        let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
        context.SetDpi(dpi as f32, dpi as f32);
        context.SetTransform(&Matrix3x2::translation(
            offset.x as f32 / scale,
            offset.y as f32 / scale,
        ));
        context.Clear(Some(&rgba(WHITE, 0.0)));
        let canvas = Canvas {
            context: &context,
            dwrite: &self.dwrite,
            fonts: &self.fonts,
        };
        let accent = self.accent();
        let (w, h) = (width as f32 / scale, height as f32 / scale);
        let draw = || -> Result<()> {
            canvas.fill_rounded(rect(0.0, 0.0, w, h), PANEL_RADIUS, rgba(0x28_20_24, 0.18))?;
            let c = settings::card(w, h);
            let [cx, cy, cw, ch] = c;
            let whole = rect(cx, cy, cw, ch);
            canvas.outer_shadow(whole, settings::CARD_RADIUS, 24.0)?;
            canvas.fill_rounded(whole, settings::CARD_RADIUS, rgba(PAPER, 1.0))?;
            canvas.ring(whole, settings::CARD_RADIUS, rgba(0x000000, 0.06))?;
            // The head: the serif title, what the card is, and the close button.
            canvas.text(
                TITLE,
                rect(cx + settings::PAD, cy + 12.0, cw - 80.0, 26.0),
                "Rivet settings",
                INK,
                false,
            )?;
            canvas.text(
                SUBTITLE,
                rect(cx + settings::PAD, cy + 36.0, cw - 80.0, 18.0),
                "Kept on this computer for this browser profile",
                MUTED,
                false,
            )?;
            let close = area(settings::close_box(c));
            if view.hover == Some(Hit::Close) {
                canvas.fill_rounded(close, 8.0, rgba(INK, 0.05))?;
            }
            canvas.icon('\u{E711}', close, 11.0, SOFT_INK)?;
            for (row, top) in settings::row_tops() {
                if let Some(heading) = row.heading() {
                    let y = cy + top - settings::HEADING_HEIGHT;
                    canvas.fill(
                        rect(cx + settings::PAD, y + 6.0, cw - 2.0 * settings::PAD, 1.0),
                        HAIRLINE,
                    )?;
                    canvas.text(
                        GROUP_TITLE,
                        rect(cx + settings::PAD, y + 8.0, cw - 2.0 * settings::PAD, 20.0),
                        heading,
                        MUTED,
                        false,
                    )?;
                }
                self.settings_row(&canvas, view, c, row, accent)?;
            }
            // The footer: the keys, or why typed text was not taken.
            let top = cy + ch - settings::FOOTER_HEIGHT;
            canvas.fill(rect(cx, top, cw, 1.0), HAIRLINE)?;
            let (line, ink) = match view.notice {
                Some(notice) => (notice, WARNING),
                None if view.editing.is_some() => {
                    ("Enter keeps the text \u{b7} Esc drops it", MUTED)
                }
                None => (
                    "\u{2191} \u{2193} move \u{b7} \u{2190} \u{2192} change \u{b7} Esc closes",
                    MUTED,
                ),
            };
            canvas.text(
                FOOTER,
                rect(
                    cx + settings::PAD,
                    top,
                    cw - 2.0 * settings::PAD,
                    settings::FOOTER_HEIGHT,
                ),
                line,
                ink,
                false,
            )?;
            let origin = (view.x, view.y);
            let drawn = settings::with_field_pixels(window as _, |place, bytes, pw, ph, caret| {
                canvas.field_pixels(place, bytes, pw, ph, caret, origin, dpi)
            });
            if let Some(result) = drawn {
                result?;
            }
            Ok(())
        };
        let result = draw();
        surface.EndDraw()?;
        result?;
        self.settings.SetContent(&surface)?;
        self.settings_painted = Some((view.clone(), dpi));
        self.device.Commit()
    }

    /// One row: its label, a note under it if it has one, and its control at the right end.
    unsafe fn settings_row(
        &self,
        canvas: &Canvas,
        view: &settings::View,
        c: [f32; 4],
        row: Row,
        accent: u32,
    ) -> Result<()> {
        let values = &view.values;
        let enabled = settings::enabled(values, view.space, row);
        let [rx, ry, rw, rh] = settings::row_box(c, row);
        let focused = view.focus == row;
        if focused {
            canvas.fill_rounded(rect(rx, ry, rw, rh), 9.0, rgba(accent, 0.09))?;
        }
        let ink = if !enabled {
            MUTED
        } else if focused {
            INK
        } else {
            SOFT_INK
        };
        let label_x = c[0] + settings::PAD;
        let [vx, vy, vw, vh] = settings::value_box(c, row);
        let room = (vx - label_x - 12.0).max(0.0);
        match row.note() {
            Some(note) => {
                canvas.text(
                    if focused { LABEL_ON } else { LABEL },
                    rect(label_x, ry + 6.0, room, 20.0),
                    row.label(),
                    ink,
                    false,
                )?;
                canvas.text(
                    NOTE,
                    rect(label_x, ry + 25.0, room, 18.0),
                    note,
                    MUTED,
                    false,
                )?;
            }
            None => canvas.text(
                if focused { LABEL_ON } else { LABEL },
                rect(label_x, ry, room, rh),
                row.label(),
                ink,
                false,
            )?,
        }
        let value = rect(vx, vy, vw, vh);
        match row.kind() {
            Kind::Choice => {
                let chosen = settings::chosen(values, row);
                let half = (vw - 6.0) / 2.0;
                for (index, label) in settings::choices(row).into_iter().enumerate() {
                    let button = rect(vx + index as f32 * (half + 6.0), vy, half, vh);
                    if chosen == Some(index) {
                        canvas.fill_rounded(button, 7.0, rgba(INK, 1.0))?;
                        canvas.text_centred(BUTTON_ON, button, label, WHITE)?;
                    } else {
                        if view.hover == Some(Hit::Choice(row, index)) {
                            canvas.fill_rounded(button, 7.0, rgba(INK, 0.05))?;
                        }
                        canvas.ring(button, 7.0, rgba(HAIRLINE, 1.0))?;
                        canvas.text_centred(BUTTON, button, label, MUTED)?;
                    }
                }
            }
            Kind::Stepper => {
                canvas.ring(value, 7.0, rgba(HAIRLINE, 1.0))?;
                let text = settings::shown(values, view.space, row);
                let arrow = settings::ARROW_WIDTH;
                if enabled {
                    for (forward, x, glyph) in
                        [(false, vx, '\u{E76B}'), (true, vx + vw - arrow, '\u{E76C}')]
                    {
                        let button = rect(x, vy, arrow, vh);
                        if view.hover == Some(Hit::Step(row, forward)) {
                            canvas.fill_rounded(button, 7.0, rgba(INK, 0.05))?;
                        }
                        canvas.icon(glyph, button, 10.0, SOFT_INK)?;
                    }
                }
                let middle = rect(vx + arrow, vy, (vw - 2.0 * arrow).max(0.0), vh);
                let shown = elide(
                    &self.dwrite,
                    &self.fonts,
                    VALUE,
                    &text,
                    middle.right - middle.left - 24.0,
                )?;
                if row == Row::SpaceColour {
                    if let Some(info) = values.spaces.get(view.space) {
                        let dot_x = middle.left + 6.0;
                        canvas.fill_rounded(
                            rect(dot_x, vy + vh / 2.0 - 5.0, 10.0, 10.0),
                            5.0,
                            rgba(rivet_core::space_accent(info.tint), 1.0),
                        )?;
                    }
                }
                canvas.text_centred(VALUE, middle, &shown, if enabled { INK } else { MUTED })?;
            }
            Kind::Text => {
                let editing = view.editing == Some(row);
                let ring = if editing {
                    rgba(accent, 0.9)
                } else {
                    rgba(HAIRLINE, 1.0)
                };
                canvas.ring(value, 7.0, ring)?;
                if !editing {
                    let text = settings::shown(values, view.space, row);
                    let [tx, ty, tw, th] = settings::text_box(c, row);
                    let shown = elide(&self.dwrite, &self.fonts, VALUE, &text, tw)?;
                    canvas.text(
                        VALUE,
                        rect(tx, ty, tw, th),
                        &shown,
                        if enabled { INK } else { MUTED },
                        false,
                    )?;
                }
            }
        }
        Ok(())
    }
}
