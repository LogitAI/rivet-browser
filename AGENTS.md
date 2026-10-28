# AGENTS.md

Guidance for working in this repository.

## Project

Rivet Browser directly integrates a native Rivet frame and agent control surface into an installed
Chromium browser. Windows comes first. Rivet does not fork Chromium and does not use a webview for
page rendering.

- `rivet/`: all code and packaging inputs. Rust is the implementation language.
  - `rivet/protocol/`: the single definition of local and Ember-facing control messages.
  - `rivet/core/`: platform-neutral sessions, workspaces, display policy and lifecycle.
  - `rivet/runtime/windows/`: the in-browser Windows runtime and native frame integration.
  - `rivet/platform/windows/`: browser launch, discovery, installation and Windows integration.
  - `rivet/cli/`: the `rivet` command for people and agents.
  - `rivet/ember/`: Ember settings, workspace and remote-browser integration.
  - `rivet/installer/windows/`: Rivet adaptation of DSA's WiX installation tool.
- `docs/INTENT.md`: decisions and rejected alternatives.
- `docs/RESEARCH.md`: investigations and their evidence.
- `docs/SPEC.md`: requirements and acceptance criteria.
- `PROJECT.md`: scope, milestones and open questions.

Folders are created by the first real implementation change. Do not add placeholders that pretend a
feature exists.

## Spec Driven Development

1. **SPEC is the source of truth.** Before implementing behaviour, find its SPEC ID. If none
   exists, stop and ask for one. A new requirement lands before its code.
2. **Never edit `docs/SPEC.md`, `docs/INTENT.md` or `PROJECT.md` without being asked to.** Propose
   the change and wait. This does not apply when the owner expressly asks to initialize or revise
   those documents.
3. If code and SPEC disagree, fix the code. If the SPEC is wrong, ask before changing it.
4. Decision changes go to `docs/INTENT.md`, then SPEC, then code. The owner makes decisions.
5. When acceptance criteria are verified, update that requirement's status in the same change.
6. Resolved open questions leave `PROJECT.md` only after the owner has answered them and the answer
   is recorded as a decision and requirement.

## Direct integration boundaries

- Use an installed Chromium browser. Do not add a Chromium fork, CEF, WebView2 or another webview
  as the page-rendering path.
- The Windows runtime may integrate into the browser process. Treat every supported browser version
  as a compatibility target and add a regression check before claiming support.
- Do not copy, export, synchronize or transmit browser passwords, passkeys, cookies, encryption
  keys or profile databases. Browser-owned authentication stays browser-owned.
- Every Rivet UI component is part of the browser window's surface, like the appbar: a
  DirectComposition visual of that window, with input taken from the subclassed browser window.
  Never add a separate top-level window, owned popup or child window for Rivet UI.
- `hidden` means no visible Rivet surface or frame stream. It does not promise that the browser's
  renderer, layout engine or JavaScript engine is absent.
- The CLI is a first-class public surface. It must use the same session and policy model as the
  native frame.
- Ember owns the shared user settings and logical workspace record. It does not become a transport
  for raw browser profiles.

## Branches

`develop` is where work happens. `release` and `main` are produced from it by
`.github/scripts/release/sync-release.sh`, which strips `PROJECT.md`, `AGENTS.md`, `CLAUDE.md` if
present, and every file directly under `docs/`.

1. **Publishing is one way: develop to release to main.** Never merge `release` or `main` back into
   `develop`.
2. **Start every branch and worktree from `develop`.** Check the latest commit before editing.
3. `scripts/tests/planning-docs.test.sh` protects the planning documents.
4. **Every worktree builds into its own build directories.** Do not share `CARGO_TARGET_DIR`.
5. Worktrees go in `.claude/worktrees/<name>/`. Throwaway work goes in `.scratch/<name>/`.
6. **One worktree per agent, and never two workers in the same checkout.**
7. Work branches are `feat/<topic>`.
8. Merge with `gh pr merge --delete-branch`, then remove its local branch and worktree.
9. Only `main`, `develop` and `release` stay on the remote.

## Where files go

1. Everything this project makes stays inside this repository.
2. Code and packaging inputs belong under `rivet/`. Scratch work and downloads belong under
   `.scratch/`. Build outputs belong in `target/`, `build/` or `out/`.
3. The DSA installer is reused by adapting its source into `rivet/installer/windows/`. Do not add
   an external installer dependency without the owner's approval.
4. Tool caches stay with their tools. A new project dependency or tool needs approval before it is
   installed or downloaded.

## Repository root

Nothing new goes in the root without owner approval. The approved entries are `.github/`, `docs/`,
`rivet/`, `scripts/`, `.gitignore`, `AGENTS.md`, `CHANGELOG.md`, `LICENSE`, `PROJECT.md` and
`README.md`.

## Writing

The house style is `docs/style/writing.md` in `thisisthepy/pythonx-compose` on its `develop`
branch. Use short plain sentences. README files are English with Korean translations in
`docs/locales/`. Planning documents are Korean. Code and code comments are English.

- Never use an em dash.
- README files never link planning documents because published branches do not contain them.
- Never cite SPEC or INTENT from production code. Explain the reason in the code itself.
- Python examples use uv, ppp and tcl only. Never use `pip install`, `python -m pip` or bare
  `python3 script.py`.

## Test Driven Development

1. Write the failing test from the SPEC acceptance criterion first.
2. Name tests after their requirement, for example `fr5_hidden_session_has_no_frame_stream`.
3. No production behaviour changes without a test that would have caught their absence.
4. Test and implementation land in the same commit.
5. Test through public surfaces: protocol, CLI, local control channel and Ember contract.
6. Use protocol vectors and in-memory transports, not mocks of Rivet's own protocol.
7. Performance claims include measurements and the environment.

## Commits and verification

- Make one logical change per commit. Commit only when asked.
- Commit subjects are `<Type>: <imperative summary>`.
- Do not add AI attribution trailers.
- Report actual verification output. Do not mark requirements done without evidence.
- Manual Windows checks run on installed builds and record the browser, Rivet and Windows versions.
