//! The command bar and the link preview header, drawn on visuals of their own over the window.

use super::*;
use crate::features::command_bar::{self, CtrlT, Hit};
use crate::features::link_preview::{self, Chip};

/// The study's `.command input` at 16 DIPs, its rows at 13 and its keys and footer at 10 and 11.
const ROW_TEXT: Type = style("Manrope", 13.0, 400, 0.0);
const ROW_TEXT_ON: Type = style("Manrope", 13.0, 500, 0.0);
const PLACEHOLDER: Type = style("Manrope", 16.0, 500, 0.0);
const KEY_TEXT: Type = style("DM Mono", 10.0, 400, 0.0);
const FOOTER_TEXT: Type = style("Manrope", 11.0, 400, 0.0);
const FOOTER_ON: Type = style("Manrope", 11.0, 600, 0.0);
const PREVIEW_TITLE: Type = style("Manrope", 12.0, 600, 0.0);
const CHIP_LABEL: Type = style("Manrope", 11.0, 500, 0.0);

impl Canvas<'_> {
    /// A soft shadow outside `area` that leaves the inside untouched, since a page lies under it.
    pub(super) unsafe fn outer_shadow(
        &self,
        area: D2D_RECT_F,
        radius: f32,
        reach: f32,
    ) -> Result<()> {
        let steps = reach.max(1.0) as i32;
        for step in 1..=steps {
            let grow = step as f32;
            let fade = 1.0 - grow / (reach + 1.0);
            let ring = D2D_RECT_F {
                left: area.left - grow,
                top: area.top - grow + grow * 0.4,
                right: area.right + grow,
                bottom: area.bottom + grow + grow * 0.4,
            };
            self.ring(ring, radius + grow, rgba(0x28_1c_1e, 0.05 * fade * fade))?;
        }
        Ok(())
    }

    /// Draws the field's pixels at their place in client pixels, with `origin` the visual's offset.
    #[allow(
        clippy::too_many_arguments,
        reason = "the field's pixels come with their place, size, caret, offset and scale"
    )]
    pub(super) unsafe fn field_pixels(
        &self,
        place: windows_sys::Win32::Foundation::RECT,
        bytes: &[u8],
        pw: i32,
        ph: i32,
        caret: Option<windows_sys::Win32::Foundation::RECT>,
        origin: (f32, f32),
        dpi: u32,
    ) -> Result<()> {
        let scale = dpi as f32 / 96.0;
        let properties = windows::Win32::Graphics::Direct2D::D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: windows::Win32::Graphics::Direct2D::Common::D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        };
        let size = windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U {
            width: pw as u32,
            height: ph as u32,
        };
        let bitmap = self.context.CreateBitmap(
            size,
            Some(bytes.as_ptr().cast()),
            pw as u32 * 4,
            &properties,
        )?;
        let left = (place.left as f32 - origin.0) / scale;
        let top = (place.top as f32 - origin.1) / scale;
        let dest = rect(left, top, pw as f32 / scale, ph as f32 / scale);
        self.context.DrawBitmap(
            &bitmap,
            Some(&dest),
            1.0,
            windows::Win32::Graphics::Direct2D::D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
            None,
            None,
        );
        if let Some(caret) = caret {
            let x = (caret.left as f32 - origin.0) / scale;
            let y = (caret.top as f32 - origin.1) / scale;
            self.fill(
                rect(x, y, 1.2, (caret.bottom - caret.top) as f32 / scale),
                INK,
            )?;
        }
        Ok(())
    }
}

impl Appbar {
    pub(super) fn accent(&self) -> u32 {
        rivet_core::space_accent(
            self.spaces
                .get(self.current)
                .map_or(TINT, |space| space.tint),
        )
    }

    /// Shows the command bar: a scrim over the page and the study's `.command` card on it, with the
    /// field, the rows and the line that says what Ctrl+T opens. `None` removes it.
    pub(crate) unsafe fn update_command(
        &mut self,
        window: isize,
        bar: Option<&command_bar::View>,
        dpi: u32,
    ) -> Result<()> {
        let Some(bar) = bar else {
            if self.command_painted.take().is_some() {
                self.command.SetContent(None)?;
                return self.device.Commit();
            }
            return Ok(());
        };
        self.command.SetOffsetX2(bar.x.round())?;
        self.command.SetOffsetY2(bar.y.round())?;
        let (width, height) = (bar.width.round() as i32, bar.height.round() as i32);
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
            // The study's scrim, without the blur a composition visual cannot take from the page.
            canvas.fill_rounded(rect(0.0, 0.0, w, h), PANEL_RADIUS, rgba(0x28_20_24, 0.18))?;
            let [cx, cy, cw, ch] = command_bar::card(w, h, bar.rows.len());
            let card = rect(cx, cy, cw, ch);
            canvas.outer_shadow(card, command_bar::CARD_RADIUS, 24.0)?;
            canvas.fill_rounded(card, command_bar::CARD_RADIUS, rgba(PAPER, 1.0))?;
            canvas.ring(card, command_bar::CARD_RADIUS, rgba(0x000000, 0.06))?;
            canvas.icon(
                '\u{E721}',
                rect(cx + 18.0, cy, 16.0, command_bar::FIELD_HEIGHT),
                14.0,
                MUTED,
            )?;
            let [fx, fy, fw, fh] = command_bar::field_box([cx, cy, cw, ch]);
            let mut top = cy + command_bar::FIELD_HEIGHT;
            if !bar.rows.is_empty() {
                canvas.fill(rect(cx, top, cw, 1.0), HAIRLINE)?;
                let list_top = top + command_bar::LIST_PAD;
                for (index, (glyph, title, key)) in bar.rows.iter().enumerate() {
                    let row = rect(
                        cx + command_bar::LIST_PAD,
                        list_top + index as f32 * command_bar::ROW_HEIGHT,
                        cw - 2.0 * command_bar::LIST_PAD,
                        command_bar::ROW_HEIGHT,
                    );
                    let on = index == bar.selected;
                    if on {
                        canvas.fill_rounded(row, 9.0, rgba(accent, 0.09))?;
                    }
                    let ink = if on { INK } else { SOFT_INK };
                    canvas.icon(
                        *glyph,
                        rect(row.left + 12.0, row.top, 16.0, command_bar::ROW_HEIGHT),
                        14.0,
                        ink,
                    )?;
                    let key_w = if key.is_empty() {
                        0.0
                    } else {
                        measure(&self.dwrite, &self.fonts, KEY_TEXT, key)?
                    };
                    let room =
                        (row.right - row.left - 12.0 - 16.0 - 12.0 - 12.0 - key_w - 12.0).max(0.0);
                    let style = if on { ROW_TEXT_ON } else { ROW_TEXT };
                    let shown = elide(&self.dwrite, &self.fonts, style, title, room)?;
                    canvas.text(
                        style,
                        rect(row.left + 40.0, row.top, room, command_bar::ROW_HEIGHT),
                        &shown,
                        ink,
                        false,
                    )?;
                    if !key.is_empty() {
                        canvas.text(
                            KEY_TEXT,
                            rect(
                                row.right - 12.0 - key_w,
                                row.top,
                                key_w,
                                command_bar::ROW_HEIGHT,
                            ),
                            key,
                            MUTED,
                            false,
                        )?;
                    }
                }
                top +=
                    command_bar::LIST_PAD * 2.0 + command_bar::ROW_HEIGHT * bar.rows.len() as f32;
            }
            // The footer: "Ctrl T opens" and the two choices, the chosen one dark.
            canvas.fill(rect(cx, top, cw, 1.0), HAIRLINE)?;
            canvas.text(
                FOOTER_TEXT,
                rect(cx + 16.0, top, 66.0, command_bar::FOOTER_HEIGHT),
                "Ctrl T opens",
                MUTED,
                false,
            )?;
            for (choice, x, bw) in command_bar::mode_buttons() {
                let button = rect(
                    cx + x,
                    top + (command_bar::FOOTER_HEIGHT - 22.0) / 2.0,
                    bw,
                    22.0,
                );
                let label = match choice {
                    CtrlT::CommandBar => "Command Bar",
                    CtrlT::NewTabPage => "New tab page",
                };
                if bar.ctrl_t == choice {
                    canvas.fill_rounded(button, 7.0, rgba(INK, 1.0))?;
                    canvas.text_centred(FOOTER_ON, button, label, WHITE)?;
                } else {
                    let hot = bar.hover == Some(Hit::Mode(choice));
                    if hot {
                        canvas.fill_rounded(button, 7.0, rgba(INK, 0.05))?;
                    }
                    canvas.ring(button, 7.0, rgba(HAIRLINE, 1.0))?;
                    canvas.text_centred(FOOTER_TEXT, button, label, MUTED)?;
                }
            }
            let origin = (bar.x, bar.y);
            let drawn =
                command_bar::with_field_pixels(window as _, |place, bytes, pw, ph, caret| {
                    canvas.field_pixels(place, bytes, pw, ph, caret, origin, dpi)
                });
            if let Some(result) = drawn {
                result?;
            }
            if bar.empty {
                canvas.text(
                    PLACEHOLDER,
                    rect(fx + 1.0, fy, fw, fh),
                    "Search, enter an address, or ask Rivet",
                    0xa09ea7,
                    false,
                )?;
            }
            Ok(())
        };
        let result = draw();
        surface.EndDraw()?;
        result?;
        self.command.SetContent(&surface)?;
        self.command_painted = Some((bar.clone(), dpi));
        self.device.Commit()
    }

    /// Shows the header and shadow of a link preview around the floating page, which the browser
    /// draws itself. `None` removes them.
    pub(crate) unsafe fn update_preview(
        &mut self,
        preview: Option<&link_preview::View>,
        dpi: u32,
    ) -> Result<()> {
        let Some(preview) = preview else {
            if self.preview_painted.take().is_some() {
                self.preview.SetContent(None)?;
                return self.device.Commit();
            }
            return Ok(());
        };
        let scale = dpi as f32 / 96.0;
        const REACH: f32 = 28.0;
        let pad = (REACH * scale).ceil();
        self.preview.SetOffsetX2((preview.x - pad).round())?;
        self.preview.SetOffsetY2((preview.y - pad).round())?;
        if self.preview_painted.as_ref() == Some(&(preview.clone(), dpi)) {
            return self.device.Commit();
        }
        let width = (preview.width + 2.0 * pad).round() as i32;
        let height = (preview.header + preview.page + 2.0 * pad).round() as i32;
        if width <= 0 || height <= 0 || preview.width <= 0.0 {
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
        let (w, header, page) = (
            preview.width / scale,
            preview.header / scale,
            preview.page / scale,
        );
        let draw = || -> Result<()> {
            let whole = rect(p, p, w, header + page);
            canvas.outer_shadow(whole, 14.0, REACH - 4.0)?;
            // The header: rounded top corners, square where the page meets it, then the hairline.
            canvas.fill_rounded(rect(p, p, w, header), 14.0, rgba(PAPER, 1.0))?;
            canvas.fill(rect(p, p + header / 2.0, w, header / 2.0), PAPER)?;
            canvas.fill(rect(p, p + header - 1.0, w, 1.0), HAIRLINE)?;
            canvas.ring(
                rect(p - 1.0, p - 1.0, w + 2.0, header + page + 2.0),
                15.0,
                rgba(0x000000, 0.06),
            )?;
            let chips = link_preview::chips(w);
            let room = (chips[0].1 - 12.0 - 8.0).max(0.0);
            let title = elide(
                &self.dwrite,
                &self.fonts,
                PREVIEW_TITLE,
                &preview.heading,
                room,
            )?;
            canvas.text(
                PREVIEW_TITLE,
                rect(p + 12.0, p, room, header),
                &title,
                SOFT_INK,
                false,
            )?;
            for (chip, x, cw) in chips {
                let area = rect(
                    p + x,
                    p + (header - link_preview::CHIP_HEIGHT) / 2.0,
                    cw,
                    link_preview::CHIP_HEIGHT,
                );
                let alpha = if preview.hover == Some(chip) {
                    0.09
                } else {
                    0.05
                };
                canvas.fill_rounded(area, 7.0, rgba(INK, alpha))?;
                canvas.text_centred(CHIP_LABEL, area, Chip::label(chip), SOFT_INK)?;
            }
            Ok(())
        };
        let result = draw();
        surface.EndDraw()?;
        result?;
        self.preview.SetContent(&surface)?;
        self.preview_painted = Some((preview.clone(), dpi));
        self.device.Commit()
    }
}
