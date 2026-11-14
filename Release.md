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
