Hier ist die vollständig verschlankte, reine TUI-Version von `gentoo-dep-tui` — ohne CLI-Subcommands/Scripting, ohne JSON/Serde, ohne `perf-metrics`-Infrastruktur, ohne `thiserror`-Boilerplate und ohne Tests. Danach folgt das didaktische Architektur- und Code-Handbuch.

---

## Teil 1: Minimaler Quellcode (4 Quelldateien + `Cargo.toml`)

Die ursprünglichen ~24 Module schrumpfen auf eine klare, schichtenreine Pipeline aus vier Dateien:

- `Cargo.toml`
- `src/main.rs` — CLI-Flags (`clap`) und Zusammenbau der Pipeline
- `src/repo.rs` — Portage-Konfiguration, Beschreibungen, Manifest/Binhost und paralleler `md5-cache`-Index
- `src/solver.rs` — Graph-Resolver, Kaskaden-Fixpunkt, Marginal-Deltas, Big Levers und Cut-Analyse
- `src/tui.rs` — `AppState`, Ratatui-Rendering, Keyboard-Dispatch und Event-Loop

### `Cargo.toml`

```toml
```

### `src/main.rs`

```rust

```

### `src/repo.rs`

```rust

```

### `src/solver.rs`

```rust
```

### `src/tui.rs`

```rust
```

---

## Teil 2: Architektur- & Implementierungs-Leitfaden

### 1. Das Kernproblem: Warum ist Gentoo kein `ncdu` für Dateien?

Wer von `ncdu` kommt, denkt an einen Baum: Jedes Verzeichnis ist die Summe seiner Kinder, und löscht man einen Unterordner, sinkt die Gesamtgröße exakt um diesen Betrag.

Bei Gentoo-Paketen scheitert diese Baum-Intuition gleich an vier Stellen:

1. **Geteilte Abhängigkeiten (Diamanten & Zyklen):** Wenn `emacs` und `firefox-bin` beide `x11-libs/gtk+` benötigen, ist der Graph ein gerichteter (manchmal zyklischer) Graph — kein Baum. Schaltet man `gtk` bei `emacs` aus, sinkt die Systemgröße um **0 Byte**, solange `firefox-bin` sie weiter hält.
2. **Runtime vs. Build-Only:** `BDEPEND`/`DEPEND`/`IDEPEND` braucht man nur zum Bauen, `RDEPEND`/`PDEPEND` zur Laufzeit. Vermischt man beides, bläht ein einzelner Compiler-Tool-Chain-Knoten die vermeintliche Systemgröße massiv auf.
3. **Kaskaden über Paketgrenzen:** Ein Flag ist selten lokal. `svt-av1` auf `ffmpeg` schaltet über `REQUIRED_USE="svt-av1? ( aom )"` still `aom` mit ein; ein `foo[bar]`-Atom zwingt ein entferntes Kindpaket in einen bestimmten Flag-Zustand — oder löst einen Widerspruch aus.
4. **Profil-Präzedenz:** Ob ein Flag standardmäßig an ist, entscheidet eine siebenschichtige Kaskade aus Ebuild-`IUSE`, Profil-Vererbung (`parent`-Bäumen), `USE_EXPAND`, `make.conf`, `package.use` (mit Versions-/Slot-Operatoren) und harten `use.force`/`use.mask`-Sperren.

Die Konsequenz für das Architektur-Design ist radikal: **Nichts wird geschätzt.** Jede Zahl in der UI („Was kostet das Flag?“, „Was spart das globale Abschalten?“, „Was bleibt übrig, wenn wir alle Blocker lösen?“) entsteht durch **exakte Simulation** (Toggle → Kaskaden-Fixpunkt → Graph-Neuauflösung → Differenz). Die Kunst des Programms besteht darin, diese Simulationen durch Vorkompilieren, Caching und exakte No-Op-Abkürzungen so schnell zu machen, dass die Oberfläche sich sofort anfühlt.

---

### 2. Die 4-Schichten-Pipeline auf einen Blick

Die gesamte Anwendung bildet eine strikte Einbahnstraße — keine Schicht kennt die darüberliegende:

```text
  /etc/portage + Repo (md5-cache, Manifest, profiles)
         │
         ▼
  ┌──────────────┐   Einmalig beim Start:
  │   repo.rs    │   Profilstapel, Pattern-Indexe, paralleler RepoIndex
  └──────┬───────┘
         │ &RepoIndex, &Profile (read-only)
         ▼
  ┌──────────────┐   Zustandslos / rein funktional:
  │  solver.rs   │   RDEPEND-BFS, Kaskaden-Fixpunkt, Deltas, Big Levers, Cut
  └──────┬───────┘
         │ AppState::refresh() / ensure_deltas()
         ▼
  ┌──────────────┐   Interaktion & Immediate-Mode-Rendering:
  │   tui.rs     │   3-Pane-Ansicht, 80ms-Idle-Debounce, Undo, package.use-Export
  └──────────────┘
         ▲
         │ main.rs verdrahtet Args → repo → AppState → tui::run
```

Ein schönes Rust-Detail in `main.rs`: Da `profile` und `index` auf dem Stack von `run()` leben und `AppState<'a>` sie nur borgt (`&'a RepoIndex`, `&'a Profile`), benötigt das Programm **keinen einzigen `Box::leak` und kein `Arc`**. Beim Beenden der TUI räumt der normale Rust-Scope alles deterministisch auf.

---

### 3. Deep-Dive: `src/repo.rs` — Konfiguration und Index als Daten-Pipeline

#### 3.1 Der Profilstapel und warum Reihenfolge alles ist
Gentoo-Profile erben über `parent`-Dateien, wobei ein Profil sogar *mehrere* Eltern haben darf. `load_profile_stack` löst das per **Post-Order-Tiefensuche** auf: zuerst die ältesten Vorfahren, zuletzt das Kindprofil. Warum? Weil `-flag` in `use.mask` oder `-atom` in `package.mask` frühere Einträge der Eltern wieder *aufhebt*.

Danach greifen die lokalen Dateien aus `/etc/portage` in exakter Portage-Präzedenz (`Profile::effective_defaults_inner`):

1. `IUSE`-Defaults (`+flag` im Ebuild)
2. Plattform-Fakten (`ARCH`, `elibc_*`, `kernel_*`)
3. `USE_EXPAND`-Defaults (nur falls das Flag in `IUSE` deklariert ist)
4. Globale `USE` aus `make.conf`
5. `package.use` (in Datei-/Zeilenreihenfolge, inklusive `>=pkg-1.0` und `:slot/subslot`)
6. `use.force` (nur für deklarierte `IUSE`-Flags)
7. `use.mask` (gewinnt immer)

**Wichtiger Schutz vor Phantom-Flags:** Schritt 3 bis 6 wenden nur Flags an, die im `iuse_defaults` des jeweiligen Pakets stehen. Ohne dieses *IUSE-Gate* würde ein globales `USE="openmp"` auf jedem Paket erscheinen, auch wenn das Ebuild gar kein `openmp` kennt.

#### 3.2 Vorkompilierte Pattern-Indexe statt Parsing in Hot Loops
In einem Gentoo-Repo liegen ~34.000 `md5-cache`-Dateien. Würde `is_masked()` oder `package_use_for()` bei jeder Datei alle Zeilen aus `package.mask` / `package.use` neu parsen, entstünden Millionen unnötiger String-Allokationen.

Deshalb übersetzt `compile_pattern_caches` am Ende des Ladevorgangs alle Textzeilen **einmalig** in `MaskRule`-
