//! The link rules panel, the study's `#rules` card, drawn on a visual of its own over the page.

use super::*;
use crate::features::link_rules::{self as panel, Hit};

/// The study's `#rules h3` in Newsreader at 18, its note at 12, patterns in DM Mono at 11 and
/// targets at 12.
const TITLE: Type = style("Newsreader", 18.0, 500, 0.0);
const NOTE: Type = style("Manrope", 12.0, 400, 0.0);
const PATTERN: Type = style("DM Mono", 11.0, 400, 0.0);
const TARGET: Type = style("Manrope", 12.0, 400, 0.0);
const TARGET_ON: Type = style("Manrope", 12.0, 600, 0.0);
const BUTTON: Type = style("Manrope", 11.0, 500, 0.0);

impl Appbar {
    /// Shows the link rules panel at its place over the page. `None` removes it.
    pub(crate) unsafe fn update_link_rules(
        &mut self,
        view: Option<&panel::View>,
        dpi: u32,
    ) -> Result<()> {
        let Some(view) = view else {
            if self.rules_painted.take().is_some() {
                self.rules.SetContent(None)?;
                return self.device.Commit();
            }
            return Ok(());
        };
        let scale = dpi as f32 / 96.0;
        const REACH: f32 = 24.0;
        let pad = (REACH * scale).ceil();
        self.rules.SetOffsetX2((view.x - pad).round())?;
        self.rules.SetOffsetY2((view.y - pad).round())?;
        if self.rules_painted.as_ref() == Some(&(view.clone(), dpi)) {
            return self.device.Commit();
        }
        let width = (view.width + 2.0 * pad).round() as i32;
        let height = (view.height + 2.0 * pad).round() as i32;
        if width <= 0 || height <= 0 || view.width <= 0.0 {
            return Ok(());
        }
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
        let p = pad / scale;
        let (w, h) = (view.width / scale, view.height / scale);
        let accent = self.accent();
        let draw = || -> Result<()> {
            let card = rect(p, p, w, h);
            canvas.outer_shadow(card, panel::RADIUS, REACH - 4.0)?;
            canvas.fill_rounded(card, panel::RADIUS, rgba(PAPER, 1.0))?;
            canvas.ring(card, panel::RADIUS, rgba(0x000000, 0.06))?;
            let left = p + panel::PAD;
            let inner = w - 2.0 * panel::PAD;
            canvas.text(
                TITLE,
                rect(left, p + panel::PAD, inner, panel::TITLE_HEIGHT),
                "Link Rules",
                INK,
                false,
            )?;
            let [bx, by, bw, bh] = panel::close_box(w);
            let close = rect(p + bx, p + by, bw, bh);
            if view.hover == Some(Hit::Close) {
                canvas.fill_rounded(close, 6.0, rgba(INK, 0.06))?;
            }
            canvas.icon('\u{E8BB}', close, 9.0, MUTED)?;
            canvas.text(
                NOTE,
                rect(
                    left,
                    p + panel::PAD + panel::TITLE_HEIGHT,
                    inner,
                    panel::NOTE_HEIGHT,
                ),
                "Where links from other apps and sites open.",
                MUTED,
                false,
            )?;
            let everything = (
                "Everything else".to_owned(),
                "Current space, new tab".to_owned(),
            );
            let rows = view.rows.iter().chain(std::iter::once(&everything));
            for (index, (pattern, target)) in rows.enumerate() {
                let top = p + panel::row_top(index);
                canvas.fill(rect(left, top, inner, 1.0), HAIRLINE)?;
                let fixed = index == view.rows.len();
                let target_hot = view.hover == Some(Hit::Target(index));
                let remove_hot = view.hover == Some(Hit::Remove(index));
                let half = inner / 2.0 - 6.0;
                let shown = elide(&self.dwrite, &self.fonts, PATTERN, pattern, half)?;
                canvas.text(
                    PATTERN,
                    rect(left, top, half, panel::ROW_HEIGHT),
                    &shown,
                    INK,
                    false,
                )?;
                // Each rule's target sits left of the glyph that removes the rule. The fixed last
                // row has neither and its target reaches the card's padding.
                let end = if fixed {
                    left + inner
                } else {
                    left + inner - panel::REMOVE - 6.0
                };
                let room = (end - (left + inner / 2.0)).max(0.0);
                let style = if target_hot { TARGET_ON } else { TARGET };
                let label = elide(&self.dwrite, &self.fonts, style, target, room)?;
                if target_hot {
                    let label_w = measure(&self.dwrite, &self.fonts, style, &label)?;
                    let chip = rect(
                        end - label_w - 10.0,
                        top + 6.0,
                        label_w + 14.0,
                        panel::ROW_HEIGHT - 12.0,
                    );
                    canvas.fill_rounded(chip, 7.0, rgba(accent, 0.09))?;
                }
                canvas.text(
                    style,
                    rect(end - room, top, room, panel::ROW_HEIGHT),
                    &label,
                    if target_hot { INK } else { SOFT_INK },
                    true,
                )?;
                if !fixed {
                    let glyph = rect(
                        left + inner - panel::REMOVE,
                        top + (panel::ROW_HEIGHT - panel::REMOVE) / 2.0,
                        panel::REMOVE,
                        panel::REMOVE,
                    );
                    if remove_hot {
                        canvas.fill_rounded(glyph, 6.0, rgba(INK, 0.06))?;
                    }
                    canvas.icon('\u{E711}', glyph, 8.0, if remove_hot { INK } else { MUTED })?;
                }
            }
            let [ax, ay, aw, ah] = panel::add_box(view.rows.len());
            let add = rect(p + ax, p + ay, aw, ah);
            let alpha = if view.hover == Some(Hit::AddSite) {
                0.09
            } else {
                0.05
            };
            canvas.fill_rounded(add, 7.0, rgba(INK, alpha))?;
            canvas.text_centred(BUTTON, add, "Add this site", SOFT_INK)?;
            Ok(())
        };
        let result = draw();
        surface.EndDraw()?;
        result?;
        self.rules.SetContent(&surface)?;
        self.rules_painted = Some((view.clone(), dpi));
        self.device.Commit()
    }
}
