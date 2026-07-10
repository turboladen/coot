# macOS code signing — killing the Keychain prompt

`billz` stores connection passwords in the macOS Keychain. macOS only lets an app
read a Keychain item without prompting if the app has a **stable code-signing
identity** that matches the item's access-control list. Two things follow:

- **`bun run tauri dev`** runs an *unsigned* debug binary that is rebuilt every
  time, so macOS can never remember an "Always Allow" for it. The in-session
  password cache (added in the `CachingSecretStore` change) means dev prompts **at
  most once per launch**, not once per query — but it can't get to zero.
- **A signed release build** has a consistent identity, so you click **"Always
  Allow" once** and are never asked again — across relaunches *and* future rebuilds.

This is set up to be turnkey.

## One-time setup

```fish
# 1. Create a stable self-signed code-signing identity in your login keychain.
#    Idempotent; prompts once for your login password (macOS trusting the cert).
./scripts/setup-macos-signing.sh

# 2. Build the signed app.
cd app/ui && bun run tauri build

# 3. Launch the built app and click "Always Allow" on the single Keychain prompt:
open target/release/bundle/macos/billz.app
#    (this is a Cargo workspace, so bundles land in the shared root `target/`,
#    NOT app/src-tauri/target/. The .dmg is alongside in target/release/bundle/dmg/.)
```

That's it. `app/tauri.conf.json` is already wired to sign with the identity
(`bundle.macOS.signingIdentity = "billz Local Signing"`), so every future
`bun run tauri build` reuses the same identity and the "Always Allow" keeps working.

> On the **first** `tauri build`, `codesign` may itself pop one "wants to sign
> using key … in your keychain" prompt — click **Always Allow** and it won't ask
> again.

## What the script does

Creates a self-signed certificate named **"billz Local Signing"** with the
`codeSigning` extended key usage, imports it into your **login** keychain (private
key accessible to `codesign`), and trusts it for the code-signing policy. It only
touches your login keychain — no sudo, no system changes, nothing for distribution
(this is not an Apple Developer ID and the app is not notarized; it's a personal
self-signed identity for signing your own tool on your own Mac).

## Fallback: create the identity in the GUI

If the script fails on your machine, Keychain Access does the same thing reliably:

1. **Keychain Access → Certificate Assistant → Create a Certificate…**
2. Name: **`billz Local Signing`** · Identity Type: **Self-Signed Root** ·
   Certificate Type: **Code Signing** → Create.
3. Re-run `cd app/ui && bun run tauri build`.

## Undo

Delete the **"billz Local Signing"** certificate in **Keychain Access** (login
keychain). Optionally remove `bundle.macOS.signingIdentity` from
`app/tauri.conf.json` to go back to unsigned builds.
