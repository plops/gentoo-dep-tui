Hier sind die fertige GitHub-Actions-Pipeline (`.github/workflows/release.yml`), eine passende `CHANGELOG.md`-Vorlage und die Schritt-für-Schritt-Anleitung `Release.md`.

### Wie kommen die Changes auf GitHub *vor* die Artefakte?
GitHub stellt auf jeder Release-Seite immer zuerst den **Release-Body** (Beschreibungstext) dar und erst darunter den Block **Assets** (die hochgeladenen Binaries/Archive).  
Der Workflow liest beim Tag-Push genau den Abschnitt zur aktuellen Version aus `CHANGELOG.md` aus und übergibt ihn per `body_path` an `softprops/action-gh-release@v3` — dadurch stehen deine Änderungen automatisch sauber formatiert über den Artefakten.

---

## 1. GitHub Action: `.github/workflows/release.yml`

Lege diese Datei unter `.github/workflows/release.yml` ab:

```yaml
name: Release

on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+*'

permissions:
  contents: write

jobs:
  build:
    name: Build (${{ matrix.target }})
    runs-on: ubuntu-latest
    strategy:
      fail-fast: true
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
          - target: x86_64-unknown-linux-musl

    steps:
      - name: Checkout repository
        uses: actions/checkout@v6

      - name: Verify Cargo.toml version matches tag
        shell: bash
        run: |
          TAG_VERSION="${GITHUB_REF_NAME#v}"
          CARGO_VERSION="$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)"
          echo "Tag version:   $TAG_VERSION"
          echo "Cargo version: $CARGO_VERSION"
          if [[ "$TAG_VERSION" != "$CARGO_VERSION" ]]; then
            echo "::error::Tag ($GITHUB_REF_NAME) stimmt nicht mit Cargo.toml ($CARGO_VERSION) ueberein!"
            exit 1
          fi

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Cache Cargo dependencies
        uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Run tests
        run: cargo test

      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }}

      - name: Package release archive
        shell: bash
        run: |
          BIN_PATH="target/${{ matrix.target }}/release/gentoo-dep-tui"
          strip "$BIN_PATH"

          STAGE_DIR="stage"
          ARCHIVE_NAME="gentoo-dep-tui-${GITHUB_REF_NAME}-${{ matrix.target }}.tar.gz"

          mkdir -p "$STAGE_DIR" dist
          cp "$BIN_PATH" "$STAGE_DIR/gentoo-dep-tui"
          [[ -f README.md ]] && cp README.md "$STAGE_DIR/"
          for lic in LICENSE LICENSE-MIT LICENSE-APACHE; do
            [[ -f "$lic" ]] && cp "$lic" "$STAGE_DIR/"
          done

          tar -C "$STAGE_DIR" -czf "dist/$ARCHIVE_NAME" .

      - name: Upload build artifact
        uses: actions/upload-artifact@v7
        with:
          name: release-${{ matrix.target }}
          path: dist/*.tar.gz
          if-no-files-found: error

  release:
    name: Publish GitHub Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v6

      - name: Download build artifacts
        uses: actions/download-artifact@v8
        with:
          pattern: release-*
          merge-multiple: true
          path: dist

      - name: Extract release notes from CHANGELOG.md
        shell: bash
        run: |
          TAG_VERSION="${GITHUB_REF_NAME#v}"
          python3 - "$TAG_VERSION" <<'PY' > release-notes.md
          import pathlib
          import re
          import sys

          version = sys.argv[1]
          changelog = pathlib.Path("CHANGELOG.md")
          if not changelog.is_file():
              sys.exit("Fehler: CHANGELOG.md nicht gefunden.")

          text = changelog.read_text(encoding="utf-8")
          ver_re = re.escape(version)
          header = re.search(rf"^## \[?v?{ver_re}\]?(?:\s+-.*)?$", text, re.MULTILINE)
          if not header:
              sys.exit(f"Fehler: Kein Abschnitt '## [{version}]' in CHANGELOG.md gefunden.")

          rest = text[header.end():]
          next_section = re.search(r"^(?:## \[?|\[[^\]]+\]:\s)", rest, re.MULTILINE)
          body = (rest[:next_section.start()] if next_section else rest).strip()
          if not body:
              sys.exit(f"Fehler: Abschnitt fuer Version {version} in CHANGELOG.md ist leer.")

          print(body)
          PY

      - name: Detect prerelease and generate SHA256SUMS
        id: meta
        shell: bash
        run: |
          if [[ "$GITHUB_REF_NAME" == *-* ]]; then
            echo "prerelease=true" >> "$GITHUB_OUTPUT"
          else
            echo "prerelease=false" >> "$GITHUB_OUTPUT"
          fi

          (cd dist && sha256sum *.tar.gz > SHA256SUMS.txt)

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v3
        with:
          name: "gentoo-dep-tui ${GITHUB_REF_NAME}"
          body_path: release-notes.md
          prerelease: ${{ steps.meta.outputs.prerelease }}
          fail_on_unmatched_files: true
          files: |
            dist/*.tar.gz
            dist/SHA256SUMS.txt
```

### Hinweise zum Workflow
- **Zwei Targets (`gnu` + `musl`)**: `x86_64-unknown-linux-musl` erzeugt ein statisch gelinktes Binary, das auf jedem Gentoo-/Linux-System unabhängig von der installierten `glibc`-Version läuft (alle Abhängigkeiten in `Cargo.toml` sind reines Rust).
- **Sicherheitsnetz**: Der Workflow bricht ab, falls `Cargo.toml` und Git-Tag (`vX.Y.Z`) nicht zusammenpassen oder der passende Abschnitt in `CHANGELOG.md` fehlt bzw. leer ist.
- **Einzelnes Binary statt `.tar.gz`?**: Falls du zusätzlich (oder stattdessen) die entpackte ausführbare Datei direkt als Asset anhängen möchtest, kannst du im Step `Package release archive` einfach `cp "$BIN_PATH" "dist/gentoo-dep-tui-${GITHUB_REF_NAME}-${{ matrix.target }}"` ergänzen und im `files:`-Block von `action-gh-release` ebenfalls `dist/gentoo-dep-tui-*` aufnehmen. (`.tar.gz` hat den Vorteil, dass das `+x`-Recht beim Entpacken direkt erhalten bleibt.)

---

## 2. Vorlage für `CHANGELOG.md`

Lege im Projekt-Root eine `CHANGELOG.md` nach diesem Muster an:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-14

### Added
- Interactive 3-pane TUI for Gentoo USE flag exploration (`md5-cache`, `Manifest`, optional binhost `Packages`)
- Exclusive cost sorting (`node_costs`), marginal flag deltas, and global Big Levers view
- Cascade solver (`REQUIRED_USE` and inter-package USE deps) with Quick-Switch conflict resolution
- Shared dependency inspection and Cut Analysis (`x` / `a`)
- Minimal `package.use` export (`S`)
```

---

## 3. Anleitung: `Release.md`

Lege diese Datei als `Release.md` im Projekt-Root ab:

```markdown
# Release-Anleitung (`gentoo-dep-tui`)

Jedes Release wird vollautomatisch über einen Git-Tag (`vX.Y.Z`) durch `.github/workflows/release.yml` gebaut und auf GitHub veröffentlicht.

---

## 1. Wo werden Änderungen eingetragen? (`CHANGELOG.md`)

Laufende Änderungen werden während der Entwicklung in **`CHANGELOG.md`** unter der Überschrift **`## [Unreleased]`** gesammelt — zum Beispiel unter `### Added`, `### Changed` oder `### Fixed`.

Beim Release benennst du `## [Unreleased]` in die konkrete Versionsnummer um:

```markdown
## [Unreleased]

## [0.2.0] - 2026-11-20

### Added
- Neue Funktion XY

### Fixed
- Fehler Z behoben
```

**Wichtig für die Darstellung auf GitHub:**
- Die Überschrift muss exakt dem Muster `## [0.2.0]` (optional mit Datum ` - YYYY-MM-DD`) entsprechen — passend zum Tag `v0.2.0`.
- Der Release-Workflow extrahiert diesen Abschnitt automatisch und setzt ihn als Release-Beschreibung (`body_path`). Dadurch erscheinen deine Changes auf der GitHub-Release-Seite **direkt über den Artefakten (`Assets`)**.
- Ist der Abschnitt nicht vorhanden oder leer, bricht die GitHub Action mit einer Fehlermeldung ab, damit kein unvollständiges Release veröffentlicht wird.

---

## 2. Version erhöhen (`Cargo.toml` & `Cargo.lock`)

1. Öffne **`Cargo.toml`** und erhöhe das Feld `version` nach [Semantic Versioning](https://semver.org/):
   - `PATCH` (`0.1.0` → `0.1.1`): reine Bugfixes
   - `MINOR` (`0.1.0` → `0.2.0`): neue Features, abwärtskompatibel
   - `MAJOR` (`0.2.0` → `1.0.0`): größere/inkompatible Änderungen (z. B. CLI-Flags oder Output-Format)

   ```toml
   [package]
   name = "gentoo-dep-tui"
   version = "0.2.0"
   ```

2. Aktualisiere `Cargo.lock` und prüfe Build sowie Tests lokal:

   ```bash
   cargo check
   cargo test
   ```

   *(Tipp: Falls `Cargo.lock` im Repository getrackt wird, aktualisiert `cargo check` dort automatisch die Paketversion von `gentoo-dep-tui`, sodass `git diff` sie direkt enthält.)*

3. Trage in **`CHANGELOG.md`** das Release-Datum ein, benenne `## [Unreleased]` in `## [0.2.0] - YYYY-MM-DD` um und lege darüber wieder eine leere Sektion `## [Unreleased]` für zukünftige Commits an.

---

## 3. Release-Commit erstellen & Tag pushen

Stelle sicher, dass der Arbeitsbaum sauber ist und nur die Release-Änderungen enthält:

```bash
git status
git diff
```

Danach den Release-Commit anlegen, einen annotierten Tag setzen und beides pushen:

```bash
git add Cargo.toml CHANGELOG.md
# Falls Cargo.lock im Repo getrackt wird:
git add Cargo.lock

git commit -m "chore(release): v0.2.0"
git tag -a v0.2.0 -m "Release v0.2.0"

git push origin main --follow-tags
```

*(Alternativ explizit in zwei Schritten: `git push origin main` und `git push origin v0.2.0`.)*

**Hinweis zu Pre-Releases:** Tags mit Bindestrich wie `v0.2.0-rc.1` (entsprechend `version = "0.2.0-rc.1"` in `Cargo.toml` und `## [0.2.0-rc.1]` in `CHANGELOG.md`) werden von GitHub automatisch als **Pre-release** markiert.

---

## 4. Was passiert auf GitHub?

Nach dem Push des Tags startet `.github/workflows/release.yml` automatisch:
1. Prüft, ob Tag (`v0.2.0`) und `Cargo.toml` (`0.2.0`) exakt übereinstimmen.
2. Führt `cargo test` aus.
3. Kompiliert gestrippte Release-Binaries für:
   - `x86_64-unknown-linux-gnu`
   - `x86_64-unknown-linux-musl` (statisch gelinkt)
4. Extrahiert die Changes aus `CHANGELOG.md`.
5. Erzeugt `SHA256SUMS.txt` und veröffentlicht das GitHub Release mit den Release Notes **vor** den `.tar.gz`-Archiven und Checksummen.

---

## 5. Falls etwas schiefgeht (Tag korrigieren)

Falls der Workflow wegen eines Tippfehlers in `CHANGELOG.md` oder `Cargo.toml` abbricht, solange noch kein Release veröffentlicht wurde:

```bash
# 1. Lokalen und entfernten Tag löschen
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0

# 2. Fehler in Cargo.toml oder CHANGELOG.md beheben und Commit ergänzen
git add Cargo.toml CHANGELOG.md
git commit --amend --no-edit   # (oder neuen Fix-Commit erstellen)

# 3. Tag neu setzen und erneut pushen
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin main --follow-tags --force-with-lease
```

*(Falls du keinen Force-Push auf `main` machen willst, erstelle statt `--amend` einfach einen normalen Korrektur-Commit.)*
```
