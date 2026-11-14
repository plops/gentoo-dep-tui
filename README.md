# gentoo-dep-tui

**`ncdu` for Gentoo USE flags** — an interactive offline terminal UI that lets you explore what your USE flags actually pull into your dependency graph, how many packages and megabytes they cost, why a flag shows `0 B` impact, and how to cleanly cut unwanted dependencies — before touching `/etc/portage/package.use`.

> **Note:** The codebase's TUI labels and status messages are currently in German (e.g., `Pakete`, `Quell-DL`, `Geteilte Abhaengigkeiten`, `Cut-Analyse`). Key concepts and translations are noted below.

---

## Why `gentoo-dep-tui`?

In Gentoo, toggling a single USE flag rarely affects just one package:
- Turning off `-rdw` on `sys-power/tlp` can drop **14 transitive packages** (`10.8 MB` of binary packages).
- Turning off `-gpg` on `dev-vcs/git` may save **`0 B`** because `app-crypt/gnupg`, `app-crypt/gpgme`, `app-portage/gemato`, and `app-portage/getuto` still hold `app-alternatives/gpg` in the graph.
- Flipping `python_single_target_*` or `USE`-coupled dependencies (`[flag?]`, `[flag=]`) can trigger `REQUIRED_USE` cascades or cross-package conflicts.

Running `emerge -pv` for every "what-if?" experiment takes seconds per check and gives no interactive tree-size breakdown. `gentoo-dep-tui` parses your local Portage metadata (`metadata/md5-cache`, `Manifest`, profiles, `/etc/portage`, and an optional binary host `Packages` index) directly in memory and simulates flag toggles in parallel across CPU cores.

---

## Features

- **Offline & Non-Destructive**: Reads `/var/db/repos/gentoo` and `/etc/portage` read-only; writes changes only when you explicitly press `S` (by default to `/tmp/gentoo-dep-tui.package.use`).
- **Exclusive Cost Sorting (`o`)**: Like `ncdu`, sorts packages by how many packages or bytes are held *exclusively* by that package (if removed from `world`/parents, how much of the graph collapses with it).
- **Per-Flag Marginal Deltas**: Shows exact binary package delta (`bin`), source tarball download delta (`dl` from `Manifest` `DIST` entries + conditional `SRC_URI`), net package diff (`(+N/-M pkg)`), and `CONFLICT` warnings before you toggle.
- **Runtime vs. Build-Only Separation**: Tracks `DEPEND`/`BDEPEND`/`IDEPEND` (and their runtime closures not already in runtime) separately as `Build-Only` so build tools do not distort runtime totals.
- **Cascade & Conflict Solver**: Automatically evaluates `REQUIRED_USE` (`||`, `^^`, `??`, conditionals) and inter-package USE dependencies (`[flag]`, `[-flag]`, `[flag?]`, `[flag=]`, `(+)`/`(-)` defaults), offering one-key **Quick-Switch** resolutions when conflicts arise.
- **Shared-Dependency & Cut Analysis (`x` / `a`)**: Explains `0 B` deltas by listing other packages holding the same dependency, and computes the minimal set of **Flag-Blockers** (or hard `world`-blockers) needed to eliminate any transitive dependency from the graph—plus one-key application (`a`).
- **Global Big Levers (Tab `2`)**: Evaluates turning off each active USE flag *system-wide* across all packages that currently enable it.
- **`||` Alternative Switching**: Inspect and cycle `AnyOf` (`|| ( ... )`) dependency choices directly from the left pane.
- **Clean `package.use` Export (`S`)**: Exports minimal `cat/pkg flag -other_flag` lines stripped of profile defaults, platform flags, forced flags, and masked flags.

---

## Requirements & Build

- **Rust**: `1.88` or newer (`edition = "2024"`)
- **Gentoo metadata**: A synced Gentoo repository (`metadata/md5-cache` and package `Manifest` files), a `world` file, and Portage configuration (or a directory `--snapshot` containing them).

```bash
git clone <your-repo-url>
cd gentoo-dep-tui
cargo build --release
```

The compiled binary will be at `target/release/gentoo-dep-tui`.

---

## Quick Start

### On a live Gentoo system

Without `--binhost`, you get package counts and source tarball sizes (`dl`). For compressed binary package sizes (`bin`), point `--binhost` to a local Gentoo `Packages` index:

```bash
# Optional: fetch the official Gentoo amd64/23.0 binhost index
wget -O /tmp/Packages \
  https://distfiles.gentoo.org/releases/amd64/binpackages/23.0/x86-64/Packages

./target/release/gentoo-dep-tui --binhost /tmp/Packages
```

### Using an offline snapshot (e.g., inside a container or non-Gentoo host)

If `/var/db/repos/gentoo` or `/etc/portage` does not exist locally, bundle a snapshot directory (`repo/`, `world`, `etc/portage/`, or top-level `make.conf` / `package.use`) and pass `--snapshot`:

```bash
./target/release/gentoo-dep-tui \
  --snapshot /path/to/gentoo-snapshot \
  --profile default/linux/amd64/23.0/no-multilib \
  --binhost /tmp/Packages
```

---

## CLI Options

| Flag | Default | Description |
| :--- | :--- | :--- |
| `--repo <PATH>` | `/var/db/repos/gentoo` | Path to the Gentoo repository (reads `metadata/md5-cache` and `Manifest` files). |
| `--profile-dir <PATH>` | `<repo>/profiles` | Path to the profiles directory. |
| `--profile <NAME>` | *(from `make.profile`)* | Explicit profile path relative to `--profile-dir` (falls back to `default/linux/amd64/23.0` if `make.profile` cannot be resolved). |
| `--world <PATH>` | `/var/lib/portage/world` | Path to the `world` file. |
| `--portage-dir <PATH>` | `/etc/portage` | Portage config directory (`make.conf`, `package.use`, `package.mask`, `package.unmask`, `package.accept_keywords`, `make.profile`). |
| `--make-conf <PATH>` | `<portage-dir>/make.conf` | Explicit path to `make.conf` (overrides `--snapshot` and `--portage-dir`). |
| `--package-use <PATH>` | `<portage-dir>/package.use` | Explicit path to `package.use` file or directory (overrides `--snapshot` and `--portage-dir`). |
| `--snapshot <PATH>` | *(none)* | Snapshot root; automatically picks up `<snap>/repo`, `<snap>/world`, `<snap>/etc/portage`, `make.conf`, and `package.use` if present. |
| `--binhost <PACKAGES-FILE>` | *(none)* | Optional path to a local Gentoo binary host `Packages` index for `Binpkg` size metrics. |
| `--out-use <PATH>` | `/tmp/gentoo-dep-tui.package.use` | Output file path written when pressing `S` in the TUI. |

---

## Navigating the TUI

### Layout (Tab `1`: Packages & Alternatives)

1. **Left Pane (`Pakete`)**: Lists `world` packages, followed by `||` (`AnyOf`) choice groups, followed by transitive runtime dependencies.
2. **Middle Pane (`Details / USE-Flags`)**: Lists toggleable USE flags for the selected package (with `[x]`/`[ ]`, `bin` delta, `dl` delta, `(+N/-M pkg)`, and `CONFLICT` markers), or candidates for a selected `||` group.
3. **Right Pane (`Blame / Pros-Cons` & `Cut-Analyse`)**:
   - Dependency blame chains (`Why is this package here?` / `Warum ist das hier?`)
   - Active conflicts on the selected package
   - Added/removed packages for the highlighted flag
   - Shared dependencies (`Geteilte Abhaengigkeiten`: other packages also holding a conditional dependency)
   - Inline **Cut Analysis** (`Cut-Analyse`) showing Flag-Blockers, hard `world`-blockers, and savings
   - Local/global USE flag descriptions from `use.local.desc`, `use.desc`, and `desc/*.desc`

### Keybindings

| Key | Action |
| :--- | :--- |
| `1` / `2` | Switch between **Packages & Alternatives** (`1`) and **Global Big Levers** (`2`, computed lazily on first visit) |
| `Tab` | Cycle active pane (`Left` → `Middle` → `Right`) |
| `↑`/`↓` or `k`/`j` | Move selection (in the Right pane, steps through referenced packages and auto-previews Cut Analysis) |
| `PageUp`/`PageDown`, `Home`/`End` | Jump by page or to top/bottom (scrolls Right pane text) |
| `/` | Live-filter left package list (`Esc` clears filter, `Enter` confirms, `Tab` jumps to flags) |
| `o` | Cycle left-pane sort order: **Default** → **Exclusive Packages** → **Exclusive Size** |
| `Space` | Jump to Middle pane (when in Left) · Toggle selected USE flag / open Quick-Switch modal (Middle) · Disable flag globally (Tab `2`) |
| `Enter` | Cycle `||` alternative (when selected) · Open Quick-Switch modal for an active conflict |
| `x` | Run Cut Analysis for the selected package (or selected target in the Right pane) |
| `a` | Apply all `Flag-Blocker` overrides from the current Cut Analysis |
| `u` | Undo last override change (up to 32 steps) |
| `S` | Export modified USE flags to `--out-use` (`/tmp/gentoo-dep-tui.package.use`) |
| `?` / `F1` | Open Help overlay |
| `Esc` | Clear filter → return to Left pane → quit |
| `q` / `Ctrl+C` | Quit |

---

## Understanding the Numbers

- **Footer (`Binpkg X (hits/total Hits)  Quell-DL Y  Build-Only ...  Pakete N  Konflikte M`)**:
  - `Binpkg`: Sum of compressed binary package `SIZE` values from `--binhost` across all runtime packages. Packages missing from the binhost count as `0 B` (check the `hits/total` ratio). Without `--binhost`, shows `n/a (ohne --binhost)`.
  - `Quell-DL`: Sum of `Manifest` `DIST` tarball sizes matching each package's active `SRC_URI` conditionals.
  - `Build-Only`: `DEPEND`/`BDEPEND`/`IDEPEND` packages (and their runtime closures) not present in the runtime graph.
- **Middle Pane (`Pkg-Quell-DL` and `0 B` deltas)**:
  - `Pkg-Quell-DL` is the source download size of the package *itself*, not its subgraph.
  - A `0 B bin / 0 B dl` delta usually means either a **compile-time-only feature** (no extra runtime dependency/distfile) or a **shared dependency** that remains pinned by another package (check `Geteilte Abhaengigkeiten` on the right).
- **Tab `2` (`Global Big Levers`)**:
  - Simulates turning each currently active flag **OFF** across every package that enables it. Note that source download (`dl`) can occasionally *increase* when disabling a flag if `!flag?` `SRC_URI` branches pull fallback archives.

---

## Example Walkthrough

Below is an actual session starting from `gentoo-dep-tui --binhost /tmp/Packages` on a 554-package runtime graph (`756.6 MB` binpkg, `3313.0 MB` source DL):

1. **Find the biggest exclusive trees (`o`)**:
   Pressing `o` sorts by exclusive package count. `sys-power/tlp (19 pkg, 11.7 MB)` jumps to the top:
   - `rdw` pulls 14 exclusive packages (`-10.8 MB bin / -17.6 MB dl`)
   - `ppd` pulls 3 exclusive packages (`-800.0 kB bin / -2.2 MB dl`)
   - `python_single_target_python3_12` and `3_13` show `CONFLICT` because `python_single_target_python3_14` is currently active (`^^` constraint).
   Disabling `ppd`, `rdw`, and `python_single_target_python3_14` drops the runtime graph from **554 to 537 packages** (`-11.6 MB` binpkg).

2. **Trim `dev-vcs/git` and investigate a `0 B` flag**:
   Disabling `nls`, `perl` (`-12 pkg`, `-810.0 kB`), `python_single_target_python3_14`, and `webdav` reduces `git` from `(13 pkg, 10.6 MB)` to `(1 pkg, 9.8 MB)`.
   Highlighting `gpg` shows `0 B bin / 0 B dl`, while the Right pane immediately explains why:
   ```text
   Geteilte Abhaengigkeiten:
   > app-alternatives/gpg
   gehalten von app-crypt/gnupg
   gehalten von app-crypt/gpgme
   gehalten von app-portage/gemato
   gehalten von app-portage/getuto
   ```

3. **Cut a shared holder in two keystrokes (`Tab` → `j`/`k` → `a`)**:
   Tabbing into the Right pane and stepping down to `app-crypt/gpgme` automatically runs Cut Analysis:
   - `Flag-Blocker`: `app-portage/portage-utils [+static]`
   - `Ohne Flag-Blocker`: `-500.0 kB bin / -1.4 MB dl (-1/+0 pkg)`
   Pressing `a` applies `app-portage/portage-utils static`, immediately removing `app-crypt/gpgme` (`525 → 524` packages).

4. **Export (`S`)**:
   Pressing `S` writes clean `/tmp/gentoo-dep-tui.package.use`:
   ```text
   app-portage/portage-utils static
   dev-vcs/git -nls -perl -python_single_target_python3_14 -webdav
   sys-power/tlp -ppd -python_single_target_python3_14 -rdw
   ```

---

## Architecture Overview

The codebase is intentionally compact (~1,900 lines across four modules) and designed around **immutable shared baselines + cheap counterfactual forks**:

- **`src/main.rs`**: CLI parsing (`clap`) and layered path resolution (`--make-conf` / `--package-use` > `--snapshot` > `--portage-dir`).
- **`src/repo.rs`**:
  - Flattens Gentoo profile `parent` DAGs via iterative post-order DFS and layers `make.defaults`, `use.mask`, `use.force`, `make.conf`, `package.use`, `package.mask`/`unmask`, and `package.accept_keywords`.
  - Pre-compiles `package.*` rules into `cpn`-indexed lookup caches.
  - Loads `metadata/md5-cache` and deduplicated `Manifest` files in parallel via `rayon`, storing each package's baseline USE set lazily in a `OnceLock`.
- **`src/solver.rs`**:
  - `Engine<'a>` borrows `&RepoIndex` and `&Profile` without cloning, owning only `overrides` and `or_decisions`.
  - `resolve_with_cascade` runs BFS dependency resolution (`RDEPEND`/`PDEPEND`) inside a fixed-point loop (up to 8 rounds) enforcing `REQUIRED_USE` and inter-package USE dependencies (`[flag]`, `[flag?]`, `[flag=]`, etc.), with backward parent-flipping and `QuickSwitch` conflict generation.
  - `package_deltas_with_base` and `big_levers_with_base` run counterfactual simulations in parallel (`rayon`), statically pruning flags that never appear in `RDEPEND`, `PDEPEND`, `SRC_URI`, `REQUIRED_USE`, or incoming `use_deps`.
  - `node_costs` computes exclusive package/byte counts for all nodes in a single allocation-free generation-counter BFS sweep.
  - `cut_package` walks `reverse` edges labeled with conditional `via_flag`s upward until hitting breakable flags (`FlagBlocker`) or `world` roots.
- **`src/tui.rs`**:
  - `ratatui` + `crossterm` event loop with an 80 ms idle poll: scrolling updates selection immediately while deferring uncached delta/cut computations until the cursor settles, keeping navigation responsive across hundreds of packages.

---

## Limitations & Caveats

- **Metadata-only approximation**: Reads `md5-cache` rather than executing ebuilds or Portage's full slot-conflict/backtracking solver. Version selection picks the newest unmasked, keyword-accepted `PkgVersion` matching each dependency atom (it does not maintain multiple simultaneously installed slots of the same `cat/pkg` as separate graph nodes).
- **`ExactlyOneOf` / `AtMostOneOf` (`^^` / `??`)**: Currently picks the first available dependency candidate rather than exposing interactive selectors like `||` (`AnyOf`) groups do.
- **No direct system modification**: Never modifies `/etc/portage/package.use` directly unless you explicitly pass `--out-use /etc/portage/package.use` (or copy the exported file after reviewing it).

