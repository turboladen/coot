# Releasing billz

How to cut a release: build the signed macOS `.dmg` locally and publish it to
GitHub Releases. Written to be followed cold — you should not have to remember
anything. CI (`.github/workflows/ci.yml`) only runs `just verify` on Linux for
cost; it does **not** build the macOS bundle, so releases are a local step.

> **Why local, not CI?** The `.dmg` needs macOS, and GitHub bills macOS runners
> at 10× the Linux rate on private repos. It also needs your local `billz Local
> Signing` Keychain identity, which a CI runner doesn't have. Building on your Mac
> and uploading with `gh` is cheaper and simpler.

## One-time setup

You only do these once per machine.

1. **Code-signing identity** (kills the Keychain re-prompt on the built app):
   ```fish
   just setup-signing   # creates the "billz Local Signing" identity; see SIGNING.md
   ```
2. **GitHub CLI**, authenticated (used to create the release):
   ```fish
   gh auth status       # should show you logged in to github.com
   ```

## Cutting a release

Assume you're releasing version `X.Y.Z` (e.g. `0.1.0`). Work on a clean `main`.

### 1. Pick the version

The version lives in **two** files and they must match:
- `app/tauri.conf.json` → `"version"`
- `app/ui/package.json` → `"version"`

Bump both if this isn't the version already there. (For the very first release,
`0.1.0` is already set in both.)

### 2. Update the changelog

Add a section to `CHANGELOG.md` for `X.Y.Z` with the date and a human summary of
what changed. Source material:
```fish
# commits since the last tag (or all commits for the first release)
git log --pretty=format:'%s' PREVIOUS_TAG..HEAD | grep -iE '^(feat|fix|perf)'
```
Update the link reference at the bottom of `CHANGELOG.md` to point at the new tag.

### 3. Green gate

```fish
just verify        # fmt + clippy + Rust tests + svelte-check + ui-test + ui-build
just audit         # cargo-deny: advisories + licenses (optional but recommended)
```
Do not release on a red gate.

### 4. Commit + tag

```fish
git add -A
git commit -m "release: vX.Y.Z"
git tag vX.Y.Z
git push origin main --tags
```

### 5. Build the signed DMG

```fish
just app-build     # = cd app/ui && bun run tauri build
```
Output lands in the **workspace root** `target/` (this is a Cargo workspace, so
bundles do NOT go under `app/`):
- App: `target/release/bundle/macos/billz.app`
- DMG: `target/release/bundle/dmg/billz_X.Y.Z_aarch64.dmg`

> On the first-ever build, macOS `codesign` may pop one "wants to sign using key…"
> prompt — click **Always Allow** and it won't ask again.

> **Architecture:** this builds for the machine's arch. Your Mac is Apple Silicon,
> so the DMG is **arm64 (Apple Silicon only)** — it will not run on Intel Macs. If
> you ever need to hand it to an Intel user, build a universal binary instead
> (`tauri build --target universal-apple-darwin`, after adding the x86_64 Rust
> target); that's a deliberate future step, not the default.

### 6. Smoke-test the DMG locally

Mount it, drag `billz.app` to `/Applications`, launch it, connect once, and run a
query. Check each of these — they're the things that only break in the *packaged*
build, not in `just dev`:
- **The SQL editor AND the results grid render with proper styling** — editor:
  syntax colors, cursor, gutter; grid: correct column widths and smooth row
  virtualization. Both rely on runtime-injected inline styles (CodeMirror's
  `<style>` elements; the grid's inline `style=""` column widths and TanStack Virtual
  `transform` offsets), and the Content-Security-Policy set in `app/tauri.conf.json`
  (`app.security.csp`) only takes effect in the packaged app. If either looks
  broken — unstyled editor, or collapsed/mis-sized grid columns — the CSP's
  `style-src` is blocking those runtime styles (Tauri's style nonce can override
  `'unsafe-inline'`); fix by feeding Tauri's nonce into CodeMirror's `cspNonce` facet
  and the grid's inline styles, or loosen `style-src`.
- **Fonts load** (IBM Plex Sans/Mono, not a system fallback) — that exercises
  `font-src`.
- **Queries return results** — that exercises `connect-src` / the IPC path.
- The Keychain does **not** re-prompt on every query (signing identity working).

(Locally the app isn't quarantined, so you won't hit the Gatekeeper issue below —
but a downloader will, which is why the release notes must document it.)

### 7. Publish the GitHub Release

```fish
set -l DMG (ls target/release/bundle/dmg/billz_*_aarch64.dmg)
gh release create vX.Y.Z "$DMG" \
    --title "billz vX.Y.Z" \
    --notes-file RELEASE_NOTES.md
```
Write `RELEASE_NOTES.md` from the changelog section for this version, and **always
include the install instructions below** (delete `RELEASE_NOTES.md` after, or keep
it untracked). Alternatively use `--notes "…"` inline for short releases.

## Install instructions to include in every release's notes

The app is signed with a **local self-signed identity and is NOT Apple-notarized.**
When someone downloads the `.dmg` from GitHub, macOS quarantines it, and Gatekeeper
will refuse to open the app — showing the misleading message **"billz is damaged
and can't be opened."** It is not damaged; macOS just doesn't trust a self-signed
build it didn't notarize. The recipient must strip the quarantine flag once:

    # 1. Open the .dmg and drag billz.app to /Applications, then run:
    xattr -dr com.apple.quarantine /Applications/billz.app
    # 2. Launch billz normally. On first connect, click "Always Allow" on the
    #    Keychain prompt (expected and safe — passwords live in the macOS Keychain,
    #    never on disk).

Also worth stating in the notes:
- **Apple Silicon only** (arm64). Intel Macs are not supported by this build.
- **SQL-auth only** (no Entra/AAD/Windows auth).

> **If the "damaged" workaround ever becomes a support burden**, the fix is an
> Apple Developer ID ($99/yr) + hardened runtime + notarization + stapling, which
> makes the DMG open on a normal double-click. Deliberately skipped for now — this
> is a personal tool handed to a few trusted people.

## After releasing

Nothing required. Optionally bump the version in the two files to the next
`-dev`/patch so `main` isn't confused with the released build.
