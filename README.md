# Rivet Browser

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**English** · [한국어](docs/locales/README_ko.md)

**Keep your browser. Replace its frame.**

Rivet Browser is a Windows-first, open-source browser frame for an installed Chromium browser.
It keeps the browser engine, profile, sign-in, extensions, media stack and updates in the browser
the user already chose. Rivet supplies the frame, workspaces and agent control surface.

Rivet is not a Chromium fork and it is not a webview browser.

## Status

**Design stage. There is no runnable implementation yet.** The repository starts with the
architecture, requirements and delivery rules. Rust implementation begins when the relevant
requirements are agreed.

## What Rivet is meant to do

- Start and directly integrate a user's installed Chromium browser on Windows.
- Replace the visible browser frame with Rivet's native frame while retaining the real browser
  process and its browser-owned features.
- Offer the same browser control surface to people and agents through a CLI.
- Let a caller select presentation: visible for visual inspection and UI automation, or hidden
  when browser processing alone is enough.
- Let Ember coordinate a user's Rivet settings and logical workspaces as the user moves between
  registered desktop clients.

Hidden presentation does not mean that a page has no renderer. JavaScript, layout and navigation
still run in the browser. It means Rivet does not create a visible composited browser surface or
capture frames unless the caller asks for one.

## Repository

- `rivet/`: all Rivet code and packaging inputs. Rust is the implementation language.
- `docs/locales/`: translations of this README.
- `scripts/tests/`: repository checks.

Work happens on `develop`. Publishing is one way: `develop` to `release` to `main` through a
pull request. Internal planning documents stay on `develop`.

## License

[Apache-2.0](LICENSE)
