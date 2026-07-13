# billz task runner. Run `just` to list recipes.
# Rust recipes run at the workspace root; frontend recipes run in app/ui.
# Requires: just, cargo, bun. (brew install just)

# Show the recipe list.
default:
    @just --list

# The full gate: everything CI / a code review checks (Rust + frontend).
verify: fmt-check lint test ui-check ui-test ui-build
    @echo "✓ all checks passed"

# ---- Run the app ----

# Launch the desktop app (Tauri + Vite dev server). This is the one you want.
dev:
    cd app/ui && bun run tauri dev

# Build the signed release .app / .dmg (run `just setup-signing` once first).
app-build:
    cd app/ui && bun run tauri build

# ---- Rust (workspace) ----

# Compile the whole workspace.
build:
    cargo build --workspace

# Run the Rust tests (DEV-box integration tests run when MSSQL_* is set, else skip).
test:
    cargo test --workspace

# Clippy with warnings-as-errors (the project's lint bar).
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format all Rust code.
fmt:
    cargo fmt --all

# Check formatting without writing (used by `verify`).
fmt-check:
    cargo fmt --all --check

# Supply-chain / advisory audit (cargo-deny; config in deny.toml). Flags real
# vulnerabilities, yanked crates, and license issues. Transitive unmaintained
# noise from Tauri's gtk3 Linux stack is filtered (see deny.toml / billz-0gh.9).
# Needs `cargo install cargo-deny`. Not in `verify` (it fetches the advisory DB).
audit:
    cargo deny check

# ---- Frontend (app/ui) ----

# Install JS deps with bun.
install:
    cd app/ui && bun install

# Type-check the Svelte + TS frontend (svelte-check).
ui-check:
    cd app/ui && bun run check

# Run the frontend unit tests (bun test).
ui-test:
    cd app/ui && bun test

# Production bundle the frontend (vite build).
ui-build:
    cd app/ui && bun run build

# ---- DEV-box smoke probes (need MSSQL_* env + a reachable box) ----

# Typed type-decoding probe against the DEV box.
probe-typed:
    cargo run -p billz-core --example typed_probe

# Untyped column/row dump probe against the DEV box.
probe-dynamic:
    cargo run -p billz-core --example dynamic_dump

# ---- macOS code signing (one-time; see SIGNING.md) ----

# Create the self-signed code-signing identity so signed builds stop re-prompting.
setup-signing:
    ./scripts/setup-macos-signing.sh

# ---- Issue tracker (beads) ----

# Show issues ready to work on.
ready:
    bd ready
