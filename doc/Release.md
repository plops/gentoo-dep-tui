Hier sind die fertige GitHub-Actions-Pipeline (`.github/workflows/release.yml`), eine passende `CHANGELOG.md`-Vorlage und die Schritt-für-Schritt-Anleitung `Release.md`.

### Wie kommen die Changes auf GitHub *vor* die Artefakte?
GitHub stellt auf jeder Release-Seite immer zuerst den **Release-Body** (Beschreibungstext) dar und erst darunter den Block **Assets** (die hochgeladenen Binaries/Archive).  
Der Workflow liest beim Tag-Push genau den Abschnitt zur aktuellen Version aus `CHANGELOG.md` aus und übergibt ihn per `body_path` an `softprops/action-gh-release@v3` — dadurch stehen deine Änderungen automatisch sauber formatiert über den Artefakten.

---

## 1. GitHub Action: `.github/workflows/release.yml`

Lege diese Datei unter `.github/workflows/release.yml` ab:

```yaml
```

### Hinweise zum Workflow
- **Zwei Targets (`gnu` + `musl`)**: `x86_64-unknown-linux-musl` erzeugt ein statisch gelinktes Binary, das auf jedem Gentoo-/Linux-System unabhängig von der installierten `glibc`-Version läuft (alle Abhängigkeiten in `Cargo.toml` sind reines Rust).
- **Sicherheitsnetz**: Der Workflow bricht ab, falls `Cargo.toml` und Git-Tag (`vX.Y.Z`) nicht zusammenpassen oder der passende Abschnitt in `CHANGELOG.md` fehlt bzw. leer ist.
- **Einzelnes Binary statt `.tar.gz`?**: Falls du zusätzlich (oder stattdessen) die entpackte ausführbare Datei direkt als Asset anhängen möchtest, kannst du im Step `Package release archive` einfach `cp "$BIN_PATH" "dist/gentoo-dep-tui-${GITHUB_REF_NAME}-${{ matrix.target }}"` ergänzen und im `files:`-Block von `action-gh-release` ebenfalls `dist/gentoo-dep-tui-*` aufnehmen. (`.tar.gz` hat den Vorteil, dass das `+x`-Recht beim Entpacken direkt erhalten bleibt.)

---

## 2. Vorlage für `CHANGELOG.md`

Lege im Projekt-Root eine `CHANGELOG.md` nach diesem Muster an:


---

## 3. Anleitung: `Release.md`

Lege diese Datei als `Release.md` im Projekt-Root ab:

```markdown
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

git push origin master --follow-tags
```

*(Alternativ explizit in zwei Schritten: `git push origin master` und `git push origin v0.2.0`.)*

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
git push origin master --follow-tags --force-with-lease
```

*(Falls du keinen Force-Push auf `master` machen willst, erstelle statt `--amend` einfach einen normalen Korrektur-Commit.)*
```
