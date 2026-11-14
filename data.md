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
[package]
name = "gentoo-dep-tui"
version = "0.1.0"
edition = "2024"
rust-version = "1.88"
description = "ncdu fuer Gentoo-USE-Flags (minimale TUI)"

[dependencies]
portage-atom = "0.12"
portage-metadata = "0.11"
clap = { version = "4", features = ["derive"] }
rayon = "1"
walkdir = "2"
ratatui = "0.30"
crossterm = "0.29"
anyhow = "1"
```

### `src/main.rs`

```rust
mod repo;
mod solver;
mod tui;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "gentoo-dep-tui", about = "ncdu fuer Gentoo-USE-Flags")]
struct Args {
    #[arg(long, default_value = "/var/db/repos/gentoo")]
    repo: PathBuf,
    #[arg(long)]
    profile_dir: Option<PathBuf>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long, default_value = "/var/lib/portage/world")]
    world: PathBuf,
    #[arg(long, value_name = "PACKAGES-DATEI")]
    binhost: Option<PathBuf>,
    #[arg(long)]
    make_conf: Option<PathBuf>,
    #[arg(long)]
    package_use: Option<PathBuf>,
    #[arg(long, default_value = "/etc/portage")]
    portage_dir: PathBuf,
    #[arg(long)]
    snapshot: Option<PathBuf>,
    #[arg(long, default_value = "/tmp/gentoo-dep-tui.package.use")]
    out_use: PathBuf,
}

impl Args {
    fn eff_repo(&self) -> PathBuf {
        if let Some(snap) = &self.snapshot
            && snap.join("repo").is_dir()
        {
            return snap.join("repo");
        }
        self.repo.clone()
    }

    fn eff_world(&self) -> PathBuf {
        if let Some(snap) = &self.snapshot {
            let w = snap.join("world");
            if w.is_file() {
                return w;
            }
        }
        self.world.clone()
    }

    fn eff_portage_dir(&self) -> PathBuf {
        if let Some(snap) = &self.snapshot {
            let p = snap.join("etc/portage");
            if p.is_dir() {
                return p;
            }
        }
        self.portage_dir.clone()
    }

    fn eff_make_conf(&self) -> PathBuf {
        if let Some(p) = &self.make_conf {
            return p.clone();
        }
        if let Some(snap) = &self.snapshot {
            for cand in [snap.join("make.conf"), snap.join("etc/portage/make.conf")] {
                if cand.is_file() {
                    return cand;
                }
            }
        }
        self.eff_portage_dir().join("make.conf")
    }

    fn eff_package_use(&self) -> PathBuf {
        if let Some(p) = &self.package_use {
            return p.clone();
        }
        if let Some(snap) = &self.snapshot {
            for cand in [
                snap.join("package.use"),
                snap.join("etc/portage/package.use"),
            ] {
                if cand.exists() {
                    return cand;
                }
            }
        }
        self.eff_portage_dir().join("package.use")
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let repo_dir = args.eff_repo();
    let profile_dir = args
        .profile_dir
        .clone()
        .unwrap_or_else(|| repo_dir.join("profiles"));
    let portage_dir = args.eff_portage_dir();

    let profile = repo::load_profile(
        &profile_dir,
        args.profile.as_deref(),
        &portage_dir,
        &args.eff_make_conf(),
        &args.eff_package_use(),
    )?;
    let world = repo::read_world(&args.eff_world())?;
    let index = repo::RepoIndex::load(&repo_dir, args.binhost.as_deref(), &profile)?;
    let state = tui::AppState::new(&index, &profile, world)?;
    tui::run(state, &args.out_use)
}

fn main() {
    if let Err(err) = run() {
        eprintln!("gentoo-dep-tui: {err:#}");
        std::process::exit(2);
    }
}
```

### `src/repo.rs`

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow};
use portage_atom::{Dep, Operator, Slot, SlotDep, Version};
use portage_metadata::{CacheEntry, Stability};
use rayon::prelude::*;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct UseRule {
    pub dep: Option<Dep>,
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MaskRule {
    pub cpn: String,
    pub unmask: bool,
    pub dep: Option<Dep>,
}

#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub profiles_dir: PathBuf,
    pub arch: String,
    pub accept_keywords: HashSet<String>,
    pub use_mask: HashSet<String>,
    pub use_force: HashSet<String>,
    pub global_use: HashSet<String>,
    pub expand_defaults: HashMap<String, Vec<String>>,
    pub use_expand_hidden: HashSet<String>,
    pub elibc: String,
    pub kernel: String,
    pub package_mask_index: Vec<MaskRule>,
    pub package_use_index: HashMap<String, Vec<UseRule>>,
    pub package_accept_keywords: HashMap<String, Vec<(Option<Dep>, HashSet<String>)>>,
}

impl Profile {
    pub fn is_forced(&self, flag: &str) -> bool {
        self.use_force.contains(flag)
    }

    pub fn is_masked(&self, flag: &str) -> bool {
        self.use_mask.contains(flag)
    }

    pub fn is_platform_flag(&self, flag: &str) -> bool {
        flag == self.arch.to_lowercase()
            || (!self.elibc.is_empty() && flag == format!("elibc_{}", self.elibc.to_lowercase()))
            || (!self.kernel.is_empty()
                && flag == format!("kernel_{}", self.kernel.to_lowercase()))
    }

    pub fn is_hidden_expand_flag(&self, flag: &str) -> bool {
        let lower = flag.to_lowercase();
        self.use_expand_hidden.iter().any(|h| {
            let prefix = h.to_lowercase();
            let prefix = prefix.trim_end_matches('*');
            lower == prefix || lower.starts_with(&format!("{prefix}_"))
        })
    }

    pub fn effective_defaults(
        &self,
        iuse_defaults: &HashMap<String, bool>,
        cpn: &str,
        version_str: &str,
        slot: &str,
    ) -> HashSet<String> {
        self.effective_defaults_inner(iuse_defaults, cpn, version_str, slot, true)
    }

    pub fn effective_defaults_without_package_use(
        &self,
        iuse_defaults: &HashMap<String, bool>,
    ) -> HashSet<String> {
        self.effective_defaults_inner(iuse_defaults, "", "", "", false)
    }

    fn effective_defaults_inner(
        &self,
        iuse_defaults: &HashMap<String, bool>,
        cpn: &str,
        version_str: &str,
        slot: &str,
        include_package_use: bool,
    ) -> HashSet<String> {
        let mut active: HashSet<String> = iuse_defaults
            .iter()
            .filter_map(|(f, on)| (*on).then(|| f.clone()))
            .collect();

        if !self.arch.is_empty() {
            active.insert(self.arch.to_lowercase());
        }
        if !self.elibc.is_empty() {
            active.insert(format!("elibc_{}", self.elibc.to_lowercase()));
        }
        if !self.kernel.is_empty() {
            active.insert(format!("kernel_{}", self.kernel.to_lowercase()));
        }

        for (var, values) in &self.expand_defaults {
            let prefix = var.to_lowercase();
            for value in values {
                let flag = format!("{prefix}_{}", value.to_lowercase());
                if iuse_defaults.contains_key(&flag) {
                    active.insert(flag);
                }
            }
        }

        let mut global: Vec<&String> = self.global_use.iter().collect();
        global.sort();
        for token in global {
            if token == "-*" || iuse_defaults.contains_key(token.trim_start_matches(['-', '+'])) {
                apply_use_token(&mut active, token);
            }
        }

        if include_package_use {
            for token in self.package_use_for(cpn, version_str, slot) {
                let t = token.trim();
                if t == "-*" || iuse_defaults.contains_key(t.trim_start_matches(['-', '+'])) {
                    apply_use_token(&mut active, t);
                }
            }
        }

        for flag in &self.use_force {
            if iuse_defaults.contains_key(flag) {
                active.insert(flag.clone());
            }
        }
        for flag in &self.use_mask {
            active.remove(flag);
        }
        active
    }

    fn package_use_for(&self, cpn: &str, version_str: &str, slot: &str) -> Vec<String> {
        let mut out = Vec::new();
        let Some(rules) = self.package_use_index.get(cpn) else {
            return out;
        };
        let have = Version::parse(version_str).ok();
        for rule in rules {
            if let Some(dep) = &rule.dep {
                let matches = match &have {
                    Some(h) => dep_matches_version_and_slot(dep, h, version_str, slot),
                    None => dep.op.is_none() && dep.slot_dep.is_none(),
                };
                if !matches {
                    continue;
                }
            }
            out.extend(rule.tokens.iter().cloned());
        }
        out
    }
}

pub fn atom_cpn(atom: &str) -> String {
    let atom = atom.split_whitespace().next().unwrap_or(atom);
    if let Ok(dep) = Dep::parse(atom) {
        return dep.cpn.to_string();
    }
    let base = atom.split(':').next().unwrap_or(atom);
    let base = base.split('[').next().unwrap_or(base);
    base.trim_start_matches(['=', '<', '>', '~']).to_string()
}

pub fn dep_matches_version_and_slot(
    dep: &Dep,
    have: &Version,
    have_str: &str,
    slot: &str,
) -> bool {
    if let (Some(op), Some(want)) = (&dep.op, &dep.version)
        && !version_matches(*op, dep.glob, want, have, have_str)
    {
        return false;
    }
    if let Some(SlotDep::Slot {
        slot: Some(named), ..
    }) = &dep.slot_dep
        && !slot_matches(named, slot)
    {
        return false;
    }
    true
}

fn slot_matches(named: &Slot, pkg_full_slot: &str) -> bool {
    let (pkg_slot, pkg_sub) = match pkg_full_slot.split_once('/') {
        Some((s, sub)) => (s, Some(sub)),
        None => (pkg_full_slot, None),
    };
    if named.slot.as_str() != pkg_slot {
        return false;
    }
    match &named.subslot {
        Some(want) => pkg_sub == Some(want.as_str()),
        None => true,
    }
}

fn version_matches(
    op: Operator,
    glob: bool,
    want: &Version,
    have: &Version,
    have_str: &str,
) -> bool {
    use std::cmp::Ordering as Ord;
    if glob && op == Operator::Equal {
        return have_str.starts_with(&want.to_string());
    }
    match op {
        Operator::Less => have.cmp(want) == Ord::Less,
        Operator::LessOrEqual => have.cmp(want) != Ord::Greater,
        Operator::Equal => have.cmp(want) == Ord::Equal,
        Operator::Approximate => strip_revision(have) == strip_revision(want),
        Operator::GreaterOrEqual => have.cmp(want) != Ord::Less,
        Operator::Greater => have.cmp(want) == Ord::Greater,
    }
}

fn strip_revision(version: &Version) -> String {
    let rendered = version.to_string();
    match rendered.rsplit_once("-r") {
        Some((base, tail)) if tail.chars().all(|c| c.is_ascii_digit()) => base.to_string(),
        _ => rendered,
    }
}

fn apply_use_token(set: &mut HashSet<String>, token: &str) {
    let token = token.trim();
    if token == "-*" {
        set.clear();
    } else if let Some(flag) = token.strip_prefix('-') {
        set.remove(flag);
    } else if let Some(flag) = token.strip_prefix('+') {
        set.insert(flag.to_string());
    } else if !token.is_empty() {
        set.insert(token.to_string());
    }
}

pub fn read_world(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "world-Datei nicht gefunden unter {} — bitte mit --world <PFAD> angeben",
            path.display()
        )
    })?;
    Ok(text
        .lines()
        .map(|l| strip_comment(l).trim())
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn load_profile(
    profiles_dir: &Path,
    explicit_profile: Option<&str>,
    portage_dir: &Path,
    make_conf: &Path,
    package_use_path: &Path,
) -> Result<Profile> {
    let profile_name = explicit_profile
        .map(str::to_string)
        .or_else(|| resolve_make_profile(profiles_dir, portage_dir))
        .unwrap_or_else(|| "default/linux/amd64/23.0".to_string());

    let (mut profile, mut expand_vars, mut mask_lines) =
        load_profile_stack(&profiles_dir.join(&profile_name), profiles_dir)
            .context("Profilstapel laden")?;

    if profile.arch.is_empty() {
        profile.arch = "amd64".into();
    }
    if profile.accept_keywords.is_empty() {
        profile.accept_keywords.insert(profile.arch.clone());
    }

    if let Ok(kv) = read_key_value(make_conf) {
        if let Some(use_line) = kv.get("USE") {
            for token in use_line.replace("${USE}", "").split_whitespace() {
                apply_use_token(&mut profile.global_use, token);
            }
        }
        if let Some(keywords) = kv.get("ACCEPT_KEYWORDS") {
            profile
                .accept_keywords
                .extend(keywords.split_whitespace().map(str::to_string));
        }
        if let Some(arch) = kv.get("ARCH") {
            profile.arch = arch.clone();
        }
        expand_vars.extend(profile.expand_defaults.keys().cloned());
        for var in expand_vars {
            if let Some(line) = kv.get(&var) {
                profile
                    .expand_defaults
                    .insert(var, line.split_whitespace().map(str::to_string).collect());
            }
        }
    }

    let use_lines = read_package_use_lines(package_use_path).unwrap_or_default();
    mask_lines.extend(read_atom_lines(&portage_dir.join("package.mask")));
    for line in read_atom_lines(&portage_dir.join("package.unmask")) {
        let atom = line.split_whitespace().next().unwrap_or("");
        if !atom.is_empty() {
            mask_lines.push(if atom.starts_with('-') {
                atom.to_string()
            } else {
                format!("-{atom}")
            });
        }
    }
    let kw_lines = read_atom_lines(&portage_dir.join("package.accept_keywords"));

    compile_pattern_caches(&mut profile, &mask_lines, &use_lines, &kw_lines);
    Ok(profile)
}

fn compile_pattern_caches(
    profile: &mut Profile,
    mask_lines: &[String],
    use_lines: &[(String, Vec<String>)],
    kw_lines: &[String],
) {
    let mut use_index: HashMap<String, Vec<UseRule>> = HashMap::new();
    for (atom, tokens) in use_lines {
        let (key, dep) = match Dep::parse(atom) {
            Ok(dep) => (dep.cpn.to_string(), Some(dep)),
            Err(_) => (atom.clone(), None),
        };
        use_index.entry(key).or_default().push(UseRule {
            dep,
            tokens: tokens.clone(),
        });
    }
    profile.package_use_index = use_index;

    profile.package_mask_index = mask_lines
        .iter()
        .map(|line| {
            let raw = line.split_whitespace().next().unwrap_or("");
            let (atom, unmask) = match raw.strip_prefix('-') {
                Some(rest) if !rest.is_empty() => (rest, true),
                _ => (raw, false),
            };
            let (cpn, dep) = match Dep::parse(atom) {
                Ok(dep) => (dep.cpn.to_string(), Some(dep)),
                Err(_) => (atom.to_string(), None),
            };
            MaskRule { cpn, unmask, dep }
        })
        .collect();

    let mut kw_index: HashMap<String, Vec<(Option<Dep>, HashSet<String>)>> = HashMap::new();
    for line in kw_lines {
        let mut parts = line.split_whitespace();
        let Some(atom) = parts.next() else { continue };
        let rest: Vec<String> = parts.map(str::to_string).collect();
        let (cpn, dep) = match Dep::parse(atom) {
            Ok(dep) => (dep.cpn.to_string(), Some(dep)),
            Err(_) => (atom.to_string(), None),
        };
        let kws: HashSet<String> = if rest.is_empty() {
            [format!("~{}", profile.arch)].into_iter().collect()
        } else {
            rest.into_iter().collect()
        };
        kw_index.entry(cpn).or_default().push((dep, kws));
    }
    profile.package_accept_keywords = kw_index;
}

fn load_profile_stack(
    profile_path: &Path,
    profiles_dir: &Path,
) -> Result<(Profile, HashSet<String>, Vec<String>)> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = vec![(profile_path.to_path_buf(), false)];
    let mut depth = 0usize;

    while let Some((dir, expanded)) = stack.pop() {
        if expanded {
            chain.push(dir);
            continue;
        }
        if !visited.insert(dir.clone()) {
            continue;
        }
        depth += 1;
        if depth > 256 {
            break;
        }
        stack.push((dir.clone(), true));
        let mut parents = Vec::new();
        if let Ok(text) = std::fs::read_to_string(dir.join("parent")) {
            for line in text.lines().map(str::trim) {
                if !line.is_empty() && !line.starts_with('#') {
                    parents.push(normalize_join(&dir, line));
                }
            }
        }
        for parent in parents.into_iter().rev() {
            stack.push((parent, false));
        }
    }

    let mut profile = Profile {
        profiles_dir: profiles_dir.to_path_buf(),
        ..Profile::default()
    };
    let mut expand_vars = HashSet::new();
    let mut mask_lines = Vec::new();

    for dir in &chain {
        apply_flag_list(&mut profile.use_mask, &dir.join("use.mask"))?;
        apply_flag_list(&mut profile.use_force, &dir.join("use.force"))?;
        extend_atom_lines(&mut mask_lines, &dir.join("package.mask"))?;
        if let Ok(kv) = read_key_value(&dir.join("make.defaults")) {
            if let Some(arch) = kv.get("ARCH") {
                profile.arch = arch.clone();
            }
            if let Some(keywords) = kv.get("ACCEPT_KEYWORDS") {
                profile
                    .accept_keywords
                    .extend(keywords.split_whitespace().map(str::to_string));
            }
            if let Some(elibc) = kv.get("ELIBC") {
                profile.elibc = elibc.clone();
            }
            if let Some(kernel) = kv.get("KERNEL") {
                profile.kernel = kernel.clone();
            }
            if let Some(use_line) = kv.get("USE") {
                let mut prev: Vec<&String> = profile.global_use.iter().collect();
                prev.sort();
                let prev_str = prev
                    .into_iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                for token in use_line.replace("${USE}", &prev_str).split_whitespace() {
                    apply_use_token(&mut profile.global_use, token);
                }
            }
            if let Some(use_expand) = kv.get("USE_EXPAND") {
                expand_vars.extend(use_expand.split_whitespace().map(str::to_string));
            }
            if let Some(hidden) = kv.get("USE_EXPAND_HIDDEN") {
                profile
                    .use_expand_hidden
                    .extend(hidden.split_whitespace().map(str::to_string));
            }
        }
    }

    for dir in &chain {
        if let Ok(kv) = read_key_value(&dir.join("make.defaults")) {
            for var in &expand_vars {
                if let Some(values) = kv.get(var) {
                    profile.expand_defaults.insert(
                        var.clone(),
                        values.split_whitespace().map(str::to_string).collect(),
                    );
                }
            }
        }
    }

    if profile.accept_keywords.remove("${ARCH}") {
        profile.accept_keywords.insert(profile.arch.clone());
    }
    if profile.accept_keywords.remove("~${ARCH}") {
        profile.accept_keywords.insert(format!("~{}", profile.arch));
    }

    Ok((profile, expand_vars, mask_lines))
}

fn resolve_make_profile(profiles_dir: &Path, portage_dir: &Path) -> Option<String> {
    let link = portage_dir.join("make.profile");
    let target = std::fs::read_link(&link).ok()?;
    let abs = if target.is_absolute() {
        target
    } else {
        link.parent().unwrap_or(Path::new("/")).join(target)
    };
    let norm_abs = normalize_join(Path::new("/"), &abs.display().to_string());
    let norm_base = normalize_join(Path::new("/"), &profiles_dir.display().to_string());
    let base_comps: Vec<_> = norm_base.components().collect();
    let abs_comps: Vec<_> = norm_abs.components().collect();
    if abs_comps.len() > base_comps.len() && abs_comps[..base_comps.len()] == base_comps[..] {
        let rest: PathBuf = abs_comps[base_comps.len()..].iter().collect();
        let s = rest.display().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

fn normalize_join(base: &Path, rel: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            Component::Normal(c) => out.push(c),
            Component::RootDir | Component::Prefix(_) => out.push(comp.as_os_str()),
        }
    }
    out
}

fn read_key_value(path: &Path) -> Result<HashMap<String, String>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e).with_context(|| format!("Lesefehler: {}", path.display())),
    };
    let mut map = HashMap::new();
    for line in logical_lines(&text) {
        let line = strip_comment(&line).trim();
        if !line.is_empty()
            && let Some((k, v)) = line.split_once('=')
        {
            map.insert(k.trim().to_string(), unquote(v.trim()));
        }
    }
    Ok(map)
}

fn logical_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        cur.push_str(line);
        let (in_single, in_double) = quote_state(&cur);
        if !in_single && continues(&cur) {
            let trimmed = cur.trim_end();
            cur.truncate(trimmed.len().saturating_sub(1));
            cur.push(' ');
            continue;
        }
        if in_single || in_double {
            cur.push('\n');
            continue;
        }
        out.push(std::mem::take(&mut cur));
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn quote_state(buf: &str) -> (bool, bool) {
    let (mut in_single, mut in_double) = (false, false);
    for c in buf.chars() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            _ => {}
        }
    }
    (in_single, in_double)
}

fn continues(buf: &str) -> bool {
    let t = buf.trim_end();
    (t.len() - t.trim_end_matches('\\').len()) % 2 == 1
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn strip_comment(line: &str) -> &str {
    let (mut in_single, mut in_double) = (false, false);
    for (i, c) in line.char_indices() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return &line[..i],
            _ => {}
        }
    }
    line
}

fn extend_atom_lines(target: &mut Vec<String>, path: &Path) -> Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("Lesefehler: {}", path.display())),
    };
    for line in text.lines() {
        let line = strip_comment(line).trim();
        if !line.is_empty() {
            target.push(line.to_string());
        }
    }
    Ok(())
}

fn apply_flag_list(target: &mut HashSet<String>, path: &Path) -> Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("Lesefehler: {}", path.display())),
    };
    for line in text.lines() {
        let line = strip_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        let flags = line.split(':').next_back().unwrap_or(line);
        for token in flags.split_whitespace() {
            if let Some(flag) = token.strip_prefix('-') {
                target.remove(flag);
            } else if let Some(flag) = token.strip_prefix('+') {
                target.insert(flag.to_string());
            } else if !token.contains('/') {
                target.insert(token.to_string());
            }
        }
    }
    Ok(())
}

fn read_atom_lines(path: &Path) -> Vec<String> {
    let files = collect_config_files(path);
    let mut out = Vec::new();
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            for line in text.lines() {
                let line = strip_comment(line).trim();
                if !line.is_empty() {
                    out.push(line.to_string());
                }
            }
        }
    }
    out
}

fn read_package_use_lines(path: &Path) -> Result<Vec<(String, Vec<String>)>> {
    let mut out = Vec::new();
    for file in collect_config_files(path) {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("Lesefehler: {}", file.display()))?;
        for line in text.lines() {
            let line = strip_comment(line).trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            if let Some(atom) = parts.next() {
                out.push((atom.to_string(), parts.map(str::to_string).collect()));
            }
        }
    }
    Ok(out)
}

fn collect_config_files(path: &Path) -> Vec<PathBuf> {
    if path.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        files
    } else if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        Vec::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct UseDescriptions {
    global: HashMap<String, String>,
    local: HashMap<String, HashMap<String, String>>,
}

impl UseDescriptions {
    pub fn load(profiles_dir: &Path) -> Self {
        let mut out = Self::default();
        out.global
            .extend(parse_desc_file(&profiles_dir.join("use.desc")));
        if let Ok(entries) = std::fs::read_dir(profiles_dir.join("desc")) {
            let mut files: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
            files.sort();
            for file in files {
                if file.is_file() {
                    for (flag, text) in parse_desc_file(&file) {
                        out.global.entry(flag).or_insert(text);
                    }
                }
            }
        }
        for (cpn, flag, text) in parse_local_desc_file(&profiles_dir.join("use.local.desc")) {
            out.local.entry(cpn).or_default().insert(flag, text);
        }
        out
    }

    pub fn describe(&self, cpn: Option<&str>, flag: &str) -> Option<&str> {
        if let Some(cpn) = cpn
            && let Some(text) = self.local.get(cpn).and_then(|m| m.get(flag))
        {
            return Some(text);
        }
        self.global.get(flag).map(String::as_str)
    }
}

fn parse_desc_file(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rest = match line.split_once(':') {
            Some((head, tail)) if head.contains('/') => tail.trim(),
            _ => line,
        };
        if let Some((flag, desc)) = rest.split_once(" - ") {
            let (flag, desc) = (flag.trim(), desc.trim());
            if is_flag_token(flag) && !desc.is_empty() {
                out.push((flag.to_string(), desc.to_string()));
            }
        }
    }
    out
}

fn parse_local_desc_file(path: &Path) -> Vec<(String, String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((head, tail)) = line.split_once(':') else {
            continue;
        };
        let (head, tail) = (head.trim(), tail.trim());
        if !head.contains('/') {
            continue;
        }
        if let Some((flag, desc)) = tail.split_once(" - ") {
            let (flag, desc) = (flag.trim(), desc.trim());
            if is_flag_token(flag) && !desc.is_empty() {
                out.push((head.to_string(), flag.to_string(), desc.to_string()));
            }
        }
    }
    out
}

fn is_flag_token(flag: &str) -> bool {
    !flag.is_empty() && !flag.chars().any(|c| c.is_whitespace() || c == ':')
}

pub type Manifest = HashMap<String, u64>;

fn load_manifest_file(package_dir: &Path) -> Result<Manifest> {
    let path = package_dir.join("Manifest");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e).with_context(|| format!("Lesefehler: {}", path.display())),
    };
    let mut dist_sizes = HashMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some("DIST"), Some(name), Some(size)) = (parts.next(), parts.next(), parts.next())
            && let Ok(bytes) = size.parse::<u64>()
        {
            dist_sizes.insert(name.to_string(), bytes);
        }
    }
    Ok(dist_sizes)
}

fn load_packages_file(path: &Path) -> Result<HashMap<String, u64>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e).with_context(|| format!("Lesefehler: {}", path.display())),
    };
    let mut entries = HashMap::new();
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut last_key = String::new();

    let flush = |fields: &mut HashMap<String, String>, entries: &mut HashMap<String, u64>| {
        if let Some(cpv) = fields.remove("CPV") {
            let size = fields
                .remove("SIZE")
                .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
                .unwrap_or(0);
            entries.insert(cpv, size);
        }
        fields.clear();
    };

    for line in text.lines() {
        if line.trim().is_empty() {
            flush(&mut fields, &mut entries);
            last_key.clear();
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if !last_key.is_empty() {
                let entry = fields.entry(last_key.clone()).or_default();
                entry.push(' ');
                entry.push_str(line.trim());
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            last_key = key.trim().to_string();
            fields.insert(last_key.clone(), value.trim().to_string());
        }
    }
    flush(&mut fields, &mut entries);
    Ok(entries)
}

pub struct PkgVersion {
    pub cpn: String,
    pub cpv: String,
    pub version_str: String,
    pub version: Version,
    pub slot: String,
    pub iuse_defaults: HashMap<String, bool>,
    pub entry: CacheEntry,
    pub manifest: Manifest,
    pub binhost_size: Option<u64>,
    pub base_use: OnceLock<HashSet<String>>,
}

pub struct RepoIndex {
    pub versions: HashMap<String, Vec<PkgVersion>>,
    pub has_binhost: bool,
}

impl RepoIndex {
    pub fn load(repo_root: &Path, binhost: Option<&Path>, profile: &Profile) -> Result<Self> {
        let cache_dir = repo_root.join("metadata").join("md5-cache");
        let binhost_index = match binhost {
            Some(path) => load_packages_file(path)?,
            None => HashMap::new(),
        };

        let mut files: Vec<PathBuf> = WalkDir::new(&cache_dir)
            .min_depth(2)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.into_path())
            .filter(|p| {
                p.is_file()
                    && !p
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("Manifest"))
            })
            .collect();
        files.sort();

        let manifests = preload_manifests(repo_root, &cache_dir, &files);
        let parsed: Vec<(String, PkgVersion)> = files
            .par_iter()
            .filter_map(|path| {
                match parse_cache_file(
                    repo_root,
                    &cache_dir,
                    path,
                    &binhost_index,
                    profile,
                    &manifests,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("warnung: {path:?} uebersprungen: {e:#}");
                        None
                    }
                }
            })
            .collect();

        let mut versions: HashMap<String, Vec<PkgVersion>> = HashMap::new();
        for (cpn, pkg) in parsed {
            versions.entry(cpn).or_default().push(pkg);
        }
        for list in versions.values_mut() {
            list.sort_by(|a, b| b.version.cmp(&a.version));
        }

        let has_binhost = versions
            .values()
            .any(|l| l.first().is_some_and(|p| p.binhost_size.is_some()));

        Ok(Self {
            versions,
            has_binhost,
        })
    }

    pub fn select(&self, cpn: &str) -> Option<&PkgVersion> {
        self.versions.get(cpn)?.first()
    }

    pub fn select_for_dep(&self, dep: &Dep) -> Option<&PkgVersion> {
        let list = self.versions.get(&dep.cpn.to_string())?;
        list.iter()
            .find(|p| dep_matches_version_and_slot(dep, &p.version, &p.version_str, &p.slot))
    }

    pub fn select_cpv(&self, cpn: &str, cpv: &str) -> Option<&PkgVersion> {
        let list = self.versions.get(cpn)?;
        if cpv.is_empty() {
            return list.first();
        }
        list.iter().find(|p| p.cpv == cpv).or_else(|| list.first())
    }
}

fn split_pf(pf: &str) -> Option<(&str, &str)> {
    let split = pf
        .rmatch_indices('-')
        .map(|(i, _)| i)
        .find(|i| pf[i + 1..].starts_with(|c: char| c.is_ascii_digit()))?;
    Some((&pf[..split], &pf[split + 1..]))
}

fn preload_manifests(
    repo_root: &Path,
    cache_dir: &Path,
    files: &[PathBuf],
) -> HashMap<String, Manifest> {
    let mut pairs = Vec::new();
    for path in files {
        let Ok(rel) = path.strip_prefix(cache_dir) else {
            continue;
        };
        let category = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned());
        let pf = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let (Some(category), Some(pf)) = (category, pf) else {
            continue;
        };
        let Some((package, _)) = split_pf(&pf) else {
            continue;
        };
        pairs.push((
            format!("{category}/{package}"),
            repo_root.join(&category).join(package),
        ));
    }
    pairs.sort();
    pairs.dedup();
    pairs
        .into_par_iter()
        .filter_map(|(key, dir)| load_manifest_file(&dir).ok().map(|m| (key, m)))
        .collect()
}

fn parse_cache_file(
    repo_root: &Path,
    cache_dir: &Path,
    path: &Path,
    binhost: &HashMap<String, u64>,
    profile: &Profile,
    manifests: &HashMap<String, Manifest>,
) -> Result<Option<(String, PkgVersion)>> {
    let rel = path.strip_prefix(cache_dir)?;
    let mut parts = rel.components();
    let category = parts
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();
    let pf = parts
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();
    let (package, version_str) = split_pf(&pf)
        .ok_or_else(|| anyhow!("kein Paketname-Version-Split: {pf}"))?;
    let cpn = format!("{category}/{package}");

    let text = std::fs::read_to_string(path)?;
    let entry = CacheEntry::parse(&text).map_err(|e| anyhow!("{e}"))?;

    let live = version_str.starts_with("9999") || entry.metadata.is_live();
    let slot = full_slot(&entry.metadata.slot);
    if live
        || is_masked(&cpn, version_str, &slot, profile)
        || !keywords_accepted(&entry, profile, &cpn, version_str, &slot)
    {
        return Ok(None);
    }

    let version = Version::parse(version_str).map_err(|e| anyhow!("{e}"))?;
    let cpv = format!("{cpn}-{version_str}");
    let iuse_defaults = entry
        .metadata
        .iuse
        .iter()
        .map(|f| (f.name().to_string(), f.is_enabled_default()))
        .collect();
    let manifest = match manifests.get(&cpn) {
        Some(m) => m.clone(),
        None => load_manifest_file(&repo_root.join(&category).join(package))?,
    };
    let binhost_size = binhost.get(&cpv).copied();

    Ok(Some((
        cpn.clone(),
        PkgVersion {
            cpn,
            cpv,
            version_str: version_str.to_string(),
            version,
            slot,
            iuse_defaults,
            entry,
            manifest,
            binhost_size,
            base_use: OnceLock::new(),
        },
    )))
}

fn full_slot(slot: &Slot) -> String {
    let base = slot.slot.as_str();
    match &slot.subslot {
        Some(sub) => format!("{base}/{sub}"),
        None => format!("{base}/{base}"),
    }
}

fn is_masked(cpn: &str, version_str: &str, slot: &str, profile: &Profile) -> bool {
    let Ok(version) = Version::parse(version_str) else {
        return false;
    };
    let mut masked = false;
    for rule in &profile.package_mask_index {
        let Some(dep) = &rule.dep else { continue };
        if rule.cpn == cpn && dep_matches_version_and_slot(dep, &version, version_str, slot) {
            masked = !rule.unmask;
        }
    }
    masked
}

fn keywords_accepted(
    entry: &CacheEntry,
    profile: &Profile,
    cpn: &str,
    version_str: &str,
    slot: &str,
) -> bool {
    if let Some(rules) = profile.package_accept_keywords.get(cpn) {
        let have = Version::parse(version_str).ok();
        for (dep, kws) in rules {
            let matches = match (dep, &have) {
                (Some(dep), Some(have)) => {
                    dep_matches_version_and_slot(dep, have, version_str, slot)
                }
                (Some(_), None) => false,
                (None, _) => true,
            };
            if !matches {
                continue;
            }
            if kws.contains("**")
                || kws.contains("*")
                || entry
                    .metadata
                    .keywords
                    .iter()
                    .any(|kw| kws.contains(&kw.to_string()))
            {
                return true;
            }
        }
    }

    if entry.metadata.keywords.is_empty() {
        return false;
    }
    let testing_key = format!("~{}", profile.arch);
    for kw in &entry.metadata.keywords {
        let arch = kw.arch.as_str();
        match kw.stability {
            Stability::Stable => {
                if arch == profile.arch
                    || profile.accept_keywords.contains(arch)
                    || profile.accept_keywords.contains("*")
                {
                    return true;
                }
            }
            Stability::Testing => {
                if arch == profile.arch
                    && (profile.accept_keywords.contains(&testing_key)
                        || profile.accept_keywords.contains("~*"))
                {
                    return true;
                }
            }
            Stability::Disabled if arch == profile.arch => return false,
            _ => {}
        }
    }
    false
}
```

### `src/solver.rs`

```rust
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use anyhow::{Result, anyhow};
use portage_atom::{Dep, DepEntry, UseDefault, UseDepKind};
use portage_metadata::{RequiredUseExpr, SrcUriEntry};
use rayon::prelude::*;

use crate::repo::{PkgVersion, Profile, RepoIndex, atom_cpn};

const MAX_CASCADE_ROUNDS: u8 = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SizeMetrics {
    pub binpkg_bytes: i64,
    pub download_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLink {
    pub package: String,
    pub required_by: String,
    pub via_flag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickSwitch {
    pub label: String,
    pub changes: Vec<(String, String, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictInfo {
    pub package: String,
    pub message: String,
    pub caused_by: Vec<String>,
    pub options: Vec<QuickSwitch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternativeChoice {
    pub group_id: String,
    pub owner: String,
    pub candidates: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlagDelta {
    pub package: String,
    pub flag: String,
    pub current: bool,
    pub delta: SizeMetrics,
    pub conflict: Option<ConflictInfo>,
    pub added_packages: Vec<String>,
    pub removed_packages: Vec<String>,
    pub has_binhost_data: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BigLever {
    pub flag: String,
    pub packages: usize,
    pub affected_packages: Vec<String>,
    pub total: SizeMetrics,
    pub net_pkg_delta: i64,
    pub conflicts: usize,
    pub added_packages: Vec<String>,
    pub removed_packages: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeCost {
    pub exclusive_packages: usize,
    pub exclusive_metrics: SizeMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FlagBlocker {
    pub package: String,
    pub flag: String,
    pub want_on: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemovalSavings {
    pub metrics: SizeMetrics,
    pub removed_packages: Vec<String>,
    pub added_packages: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CutPlan {
    pub target: String,
    pub flag_blockers: Vec<FlagBlocker>,
    pub world_blockers: Vec<String>,
    pub without_flag_blockers: RemovalSavings,
    pub without_all_blockers: RemovalSavings,
}

pub type UseOverrides = HashMap<String, HashMap<String, bool>>;

pub struct Engine<'a> {
    pub index: &'a RepoIndex,
    pub profile: &'a Profile,
    pub overrides: UseOverrides,
    pub or_decisions: HashMap<String, usize>,
}

impl<'a> Engine<'a> {
    pub fn new(index: &'a RepoIndex, profile: &'a Profile) -> Self {
        Self {
            index,
            profile,
            overrides: HashMap::new(),
            or_decisions: HashMap::new(),
        }
    }

    pub fn fork(&self) -> Engine<'a> {
        Engine {
            index: self.index,
            profile: self.profile,
            overrides: self.overrides.clone(),
            or_decisions: self.or_decisions.clone(),
        }
    }

    pub fn effective_use(&self, pkg: &PkgVersion) -> HashSet<String> {
        let base = pkg.base_use.get_or_init(|| {
            self.profile.effective_defaults(
                &pkg.iuse_defaults,
                &pkg.cpn,
                &pkg.version_str,
                &pkg.slot,
            )
        });
        let Some(explicit) = self.overrides.get(&pkg.cpn) else {
            return base.clone();
        };
        let mut active = base.clone();
        for (flag, on) in explicit {
            if *on {
                active.insert(flag.clone());
            } else {
                active.remove(flag);
            }
        }
        active.retain(|f| !self.profile.use_mask.contains(f));
        for flag in &self.profile.use_force {
            if pkg.iuse_defaults.contains_key(flag) {
                active.insert(flag.clone());
            }
        }
        active
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedNode {
    pub cpv: String,
    pub binpkg: u64,
    pub download: u64,
    pub use_flags: HashSet<String>,
    pub has_binpkg: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedGraph {
    pub nodes: HashMap<String, ResolvedNode>,
    pub alternatives: Vec<AlternativeChoice>,
    pub reverse: HashMap<String, Vec<(String, Option<String>)>>,
    pub world: Vec<String>,
    pub build_extra: SizeMetrics,
    pub out_edges: HashMap<String, Vec<Dep>>,
}

impl ResolvedGraph {
    pub fn totals(&self) -> SizeMetrics {
        let mut total = SizeMetrics::default();
        for node in self.nodes.values() {
            total.binpkg_bytes = total.binpkg_bytes.saturating_add(node.binpkg as i64);
            total.download_bytes = total.download_bytes.saturating_add(node.download as i64);
        }
        total
    }

    pub fn node_costs(&self) -> HashMap<String, NodeCost> {
        let mut names: Vec<&String> = self.nodes.keys().collect();
        names.sort();
        let idx_of: HashMap<&str, usize> = names
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let n = names.len();
        let mut adj = vec![Vec::new(); n];
        for (i, name) in names.iter().enumerate() {
            if let Some(edges) = self.out_edges.get(name.as_str()) {
                for dep in edges {
                    if dep.blocker.is_none()
                        && let Some(&j) = idx_of.get(dep.cpn.to_string().as_str())
                    {
                        adj[i].push(j);
                    }
                }
            }
        }
        let world_idx: Vec<usize> = self
            .world
            .iter()
            .filter_map(|w| idx_of.get(atom_cpn(w).as_str()).copied())
            .collect();
        let mut seen = vec![0u32; n];
        let mut gen_id = 0u32;
        let mut queue = VecDeque::new();
        let mut out = HashMap::with_capacity(n);

        for cut in 0..n {
            gen_id += 1;
            queue.clear();
            for &w in &world_idx {
                if w != cut && seen[w] != gen_id {
                    seen[w] = gen_id;
                    queue.push_back(w);
                }
            }
            while let Some(u) = queue.pop_front() {
                for &v in &adj[u] {
                    if v != cut && seen[v] != gen_id {
                        seen[v] = gen_id;
                        queue.push_back(v);
                    }
                }
            }
            let mut pkgs = 0usize;
            let mut metrics = SizeMetrics::default();
            for (i, name) in names.iter().enumerate() {
                if seen[i] != gen_id {
                    let node = &self.nodes[name.as_str()];
                    pkgs += 1;
                    metrics.binpkg_bytes = metrics.binpkg_bytes.saturating_add(node.binpkg as i64);
                    metrics.download_bytes =
                        metrics.download_bytes.saturating_add(node.download as i64);
                }
            }
            out.insert(
                names[cut].clone(),
                NodeCost {
                    exclusive_packages: pkgs,
                    exclusive_metrics: metrics,
                },
            );
        }
        out
    }

    pub fn blame(&self, target: &str) -> Vec<Vec<BlameLink>> {
        let world: HashSet<String> = self.world.iter().map(|w| atom_cpn(w)).collect();
        let mut chains = Vec::new();
        let mut path = vec![target.to_string()];
        self.blame_dfs(&world, target, &mut path, &mut chains, 0);
        chains
    }

    fn blame_dfs(
        &self,
        world: &HashSet<String>,
        node: &str,
        path: &mut Vec<String>,
        chains: &mut Vec<Vec<BlameLink>>,
        depth: usize,
    ) {
        if depth > 32 || chains.len() >= 16 {
            return;
        }
        match self.reverse.get(node) {
            None => {
                if path.len() > 1 {
                    self.close_chain(path, chains);
                }
            }
            Some(parents) => {
                if world.contains(node) && path.len() > 1 {
                    self.close_chain(path, chains);
                    return;
                }
                for (parent, _) in parents {
                    if path.contains(parent) {
                        continue;
                    }
                    path.push(parent.clone());
                    self.blame_dfs(world, parent, path, chains, depth + 1);
                    path.pop();
                }
            }
        }
    }

    fn close_chain(&self, path: &[String], chains: &mut Vec<Vec<BlameLink>>) {
        if path.len() < 2 {
            return;
        }
        let links: Vec<BlameLink> = path
            .windows(2)
            .map(|w| {
                let via = self
                    .reverse
                    .get(&w[0])
                    .and_then(|ps| ps.iter().find(|(p, _)| *p == w[1]))
                    .and_then(|(_, v)| v.clone())
                    .filter(|v| !v.is_empty());
                BlameLink {
                    package: w[0].clone(),
                    required_by: w[1].clone(),
                    via_flag: via,
                }
            })
            .collect();
        if !links.is_empty() {
            chains.push(links);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CascadeOutcome {
    pub graph: ResolvedGraph,
    pub conflicts: Vec<ConflictInfo>,
}

pub fn resolve_with_cascade(
    engine: &Engine<'_>,
    world: &[String],
    include_build: bool,
    collect_blame: bool,
) -> Result<CascadeOutcome> {
    let user = engine.overrides.clone();
    let mut sim = engine.fork();
    let mut forced = HashSet::new();
    let mut rounds = 0u8;

    loop {
        let graph = resolve_graph(&sim, world, include_build, collect_blame)?;
        let (changed, conflicts) = {
            let mut step = CascadeStep::new(&mut sim, &user, &mut forced);
            step.enforce_graph(&graph);
            (step.changed, step.conflicts)
        };
        rounds += 1;
        if !changed {
            return Ok(CascadeOutcome { graph, conflicts });
        }
        if rounds >= MAX_CASCADE_ROUNDS {
            let graph = resolve_graph(&sim, world, include_build, collect_blame)?;
            let mut out_conflicts = conflicts;
            out_conflicts.push(ConflictInfo {
                package: "(kaskade)".into(),
                message: format!(
                    "Fixpunkt konvergiert nicht nach {rounds} Runden (oszillierende Constraints)"
                ),
                caused_by: Vec::new(),
                options: Vec::new(),
            });
            return Ok(CascadeOutcome {
                graph,
                conflicts: out_conflicts,
            });
        }
    }
}

fn resolve_graph(
    engine: &Engine<'_>,
    world: &[String],
    include_build: bool,
    collect_blame: bool,
) -> Result<ResolvedGraph> {
    let mut graph = ResolvedGraph {
        world: world.to_vec(),
        ..ResolvedGraph::default()
    };
    let mut queue = VecDeque::new();

    for atom in world {
        let dep = Dep::parse(atom).map_err(|e| anyhow!("Atom-Fehler in {atom}: {e}"))?;
        if dep.blocker.is_none()
            && let Some(pkg) = engine.index.select_for_dep(&dep)
        {
            queue.push_back((pkg.cpn.clone(), dep));
        }
    }

    while let Some((cpn, required_by)) = queue.pop_front() {
        if graph.nodes.contains_key(&cpn) {
            continue;
        }
        let pkg = resolve_target(engine, &cpn, &required_by)?;
        let active = engine.effective_use(pkg);
        let download = download_bytes(pkg, &active);
        graph.nodes.insert(
            cpn.clone(),
            ResolvedNode {
                cpv: pkg.cpv.clone(),
                binpkg: pkg.binhost_size.unwrap_or(0),
                download,
                use_flags: active.clone(),
                has_binpkg: pkg.binhost_size.is_some(),
            },
        );

        let mut via_map = HashMap::new();
        if collect_blame {
            collect_via_flags(pkg.entry.metadata.rdepend.list(), &active, &mut via_map, None, 0);
            collect_via_flags(pkg.entry.metadata.pdepend.list(), &active, &mut via_map, None, 0);
        }

        let mut evaluated = DepEntry::evaluate_use(pkg.entry.metadata.rdepend.list(), &active);
        evaluated.extend(DepEntry::evaluate_use(pkg.entry.metadata.pdepend.list(), &active));
        descend_runtime(
            engine,
            &evaluated,
            &cpn,
            &mut graph,
            &mut queue,
            &via_map,
            collect_blame,
        );
    }

    if include_build {
        resolve_build_extra(engine, &mut graph);
    }
    Ok(graph)
}

fn resolve_target<'b>(
    engine: &'b Engine<'b>,
    cpn: &str,
    required_by: &Dep,
) -> Result<&'b PkgVersion> {
    if required_by.cpn.to_string() == cpn
        && required_by.version.is_some()
        && let Some(pkg) = engine.index.select_for_dep(required_by)
    {
        return Ok(pkg);
    }
    engine
        .index
        .select(cpn)
        .ok_or_else(|| anyhow!("unaufloesbare Abhaengigkeit {cpn} (benoetigt von {required_by})"))
}

fn descend_runtime(
    engine: &Engine<'_>,
    entries: &[DepEntry],
    owner: &str,
    graph: &mut ResolvedGraph,
    queue: &mut VecDeque<(String, Dep)>,
    via_map: &HashMap<String, (usize, Option<String>)>,
    collect_blame: bool,
) {
    for entry in entries {
        match entry {
            DepEntry::Atom(dep) => {
                push_edge(graph, owner, dep);
                if dep.blocker.is_some() {
                    continue;
                }
                let Some(pkg) = engine.index.select_for_dep(dep) else {
                    continue;
                };
                if collect_blame {
                    let via = via_map.get(&pkg.cpn).and_then(|(_, v)| v.clone());
                    push_reverse(graph, &pkg.cpn, owner, via);
                }
                if !graph.nodes.contains_key(&pkg.cpn) {
                    queue.push_back((pkg.cpn.clone(), dep.clone()));
                }
            }
            DepEntry::AllOf(children) => {
                descend_runtime(engine, children, owner, graph, queue, via_map, collect_blame);
            }
            DepEntry::AnyOf(children) => {
                descend_any_of(engine, children, owner, graph, queue, via_map, collect_blame);
            }
            DepEntry::ExactlyOneOf(children) | DepEntry::AtMostOneOf(children) => {
                pick_first(engine, children, owner, graph, queue, via_map, collect_blame);
            }
            DepEntry::UseConditional { .. } => {}
        }
    }
}

fn descend_any_of(
    engine: &Engine<'_>,
    children: &[DepEntry],
    owner: &str,
    graph: &mut ResolvedGraph,
    queue: &mut VecDeque<(String, Dep)>,
    via_map: &HashMap<String, (usize, Option<String>)>,
    collect_blame: bool,
) {
    let mut candidates = Vec::new();
    let mut selectable = Vec::new();
    for child in children {
        let mut atoms = Vec::new();
        DepEntry::walk_atoms(std::slice::from_ref(child), &mut |d| atoms.push(d.clone()));
        for atom in atoms {
            if atom.blocker.is_some() {
                continue;
            }
            candidates.push(atom.to_string());
            if let Some(pkg) = engine.index.select_for_dep(&atom) {
                selectable.push((candidates.len() - 1, pkg.cpn.clone(), atom));
            }
        }
    }
    if selectable.is_empty() {
        return;
    }
    let group_id = format!("{owner}||{}", candidates.join(","));
    let want = engine.or_decisions.get(&group_id).copied().unwrap_or(0);
    let (sel_idx, sel_cpn, sel_atom) = selectable
        .iter()
        .find(|(i, _, _)| *i == want)
        .or(selectable.first())
        .cloned()
        .unwrap();
    push_edge(graph, owner, &sel_atom);
    if collect_blame {
        graph.alternatives.push(AlternativeChoice {
            group_id,
            owner: owner.to_string(),
            candidates,
            selected: sel_idx,
        });
        let via = via_map.get(&sel_cpn).and_then(|(_, v)| v.clone());
        push_reverse(graph, &sel_cpn, owner, via);
    }
    if !graph.nodes.contains_key(&sel_cpn) {
        queue.push_back((sel_cpn, sel_atom));
    }
}

fn pick_first(
    engine: &Engine<'_>,
    children: &[DepEntry],
    owner: &str,
    graph: &mut ResolvedGraph,
    queue: &mut VecDeque<(String, Dep)>,
    via_map: &HashMap<String, (usize, Option<String>)>,
    collect_blame: bool,
) {
    let mut atoms = Vec::new();
    DepEntry::walk_atoms(children, &mut |d| atoms.push(d.clone()));
    for atom in atoms {
        if atom.blocker.is_some() {
            continue;
        }
        if let Some(pkg) = engine.index.select_for_dep(&atom) {
            push_edge(graph, owner, &atom);
            if collect_blame {
                let via = via_map.get(&pkg.cpn).and_then(|(_, v)| v.clone());
                push_reverse(graph, &pkg.cpn, owner, via);
            }
            if !graph.nodes.contains_key(&pkg.cpn) {
                queue.push_back((pkg.cpn.clone(), atom));
            }
            return;
        }
    }
}

fn push_edge(graph: &mut ResolvedGraph, owner: &str, atom: &Dep) {
    let entry = graph.out_edges.entry(owner.to_string()).or_default();
    if !entry.contains(atom) {
        entry.push(atom.clone());
    }
}

fn push_reverse(graph: &mut ResolvedGraph, child: &str, parent: &str, via: Option<String>) {
    let entry = graph.reverse.entry(child.to_string()).or_default();
    if !entry.iter().any(|(p, v)| p == parent && *v == via) {
        entry.push((parent.to_string(), via));
    }
}

fn collect_via_flags(
    entries: &[DepEntry],
    active: &HashSet<String>,
    out: &mut HashMap<String, (usize, Option<String>)>,
    cond: Option<String>,
    depth: usize,
) {
    for entry in entries {
        match entry {
            DepEntry::Atom(dep) => match &cond {
                None => {
                    out.insert(dep.cpn.to_string(), (0, None));
                }
                Some(f) => {
                    out.entry(dep.cpn.to_string())
                        .and_modify(|(d, v)| {
                            if v.is_some() && depth >= *d {
                                *d = depth;
                                *v = Some(f.clone());
                            }
                        })
                        .or_insert((depth, Some(f.clone())));
                }
            },
            DepEntry::UseConditional {
                flag,
                negate,
                children,
            } => {
                if active.contains(flag.as_str()) != *negate {
                    let inner = if *negate {
                        format!("!{flag}")
                    } else {
                        flag.to_string()
                    };
                    collect_via_flags(children, active, out, Some(inner), depth + 1);
                }
            }
            DepEntry::AllOf(c)
            | DepEntry::AnyOf(c)
            | DepEntry::ExactlyOneOf(c)
            | DepEntry::AtMostOneOf(c) => {
                collect_via_flags(c, active, out, cond.clone(), depth);
            }
        }
    }
}

fn download_bytes(pkg: &PkgVersion, active: &HashSet<String>) -> u64 {
    let mut files = Vec::new();
    collect_dist_files(pkg.entry.metadata.src_uri.list(), active, &mut files);
    files.sort();
    files.dedup();
    files
        .iter()
        .filter_map(|f| pkg.manifest.get(f))
        .fold(0u64, |acc, &sz| acc.saturating_add(sz))
}

fn collect_dist_files(entries: &[SrcUriEntry], active: &HashSet<String>, out: &mut Vec<String>) {
    for entry in entries {
        match entry {
            SrcUriEntry::Uri { filename, .. } => out.push(filename.clone()),
            SrcUriEntry::Renamed { target, .. } => out.push(target.clone()),
            SrcUriEntry::UseConditional {
                flag,
                negated,
                entries,
            } => {
                if active.contains(flag.as_str()) != *negated {
                    collect_dist_files(entries, active, out);
                }
            }
            SrcUriEntry::Group(entries) => collect_dist_files(entries, active, out),
        }
    }
}

fn resolve_build_extra(engine: &Engine<'_>, graph: &mut ResolvedGraph) {
    let runtime: HashSet<String> = graph.nodes.keys().cloned().collect();
    let mut build: HashMap<String, String> = HashMap::new();
    let mut queue = VecDeque::new();

    for (cpn, node) in &graph.nodes {
        let Some(pkg) = engine.index.select_cpv(cpn, &node.cpv) else {
            continue;
        };
        for list in [
            pkg.entry.metadata.depend.list(),
            pkg.entry.metadata.bdepend.list(),
            pkg.entry.metadata.idepend.list(),
        ] {
            let evaluated = DepEntry::evaluate_use(list, &node.use_flags);
            let mut atoms = Vec::new();
            DepEntry::walk_atoms(&evaluated, &mut |d| atoms.push(d.clone()));
            for atom in atoms {
                if atom.blocker.is_none()
                    && let Some(target) = engine.index.select_for_dep(&atom)
                    && !runtime.contains(&target.cpn)
                    && !build.contains_key(&target.cpn)
                {
                    build.insert(target.cpn.clone(), target.cpv.clone());
                    queue.push_back((target.cpn.clone(), target.cpv.clone()));
                }
            }
        }
    }

    while let Some((cpn, cpv)) = queue.pop_front() {
        let Some(pkg) = engine.index.select_cpv(&cpn, &cpv) else {
            continue;
        };
        let active = engine.effective_use(pkg);
        let mut evaluated = DepEntry::evaluate_use(pkg.entry.metadata.rdepend.list(), &active);
        evaluated.extend(DepEntry::evaluate_use(pkg.entry.metadata.pdepend.list(), &active));
        let mut atoms = Vec::new();
        DepEntry::walk_atoms(&evaluated, &mut |d| atoms.push(d.clone()));
        for atom in atoms {
            if atom.blocker.is_none()
                && let Some(target) = engine.index.select_for_dep(&atom)
                && !runtime.contains(&target.cpn)
                && !build.contains_key(&target.cpn)
            {
                build.insert(target.cpn.clone(), target.cpv.clone());
                queue.push_back((target.cpn.clone(), target.cpv.clone()));
            }
        }
    }

    let mut extra = SizeMetrics::default();
    for (cpn, cpv) in &build {
        if let Some(pkg) = engine.index.select_cpv(cpn, cpv) {
            let active = engine.effective_use(pkg);
            extra.binpkg_bytes = extra
                .binpkg_bytes
                .saturating_add(pkg.binhost_size.unwrap_or(0) as i64);
            extra.download_bytes = extra
                .download_bytes
                .saturating_add(download_bytes(pkg, &active) as i64);
        }
    }
    graph.build_extra = extra;
}

struct CascadeStep<'e, 'a, 'u, 'f> {
    engine: &'e mut Engine<'a>,
    user: &'u UseOverrides,
    forced: &'f mut HashSet<(String, String)>,
    changed: bool,
    conflicts: Vec<ConflictInfo>,
    flag_cache: HashMap<String, HashSet<String>>,
}

impl<'e, 'a, 'u, 'f> CascadeStep<'e, 'a, 'u, 'f> {
    fn new(
        engine: &'e mut Engine<'a>,
        user: &'u UseOverrides,
        forced: &'f mut HashSet<(String, String)>,
    ) -> Self {
        Self {
            engine,
            user,
            forced,
            changed: false,
            conflicts: Vec::new(),
            flag_cache: HashMap::new(),
        }
    }

    fn cached_effective(&mut self, cpn: &str, cpv: &str) -> Option<HashSet<String>> {
        if let Some(cached) = self.flag_cache.get(cpn) {
            return Some(cached.clone());
        }
        let active = self.engine.effective_use(self.engine.index.select_cpv(cpn, cpv)?);
        self.flag_cache.insert(cpn.to_string(), active.clone());
        Some(active)
    }

    fn user_value(&self, cpn: &str, flag: &str) -> Option<bool> {
        self.user.get(cpn).and_then(|set| set.get(flag)).copied()
    }

    fn current_flags(&mut self, cpn: &str, cpv: &str, fallback: &HashSet<String>) -> HashSet<String> {
        self.cached_effective(cpn, cpv)
            .unwrap_or_else(|| fallback.clone())
    }

    fn enforce_graph(&mut self, graph: &ResolvedGraph) {
        let mut nodes: Vec<(String, String, HashSet<String>)> = graph
            .nodes
            .iter()
            .map(|(k, n)| (k.clone(), n.cpv.clone(), n.use_flags.clone()))
            .collect();
        nodes.sort_by(|a, b| a.0.cmp(&b.0));
        for (cpn, cpv, active) in &nodes {
            self.enforce_required_use(cpn, cpv, active);
        }
        for (parent, pcpv, active) in &nodes {
            self.enforce_use_deps(parent, pcpv, active, graph);
        }
    }

    fn force_flag(
        &mut self,
        cpn: &str,
        cpv: &str,
        flag: &str,
        want_on: bool,
        reason: String,
        caused_by: Vec<String>,
        fallback: &HashSet<String>,
    ) {
        if self.engine.profile.is_masked(flag) && want_on {
            self.conflicts.push(ConflictInfo {
                package: cpn.to_string(),
                message: format!("{flag} von {reason} verlangt, aber profil-maskiert"),
                caused_by,
                options: Vec::new(),
            });
            return;
        }
        if !want_on && self.engine.profile.is_forced(flag) {
            self.conflicts.push(ConflictInfo {
                package: cpn.to_string(),
                message: format!("{flag} von {reason} verboten, aber profil-erzwungen"),
                caused_by,
                options: Vec::new(),
            });
            return;
        }
        if self.user_value(cpn, flag) == Some(!want_on) {
            self.conflicts.push(ConflictInfo {
                package: cpn.to_string(),
                message: format!(
                    "{flag} soll laut {reason} {}, User-Intent ist das Gegenteil",
                    if want_on { "an" } else { "aus" }
                ),
                caused_by,
                options: vec![QuickSwitch {
                    label: format!(
                        "Kaskade uebernehmen: {flag} {}",
                        if want_on { "an" } else { "aus" }
                    ),
                    changes: vec![(cpn.to_string(), flag.to_string(), want_on)],
                }],
            });
            return;
        }
        let current = self.current_flags(cpn, cpv, fallback);
        if current.contains(flag) != want_on {
            self.engine
                .overrides
                .entry(cpn.to_string())
                .or_default()
                .insert(flag.to_string(), want_on);
            self.forced.insert((cpn.to_string(), flag.to_string()));
            self.flag_cache.remove(cpn);
            self.changed = true;
        }
    }

    fn enforce_required_use(&mut self, cpn: &str, cpv: &str, active: &HashSet<String>) {
        let Some(pkg) = self.engine.index.select_cpv(cpn, cpv) else {
            return;
        };
        let Some(expr) = &pkg.entry.metadata.required_use else {
            return;
        };
        let current = self.current_flags(cpn, cpv, active);
        let mut actions = Vec::new();
        self.enforce_expr(expr, &current, &mut actions, cpn, None);
        for action in actions {
            match action {
                ForceAction::Set(flag, want_on) => {
                    self.force_flag(
                        cpn,
                        cpv,
                        &flag,
                        want_on,
                        format!("REQUIRED_USE von {cpn}"),
                        vec![cpn.to_string()],
                        active,
                    );
                }
                ForceAction::Conflict(msg, options) => {
                    self.conflicts.push(ConflictInfo {
                        package: cpn.to_string(),
                        message: msg,
                        caused_by: vec![cpn.to_string()],
                        options,
                    });
                }
            }
        }
    }

    fn enforce_expr(
        &self,
        expr: &RequiredUseExpr,
        active: &HashSet<String>,
        out: &mut Vec<ForceAction>,
        cpn: &str,
        cond: Option<&str>,
    ) {
        match expr {
            RequiredUseExpr::Flag { name, negated } => {
                let has = active.contains(name);
                if *negated && has {
                    let mut options = vec![QuickSwitch {
                        label: format!("{name} abschalten"),
                        changes: vec![(cpn.to_string(), name.clone(), false)],
                    }];
                    if let Some(c) = cond
                        && c != name
                    {
                        options.push(QuickSwitch {
                            label: format!("Bedingung {c} abschalten"),
                            changes: vec![(cpn.to_string(), c.to_string(), false)],
                        });
                    }
                    out.push(ForceAction::Conflict(
                        format!("REQUIRED_USE verbietet {name}, ist aber an"),
                        options,
                    ));
                } else if !*negated && !has {
                    out.push(ForceAction::Set(name.clone(), true));
                }
            }
            RequiredUseExpr::UseConditional {
                flag,
                negated,
                entries,
            } => {
                if active.contains(flag) != *negated {
                    for child in entries {
                        self.enforce_expr(child, active, out, cpn, Some(flag));
                    }
                }
            }
            RequiredUseExpr::All(entries) => {
                for child in entries {
                    self.enforce_expr(child, active, out, cpn, cond);
                }
            }
            RequiredUseExpr::AnyOf(children) => {
                if !children.iter().any(|c| satisfied(c, active)) {
                    if let Some((flag, want)) = children
                        .iter()
                        .filter_map(|c| self.viable_pick(cpn, c, active))
                        .next()
                    {
                        out.push(ForceAction::Set(flag, want));
                    } else {
                        out.push(ForceAction::Conflict(
                            "REQUIRED_USE-|| unerfuellt, keine Option einschaltbar".into(),
                            children
                                .iter()
                                .map(|c| QuickSwitch {
                                    label: format!("erfuellen: {}", describe_expr(c)),
                                    changes: satisfy_changes(c, active, cpn),
                                })
                                .collect(),
                        ));
                    }
                }
            }
            RequiredUseExpr::ExactlyOne(children) => {
                let n = children.iter().filter(|c| satisfied(c, active)).count();
                if n == 0 {
                    self.enforce_expr(
                        &RequiredUseExpr::AnyOf(children.clone()),
                        active,
                        out,
                        cpn,
                        cond,
                    );
                } else if n > 1 {
                    out.push(ForceAction::Conflict(
                        "REQUIRED_USE-^^ mehrfach erfuellt (genau eines erlaubt)".into(),
                        keep_only_options(children, active, cpn),
                    ));
                }
            }
            RequiredUseExpr::AtMostOne(children) => {
                if children.iter().filter(|c| satisfied(c, active)).count() > 1 {
                    out.push(ForceAction::Conflict(
                        "REQUIRED_USE-?? mehrfach erfuellt (hoechstens eines erlaubt)".into(),
                        keep_only_options(children, active, cpn),
                    ));
                }
            }
        }
    }

    fn viable_pick(
        &self,
        cpn: &str,
        expr: &RequiredUseExpr,
        active: &HashSet<String>,
    ) -> Option<(String, bool)> {
        match expr {
            RequiredUseExpr::Flag { name, negated } => {
                if *negated {
                    (active.contains(name.as_str())
                        && self.user_value(cpn, name) != Some(true)
                        && !self.engine.profile.is_forced(name))
                    .then(|| (name.clone(), false))
                } else {
                    (!active.contains(name.as_str())
                        && !self.engine.profile.is_masked(name)
                        && self.user_value(cpn, name) != Some(false))
                    .then(|| (name.clone(), true))
                }
            }
            RequiredUseExpr::UseConditional {
                flag,
                negated,
                entries,
            } => (active.contains(flag) != *negated)
                .then(|| {
                    entries
                        .iter()
                        .filter_map(|c| self.viable_pick(cpn, c, active))
                        .next()
                })
                .flatten(),
            _ => None,
        }
    }

    fn enforce_use_deps(
        &mut self,
        parent: &str,
        parent_cpv: &str,
        parent_flags: &HashSet<String>,
        graph: &ResolvedGraph,
    ) {
        let Some(atoms) = graph.out_edges.get(parent).cloned() else {
            return;
        };
        let active = self.current_flags(parent, parent_cpv, parent_flags);
        for atom in &atoms {
            let Some(use_deps) = &atom.use_deps else {
                continue;
            };
            let target = atom.cpn.to_string();
            let Some(node) = graph.nodes.get(&target) else {
                continue;
            };
            let invert = atom.blocker.is_some();
            for ud in use_deps {
                let flag = ud.flag.to_string();
                let parent_has = active.contains(&flag);
                match ud.kind {
                    UseDepKind::Enabled => self.force_target(
                        &target,
                        &node.cpv,
                        &flag,
                        !invert,
                        parent,
                        parent_cpv,
                        &node.use_flags,
                        None,
                        ud.default,
                    ),
                    UseDepKind::Disabled => self.force_target(
                        &target,
                        &node.cpv,
                        &flag,
                        invert,
                        parent,
                        parent_cpv,
                        &node.use_flags,
                        None,
                        ud.default,
                    ),
                    UseDepKind::Conditional if parent_has => {
                        let flip = (!invert).then(|| (parent.to_string(), flag.clone(), false));
                        self.force_target(
                            &target,
                            &node.cpv,
                            &flag,
                            !invert,
                            parent,
                            parent_cpv,
                            &node.use_flags,
                            flip,
                            ud.default,
                        );
                    }
                    UseDepKind::ConditionalInverse if !parent_has => {
                        let flip = (!invert).then(|| (parent.to_string(), flag.clone(), true));
                        self.force_target(
                            &target,
                            &node.cpv,
                            &flag,
                            !invert,
                            parent,
                            parent_cpv,
                            &node.use_flags,
                            flip,
                            ud.default,
                        );
                    }
                    UseDepKind::Equal => {
                        let flip = (!invert).then(|| {
                            let fixed = self
                                .cached_effective(&target, &node.cpv)
                                .is_some_and(|a| a.contains(&flag));
                            (parent.to_string(), flag.clone(), fixed)
                        });
                        self.force_target(
                            &target,
                            &node.cpv,
                            &flag,
                            parent_has != invert,
                            parent,
                            parent_cpv,
                            &node.use_flags,
                            flip,
                            ud.default,
                        );
                    }
                    UseDepKind::EqualInverse => {
                        let flip = (!invert).then(|| {
                            let fixed = self
                                .cached_effective(&target, &node.cpv)
                                .is_some_and(|a| a.contains(&flag));
                            (parent.to_string(), flag.clone(), !fixed)
                        });
                        self.force_target(
                            &target,
                            &node.cpv,
                            &flag,
                            parent_has == invert,
                            parent,
                            parent_cpv,
                            &node.use_flags,
                            flip,
                            ud.default,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn force_target(
        &mut self,
        target: &str,
        target_cpv: &str,
        flag: &str,
        want_on: bool,
        parent: &str,
        parent_cpv: &str,
        fallback: &HashSet<String>,
        flip: Option<(String, String, bool)>,
        default: Option<UseDefault>,
    ) {
        let has_iuse = self
            .engine
            .index
            .select_cpv(target, target_cpv)
            .is_some_and(|pkg| pkg.iuse_defaults.contains_key(flag));
        if !has_iuse {
            match default {
                Some(UseDefault::Enabled) if want_on => return,
                Some(UseDefault::Disabled) if !want_on => return,
                _ => {}
            }
            let detail = match default {
                Some(_) => "Default widerspricht der Forderung",
                None => "ohne Default in IUSE",
            };
            let mut options = vec![QuickSwitch {
                label: format!(
                    "trotzdem setzen: {target}[{flag}] {} (User-Override)",
                    if want_on { "an" } else { "aus" }
                ),
                changes: vec![(target.to_string(), flag.to_string(), want_on)],
            }];
            if let Some((pcpn, pflag, need_on)) = flip {
                options.push(QuickSwitch {
                    label: format!(
                        "{parent}-Constraint lockern ({pflag} {})",
                        if need_on { "an" } else { "aus" }
                    ),
                    changes: vec![(pcpn, pflag, need_on)],
                });
            }
            self.conflicts.push(ConflictInfo {
                package: target.to_string(),
                message: format!(
                    "{parent} verlangt {target}[{flag}] = {}, aber {flag} fehlt in IUSE ({detail})",
                    if want_on { "an" } else { "aus" }
                ),
                caused_by: vec![parent.to_string(), target.to_string()],
                options,
            });
            return;
        }

        let current_on = self.current_flags(target, target_cpv, fallback).contains(flag);
        if current_on == want_on {
            return;
        }

        match self.user_value(target, flag) {
            Some(u) if u == current_on => {
                if let Some((pcpn, pflag, need_on)) = flip.clone()
                    && self.try_parent_flip(&pcpn, parent_cpv, &pflag, need_on, target, flag)
                {
                    return;
                }
                self.push_use_conflict(target, flag, want_on, parent, flip);
            }
            Some(_) => {
                self.force_flag(
                    target,
                    target_cpv,
                    flag,
                    want_on,
                    format!("{parent}-Abhaengigkeit (User-Restore)"),
                    vec![parent.to_string(), target.to_string()],
                    fallback,
                );
            }
            None if self.forced.contains(&(target.to_string(), flag.to_string())) => {
                if let Some((pcpn, pflag, need_on)) = flip.clone()
                    && self.try_parent_flip(&pcpn, parent_cpv, &pflag, need_on, target, flag)
                {
                    return;
                }
                self.push_use_conflict(target, flag, want_on, parent, flip);
            }
            None => {
                self.force_flag(
                    target,
                    target_cpv,
                    flag,
                    want_on,
                    format!("{parent}-Abhaengigkeit"),
                    vec![parent.to_string(), target.to_string()],
                    fallback,
                );
            }
        }
    }

    fn push_use_conflict(
        &mut self,
        target: &str,
        flag: &str,
        want_on: bool,
        parent: &str,
        flip: Option<(String, String, bool)>,
    ) {
        let mut options = vec![QuickSwitch {
            label: format!("{target}[{flag}] uebernehmen"),
            changes: vec![(target.to_string(), flag.to_string(), want_on)],
        }];
        if let Some((pcpn, pflag, need_on)) = flip {
            options.push(QuickSwitch {
                label: format!(
                    "{parent}-Constraint lockern ({pflag} {})",
                    if need_on { "an" } else { "aus" }
                ),
                changes: vec![(pcpn, pflag, need_on)],
            });
        }
        self.conflicts.push(ConflictInfo {
            package: target.to_string(),
            message: format!(
                "{parent} verlangt {target}[{flag}] = {}, gesetzt ist das Gegenteil",
                if want_on { "an" } else { "aus" }
            ),
            caused_by: vec![parent.to_string(), target.to_string()],
            options,
        });
    }

    fn try_parent_flip(
        &mut self,
        parent: &str,
        parent_cpv: &str,
        flag: &str,
        need_on: bool,
        target: &str,
        target_flag: &str,
    ) -> bool {
        if parent == target && flag == target_flag {
            return false;
        }
        let current_on = self
            .cached_effective(parent, parent_cpv)
            .is_some_and(|a| a.contains(flag));
        if current_on == need_on {
            return true;
        }
        if self.user_value(parent, flag) == Some(current_on)
            || (self.engine.profile.is_masked(flag) && need_on)
            || (!need_on && self.engine.profile.is_forced(flag))
        {
            return false;
        }
        self.force_flag(
            parent,
            parent_cpv,
            flag,
            need_on,
            format!("Rueckwaerts-Abgleich mit {target}"),
            vec![parent.to_string(), target.to_string()],
            &HashSet::new(),
        );
        true
    }
}

enum ForceAction {
    Set(String, bool),
    Conflict(String, Vec<QuickSwitch>),
}

fn satisfied(expr: &RequiredUseExpr, active: &HashSet<String>) -> bool {
    match expr {
        RequiredUseExpr::Flag { name, negated } => active.contains(name) != *negated,
        RequiredUseExpr::UseConditional {
            flag,
            negated,
            entries,
        } => (active.contains(flag) != *negated)
            .then(|| entries.iter().all(|c| satisfied(c, active)))
            .unwrap_or(true),
        RequiredUseExpr::All(entries) => entries.iter().all(|c| satisfied(c, active)),
        RequiredUseExpr::AnyOf(children) => children.iter().any(|c| satisfied(c, active)),
        RequiredUseExpr::ExactlyOne(children) => {
            children.iter().filter(|c| satisfied(c, active)).count() == 1
        }
        RequiredUseExpr::AtMostOne(children) => {
            children.iter().filter(|c| satisfied(c, active)).count() <= 1
        }
    }
}

fn keep_only_options(
    children: &[RequiredUseExpr],
    active: &HashSet<String>,
    cpn: &str,
) -> Vec<QuickSwitch> {
    let mut on_flags = Vec::new();
    for c in children {
        collect_on_simple(c, active, &mut on_flags);
    }
    on_flags.sort();
    on_flags.dedup();
    let mut options: Vec<QuickSwitch> = on_flags
        .iter()
        .map(|keep| QuickSwitch {
            label: format!("nur behalten: {keep}"),
            changes: on_flags
                .iter()
                .filter(|f| *f != keep)
                .map(|f| (cpn.to_string(), f.clone(), false))
                .collect(),
        })
        .filter(|o| !o.changes.is_empty())
        .collect();
    if options.is_empty() {
        options = children
            .iter()
            .filter(|c| satisfied(c, active))
            .map(|c| QuickSwitch {
                label: format!("nur behalten (komplex): {}", describe_expr(c)),
                changes: break_changes(c, active, cpn),
            })
            .filter(|o| !o.changes.is_empty())
            .collect();
    }
    options
}

fn collect_on_simple(expr: &RequiredUseExpr, active: &HashSet<String>, out: &mut Vec<String>) {
    match expr {
        RequiredUseExpr::Flag { name, negated } => {
            if !negated && active.contains(name.as_str()) {
                out.push(name.clone());
            }
        }
        RequiredUseExpr::UseConditional {
            flag,
            negated,
            entries,
        } => {
            if active.contains(flag) != *negated {
                for c in entries {
                    collect_on_simple(c, active, out);
                }
            }
        }
        RequiredUseExpr::All(entries) => {
            for c in entries {
                collect_on_simple(c, active, out);
            }
        }
        RequiredUseExpr::AnyOf(children)
        | RequiredUseExpr::ExactlyOne(children)
        | RequiredUseExpr::AtMostOne(children) => {
            for c in children {
                if satisfied(c, active) {
                    collect_on_simple(c, active, out);
                }
            }
        }
    }
}

fn break_changes(
    expr: &RequiredUseExpr,
    active: &HashSet<String>,
    cpn: &str,
) -> Vec<(String, String, bool)> {
    match expr {
        RequiredUseExpr::Flag { name, negated } => {
            if active.contains(name.as_str()) == !negated {
                vec![(cpn.to_string(), name.clone(), *negated)]
            } else {
                Vec::new()
            }
        }
        RequiredUseExpr::UseConditional {
            flag,
            negated,
            entries,
        } => {
            if active.contains(flag) != *negated {
                entries
                    .iter()
                    .map(|c| break_changes(c, active, cpn))
                    .find(|v| !v.is_empty())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        RequiredUseExpr::All(entries) => entries
            .iter()
            .map(|c| break_changes(c, active, cpn))
            .find(|v| !v.is_empty())
            .unwrap_or_default(),
        RequiredUseExpr::AnyOf(children)
        | RequiredUseExpr::ExactlyOne(children)
        | RequiredUseExpr::AtMostOne(children) => children
            .iter()
            .filter(|c| satisfied(c, active))
            .flat_map(|c| break_changes(c, active, cpn))
            .collect(),
    }
}

fn satisfy_changes(
    expr: &RequiredUseExpr,
    active: &HashSet<String>,
    cpn: &str,
) -> Vec<(String, String, bool)> {
    match expr {
        RequiredUseExpr::Flag { name, negated } => {
            if active.contains(name.as_str()) == !negated {
                Vec::new()
            } else {
                vec![(cpn.to_string(), name.clone(), !negated)]
            }
        }
        RequiredUseExpr::UseConditional {
            flag,
            negated,
            entries,
        } => {
            if active.contains(flag) != *negated {
                entries
                    .iter()
                    .flat_map(|c| satisfy_changes(c, active, cpn))
                    .collect()
            } else {
                Vec::new()
            }
        }
        RequiredUseExpr::All(entries) => entries
            .iter()
            .flat_map(|c| satisfy_changes(c, active, cpn))
            .collect(),
        RequiredUseExpr::AnyOf(children)
        | RequiredUseExpr::ExactlyOne(children)
        | RequiredUseExpr::AtMostOne(children) => children
            .iter()
            .map(|c| satisfy_changes(c, active, cpn))
            .find(|v| !v.is_empty())
            .unwrap_or_default(),
    }
}

fn describe_expr(expr: &RequiredUseExpr) -> String {
    match expr {
        RequiredUseExpr::Flag { name, negated } => {
            format!("{}{name}", if *negated { "!" } else { "" })
        }
        RequiredUseExpr::AnyOf(_) => "|| (...)".into(),
        RequiredUseExpr::ExactlyOne(_) => "^^ (...)".into(),
        RequiredUseExpr::AtMostOne(_) => "?? (...)".into(),
        RequiredUseExpr::UseConditional { flag, negated, .. } => {
            format!("{}{flag}? (...)", if *negated { "!" } else { "" })
        }
        RequiredUseExpr::All(_) => "(...)".into(),
    }
}

pub fn package_deltas_with_base(
    engine: &Engine<'_>,
    world: &[String],
    package: &str,
    base: &CascadeOutcome,
) -> Result<Vec<FlagDelta>> {
    let mut candidates = BTreeSet::new();
    if let Some(node) = base.graph.nodes.get(package)
        && let Some(pkg) = engine.index.select_cpv(package, &node.cpv)
    {
        candidates.extend(pkg.iuse_defaults.keys().cloned());
    }
    candidates.extend(current_from_base(base, engine, package));
    candidates.retain(|f| {
        !engine.profile.is_masked(f)
            && !engine.profile.is_platform_flag(f)
            && !engine.profile.is_hidden_expand_flag(f)
    });

    let flags: Vec<String> = candidates.into_iter().collect();
    let mut out = flags
        .into_par_iter()
        .map(|flag| flag_delta(engine, world, package, &flag, base))
        .collect::<Result<Vec<_>>>()?;
    out.sort_by(|a, b| a.flag.cmp(&b.flag));
    Ok(out)
}

fn flag_delta(
    engine: &Engine<'_>,
    world: &[String],
    package: &str,
    flag: &str,
    before: &CascadeOutcome,
) -> Result<FlagDelta> {
    let current = current_from_base(before, engine, package);
    let is_on = current.contains(flag);

    if !flag_can_affect_graph(&before.graph, engine, package, flag) {
        return Ok(FlagDelta {
            package: package.to_string(),
            flag: flag.to_string(),
            current: is_on,
            delta: SizeMetrics::default(),
            conflict: None,
            added_packages: Vec::new(),
            removed_packages: Vec::new(),
            has_binhost_data: engine.index.has_binhost,
        });
    }

    let mut sim = engine.fork();
    sim.overrides
        .entry(package.to_string())
        .or_default()
        .insert(flag.to_string(), !is_on);
    let outcome = resolve_with_cascade(&sim, world, false, false)?;
    let before_totals = before.graph.totals();
    let new_totals = outcome.graph.totals();
    let (added, removed) = diff_packages(&before.graph, &outcome.graph);
    let conflict = outcome.conflicts.into_iter().find(|c| {
        !before
            .conflicts
            .iter()
            .any(|b| b.package == c.package && b.message == c.message)
    });

    Ok(FlagDelta {
        package: package.to_string(),
        flag: flag.to_string(),
        current: is_on,
        delta: SizeMetrics {
            binpkg_bytes: new_totals.binpkg_bytes - before_totals.binpkg_bytes,
            download_bytes: new_totals.download_bytes - before_totals.download_bytes,
        },
        conflict,
        added_packages: added,
        removed_packages: removed,
        has_binhost_data: engine.index.has_binhost,
    })
}

fn current_from_base(
    base: &CascadeOutcome,
    engine: &Engine<'_>,
    package: &str,
) -> HashSet<String> {
    match base.graph.nodes.get(package) {
        Some(node) => node.use_flags.clone(),
        None => engine
            .index
            .select(package)
            .map(|p| engine.effective_use(p))
            .unwrap_or_default(),
    }
}

fn flag_can_affect_graph(
    base: &ResolvedGraph,
    engine: &Engine<'_>,
    package: &str,
    flag: &str,
) -> bool {
    let Some(node) = base.nodes.get(package) else {
        return true;
    };
    let Some(pkg) = engine.index.select_cpv(package, &node.cpv) else {
        return true;
    };
    let md = &pkg.entry.metadata;
    if dep_entries_mention_flag(md.rdepend.list(), flag)
        || dep_entries_mention_flag(md.pdepend.list(), flag)
        || src_uri_entries_mention_flag(md.src_uri.list(), flag)
        || md
            .required_use
            .as_ref()
            .is_some_and(|r| required_use_mentions_flag(r, flag))
    {
        return true;
    }
    base.out_edges.values().flatten().any(|atom| {
        atom.cpn.to_string() == package
            && atom
                .use_deps
                .as_ref()
                .is_some_and(|uds| uds.iter().any(|u| u.flag.as_str() == flag))
    })
}

fn dep_entries_mention_flag(entries: &[DepEntry], flag: &str) -> bool {
    entries.iter().any(|entry| match entry {
        DepEntry::Atom(dep) => dep
            .use_deps
            .as_ref()
            .is_some_and(|uds| uds.iter().any(|u| u.flag.as_str() == flag)),
        DepEntry::UseConditional {
            flag: cond,
            children,
            ..
        } => cond.as_str() == flag || dep_entries_mention_flag(children, flag),
        DepEntry::AllOf(c)
        | DepEntry::AnyOf(c)
        | DepEntry::ExactlyOneOf(c)
        | DepEntry::AtMostOneOf(c) => dep_entries_mention_flag(c, flag),
    })
}

fn src_uri_entries_mention_flag(entries: &[SrcUriEntry], flag: &str) -> bool {
    entries.iter().any(|entry| match entry {
        SrcUriEntry::UseConditional {
            flag: cond,
            entries,
            ..
        } => cond == flag || src_uri_entries_mention_flag(entries, flag),
        SrcUriEntry::Group(entries) => src_uri_entries_mention_flag(entries, flag),
        SrcUriEntry::Uri { .. } | SrcUriEntry::Renamed { .. } => false,
    })
}

fn required_use_mentions_flag(expr: &RequiredUseExpr, flag: &str) -> bool {
    match expr {
        RequiredUseExpr::Flag { name, .. } => name == flag,
        RequiredUseExpr::UseConditional {
            flag: cond,
            entries,
            ..
        } => cond == flag || entries.iter().any(|e| required_use_mentions_flag(e, flag)),
        RequiredUseExpr::AnyOf(e)
        | RequiredUseExpr::ExactlyOne(e)
        | RequiredUseExpr::AtMostOne(e)
        | RequiredUseExpr::All(e) => e.iter().any(|x| required_use_mentions_flag(x, flag)),
    }
}

fn diff_packages(before: &ResolvedGraph, after: &ResolvedGraph) -> (Vec<String>, Vec<String>) {
    let mut added: Vec<String> = after
        .nodes
        .keys()
        .filter(|k| !before.nodes.contains_key(*k))
        .cloned()
        .collect();
    let mut removed: Vec<String> = before
        .nodes
        .keys()
        .filter(|k| !after.nodes.contains_key(*k))
        .cloned()
        .collect();
    added.sort();
    removed.sort();
    (added, removed)
}

pub fn big_levers_with_base(
    engine: &Engine<'_>,
    world: &[String],
    base: &CascadeOutcome,
) -> Result<Vec<BigLever>> {
    let base_totals = base.graph.totals();
    let mut flag_packages: HashMap<String, Vec<String>> = HashMap::new();

    for (cpn, node) in &base.graph.nodes {
        for flag in &node.use_flags {
            if engine.profile.is_platform_flag(flag)
                || engine.profile.is_masked(flag)
                || engine.profile.is_forced(flag)
                || engine.profile.is_hidden_expand_flag(flag)
            {
                continue;
            }
            flag_packages
                .entry(flag.clone())
                .or_default()
                .push(cpn.clone());
        }
    }

    let mut flags: Vec<String> = flag_packages.keys().cloned().collect();
    flags.sort();

    let mut levers = flags
        .into_par_iter()
        .map(|flag| -> Result<BigLever> {
            let mut affected = flag_packages[&flag].clone();
            affected.sort();
            if affected
                .iter()
                .all(|cpn| !flag_can_affect_graph(&base.graph, engine, cpn, &flag))
            {
                return Ok(BigLever {
                    flag,
                    packages: affected.len(),
                    affected_packages: affected,
                    total: SizeMetrics::default(),
                    net_pkg_delta: 0,
                    conflicts: base.conflicts.len(),
                    added_packages: Vec::new(),
                    removed_packages: Vec::new(),
                });
            }

            let mut sim = engine.fork();
            for cpn in &affected {
                sim.overrides
                    .entry(cpn.clone())
                    .or_default()
                    .insert(flag.clone(), false);
            }
            let outcome = resolve_with_cascade(&sim, world, false, false)?;
            let totals = outcome.graph.totals();
            let (added, removed) = diff_packages(&base.graph, &outcome.graph);
            let net_pkg_delta = added.len() as i64 - removed.len() as i64;
            Ok(BigLever {
                flag,
                packages: affected.len(),
                affected_packages: affected,
                total: SizeMetrics {
                    binpkg_bytes: totals.binpkg_bytes - base_totals.binpkg_bytes,
                    download_bytes: totals.download_bytes - base_totals.download_bytes,
                },
                net_pkg_delta,
                conflicts: outcome.conflicts.len(),
                added_packages: added,
                removed_packages: removed,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let has_bin = engine.index.has_binhost;
    levers.sort_by(|a, b| {
        if has_bin {
            a.total
                .binpkg_bytes
                .cmp(&b.total.binpkg_bytes)
                .then_with(|| a.net_pkg_delta.cmp(&b.net_pkg_delta))
        } else {
            a.net_pkg_delta.cmp(&b.net_pkg_delta)
        }
        .then_with(|| a.total.download_bytes.cmp(&b.total.download_bytes))
        .then_with(|| a.flag.cmp(&b.flag))
    });
    Ok(levers)
}

pub fn shared_flag_deps(
    engine: &Engine<'_>,
    graph: &ResolvedGraph,
    package: &str,
    flag: &str,
) -> Vec<(String, Vec<String>)> {
    let Some(node) = graph.nodes.get(package) else {
        return Vec::new();
    };
    let Some(pkg) = engine.index.select_cpv(package, &node.cpv) else {
        return Vec::new();
    };
    let mut targets = BTreeSet::new();
    collect_conditional_targets(
        pkg.entry.metadata.rdepend.list(),
        &node.use_flags,
        flag,
        false,
        &mut targets,
    );
    collect_conditional_targets(
        pkg.entry.metadata.pdepend.list(),
        &node.use_flags,
        flag,
        false,
        &mut targets,
    );

    let mut out = Vec::new();
    for dep_cpn in targets {
        if dep_cpn == package || !graph.nodes.contains_key(&dep_cpn) {
            continue;
        }
        if let Some(parents) = graph.reverse.get(&dep_cpn) {
            let mut others: Vec<String> = parents
                .iter()
                .filter(|(p, _)| p != package)
                .map(|(p, _)| p.clone())
                .collect();
            others.sort();
            others.dedup();
            if !others.is_empty() {
                out.push((dep_cpn, others));
            }
        }
    }
    out
}

fn collect_conditional_targets(
    entries: &[DepEntry],
    active: &HashSet<String>,
    target_flag: &str,
    inside_target: bool,
    out: &mut BTreeSet<String>,
) {
    for entry in entries {
        match entry {
            DepEntry::Atom(dep) => {
                if inside_target && dep.blocker.is_none() {
                    out.insert(dep.cpn.to_string());
                }
            }
            DepEntry::UseConditional {
                flag,
                negate,
                children,
            } => {
                if active.contains(flag.as_str()) != *negate {
                    collect_conditional_targets(
                        children,
                        active,
                        target_flag,
                        inside_target || flag.as_str() == target_flag,
                        out,
                    );
                }
            }
            DepEntry::AllOf(c)
            | DepEntry::AnyOf(c)
            | DepEntry::ExactlyOneOf(c)
            | DepEntry::AtMostOneOf(c) => {
                collect_conditional_targets(c, active, target_flag, inside_target, out);
            }
        }
    }
}

pub fn cut_package(
    engine: &Engine<'_>,
    world: &[String],
    base: &CascadeOutcome,
    target: &str,
) -> Result<Option<CutPlan>> {
    if !base.graph.nodes.contains_key(target) {
        return Ok(None);
    }
    let world_cpns: HashSet<String> = world.iter().map(|w| atom_cpn(w)).collect();
    let mut flag_blockers = BTreeSet::new();
    let mut world_blockers = BTreeSet::new();
    let mut visited = HashSet::new();
    collect_blockers_up(
        &base.graph,
        engine,
        &world_cpns,
        target,
        &mut flag_blockers,
        &mut world_blockers,
        &mut visited,
    );
    let flag_blockers: Vec<FlagBlocker> = flag_blockers.into_iter().collect();
    let world_blockers: Vec<String> = world_blockers.into_iter().collect();

    let without_flag_blockers = simulate_cut(engine, world, &base.graph, &flag_blockers, &[])?;
    let without_all_blockers =
        simulate_cut(engine, world, &base.graph, &flag_blockers, &world_blockers)?;

    Ok(Some(CutPlan {
        target: target.to_string(),
        flag_blockers,
        world_blockers,
        without_flag_blockers,
        without_all_blockers,
    }))
}

fn collect_blockers_up(
    graph: &ResolvedGraph,
    engine: &Engine<'_>,
    world_cpns: &HashSet<String>,
    dep: &str,
    flag_blockers: &mut BTreeSet<FlagBlocker>,
    world_blockers: &mut BTreeSet<String>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(dep.to_string()) {
        return;
    }
    if world_cpns.contains(dep) {
        world_blockers.insert(dep.to_string());
    }
    let Some(parents) = graph.reverse.get(dep) else {
        return;
    };
    for (parent, via) in parents {
        if let Some(via_str) = via
            && let Some((flag, want_on)) = parse_breakable_flag(engine, graph, parent, via_str)
        {
            flag_blockers.insert(FlagBlocker {
                package: parent.clone(),
                flag,
                want_on,
            });
            continue;
        }
        collect_blockers_up(
            graph,
            engine,
            world_cpns,
            parent,
            flag_blockers,
            world_blockers,
            visited,
        );
    }
}

fn parse_breakable_flag(
    engine: &Engine<'_>,
    graph: &ResolvedGraph,
    parent: &str,
    via: &str,
) -> Option<(String, bool)> {
    let (flag, want_on) = match via.strip_prefix('!') {
        Some(f) => (f, true),
        None => (via, false),
    };
    if engine.profile.is_platform_flag(flag)
        || engine.profile.is_hidden_expand_flag(flag)
        || (want_on && engine.profile.is_masked(flag))
        || (!want_on && engine.profile.is_forced(flag))
    {
        return None;
    }
    let node = graph.nodes.get(parent)?;
    let pkg = engine.index.select_cpv(parent, &node.cpv)?;
    pkg.iuse_defaults
        .contains_key(flag)
        .then(|| (flag.to_string(), want_on))
}

fn simulate_cut(
    engine: &Engine<'_>,
    world: &[String],
    before: &ResolvedGraph,
    flags: &[FlagBlocker],
    drop_world: &[String],
) -> Result<RemovalSavings> {
    let mut sim = engine.fork();
    for fb in flags {
        sim.overrides
            .entry(fb.package.clone())
            .or_default()
            .insert(fb.flag.clone(), fb.want_on);
    }
    let filtered_world: Vec<String> = if drop_world.is_empty() {
        world.to_vec()
    } else {
        world
            .iter()
            .filter(|w| !drop_world.contains(&atom_cpn(w)))
            .cloned()
            .collect()
    };
    let after = resolve_with_cascade(&sim, &filtered_world, false, false)?;
    let before_totals = before.totals();
    let after_totals = after.graph.totals();
    let (added, removed) = diff_packages(before, &after.graph);
    Ok(RemovalSavings {
        metrics: SizeMetrics {
            binpkg_bytes: after_totals.binpkg_bytes - before_totals.binpkg_bytes,
            download_bytes: after_totals.download_bytes - before_totals.download_bytes,
        },
        removed_packages: removed,
        added_packages: added,
    })
}
```

### `src/tui.rs`

```rust
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Cell as TableCell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
    TableState, Tabs, Wrap,
};
use ratatui::{DefaultTerminal, Frame};

use crate::repo::{Profile, RepoIndex, UseDescriptions, atom_cpn};
use crate::solver::{
    BigLever, BlameLink, CascadeOutcome, ConflictInfo, CutPlan, Engine, FlagDelta, NodeCost,
    QuickSwitch, ResolvedGraph, UseOverrides, big_levers_with_base, cut_package,
    package_deltas_with_base, resolve_with_cascade, shared_flag_deps,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Packages,
    BigLevers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Pane {
    #[default]
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LeftSort {
    #[default]
    Default,
    ExclusivePackages,
    ExclusiveSize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LeftEntry {
    WorldPkg(String),
    OrDecision(String),
    Transitive(String),
}

enum Modal {
    Conflict { conflict: usize, selected: usize },
    SimulatedConflict { conflict: ConflictInfo, selected: usize },
    Help,
}

pub struct AppState<'a> {
    engine: Engine<'a>,
    world: Vec<String>,
    graph: ResolvedGraph,
    conflicts: Vec<ConflictInfo>,
    last_outcome: Option<CascadeOutcome>,
    right_targets: Vec<String>,
    right_target_selected: Option<usize>,
    cut_cache: HashMap<String, CutPlan>,
    cut_loading: bool,
    viewport_height: u16,
    middle_scroll: Cell<u16>,
    node_costs: HashMap<String, NodeCost>,
    left_sort: LeftSort,
    cut_plan: Option<CutPlan>,
    undo_stack: Vec<UseOverrides>,
    use_descriptions: UseDescriptions,
    tab: Tab,
    pane: Pane,
    left: Vec<LeftEntry>,
    left_selected: usize,
    flag_deltas: Vec<FlagDelta>,
    middle_selected: usize,
    levers: Vec<BigLever>,
    lever_selected: usize,
    filter: String,
    filtering: bool,
    modal: Option<Modal>,
    dirty: bool,
    status: String,
    left_list_state: ListState,
    lever_table_state: TableState,
    right_scroll: u16,
    levers_dirty: bool,
    deltas_loading: bool,
    delta_cache: HashMap<(String, String), Vec<FlagDelta>>,
}

impl<'a> AppState<'a> {
    pub fn new(index: &'a RepoIndex, profile: &'a Profile, world: Vec<String>) -> Result<Self> {
        let mut state = Self {
            engine: Engine::new(index, profile),
            world,
            graph: ResolvedGraph::default(),
            conflicts: Vec::new(),
            last_outcome: None,
            right_targets: Vec::new(),
            right_target_selected: None,
            cut_cache: HashMap::new(),
            cut_loading: false,
            viewport_height: 24,
            middle_scroll: Cell::new(0),
            node_costs: HashMap::new(),
            left_sort: LeftSort::Default,
            cut_plan: None,
            undo_stack: Vec::new(),
            use_descriptions: UseDescriptions::load(&profile.profiles_dir),
            tab: Tab::Packages,
            pane: Pane::Left,
            left: Vec::new(),
            left_selected: 0,
            flag_deltas: Vec::new(),
            middle_selected: 0,
            levers: Vec::new(),
            lever_selected: 0,
            filter: String::new(),
            filtering: false,
            modal: None,
            dirty: false,
            status: String::new(),
            left_list_state: ListState::default(),
            lever_table_state: TableState::default(),
            right_scroll: 0,
            levers_dirty: true,
            deltas_loading: false,
            delta_cache: HashMap::new(),
        };
        state.refresh()?;
        Ok(state)
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.engine.overrides.clone());
        if self.undo_stack.len() > 32 {
            self.undo_stack.remove(0);
        }
    }

    fn pop_undo(&mut self) -> Result<bool> {
        let Some(prev) = self.undo_stack.pop() else {
            return Ok(false);
        };
        self.engine.overrides = prev;
        self.dirty = true;
        self.refresh()?;
        Ok(true)
    }

    fn refresh(&mut self) -> Result<()> {
        let prev_entry = self.left.get(self.left_selected).cloned();
        let outcome = resolve_with_cascade(&self.engine, &self.world, true, true)?;
        let levers = (self.tab == Tab::BigLevers)
            .then(|| big_levers_with_base(&self.engine, &self.world, &outcome).unwrap_or_default());
        self.last_outcome = Some(outcome.clone());
        self.graph = outcome.graph;
        self.conflicts = outcome.conflicts;
        self.node_costs = self.graph.node_costs();
        self.cut_plan = None;
        self.cut_cache.clear();
        self.cut_loading = false;
        self.delta_cache.clear();
        if self.status.starts_with("package.use geschrieben:") {
            self.status.clear();
        }
        self.rebuild_left_preserving(prev_entry.as_ref());
        self.rebuild_middle()?;
        self.rebuild_right_targets();
        match levers {
            Some(levers) => {
                self.levers = levers;
                self.levers_dirty = false;
                self.left_list_state.select(Some(self.left_selected));
                self.lever_table_state.select(Some(self.lever_selected));
            }
            None => self.levers_dirty = true,
        }
        Ok(())
    }

    fn set_tab(&mut self, tab: Tab) -> Result<()> {
        self.tab = tab;
        if tab == Tab::BigLevers && self.levers_dirty {
            if let Some(base) = &self.last_outcome {
                self.levers =
                    big_levers_with_base(&self.engine, &self.world, base).unwrap_or_default();
            }
            self.levers_dirty = false;
            self.left_list_state.select(Some(self.left_selected));
            self.lever_table_state.select(Some(self.lever_selected));
        }
        Ok(())
    }

    fn cycle_pane(&mut self) {
        self.pane = match self.pane {
            Pane::Left => Pane::Middle,
            Pane::Middle => Pane::Right,
            Pane::Right => Pane::Left,
        };
        if self.pane == Pane::Right {
            if self.right_target_selected.is_none() && !self.right_targets.is_empty() {
                self.right_target_selected = Some(0);
            }
            self.preview_cut_for_selected_right_target();
        }
    }

    fn preview_cut_for_selected_right_target(&mut self) {
        let Some(target) = self.selected_right_target().map(str::to_string) else {
            return;
        };
        if let Some(cached) = self.cut_cache.get(&target) {
            self.cut_plan = Some(cached.clone());
            self.cut_loading = false;
        } else {
            self.cut_loading = true;
        }
    }

    fn selected_package(&self) -> Option<String> {
        match self.left.get(self.left_selected)? {
            LeftEntry::WorldPkg(cpn) | LeftEntry::Transitive(cpn) => Some(cpn.clone()),
            LeftEntry::OrDecision(_) => None,
        }
    }

    fn selected_or_group(&self) -> Option<String> {
        match self.left.get(self.left_selected)? {
            LeftEntry::OrDecision(id) => Some(id.clone()),
            _ => None,
        }
    }

    fn selected_right_target(&self) -> Option<&str> {
        self.right_target_selected
            .and_then(|i| self.right_targets.get(i))
            .map(String::as_str)
    }

    fn selected_blame(&self) -> Vec<Vec<BlameLink>> {
        match self.selected_package() {
            Some(p) => self.graph.blame(&p),
            None => Vec::new(),
        }
    }

    fn page_step(&self) -> i32 {
        (self.viewport_height as i32 - 9).max(3)
    }

    fn move_clamped(cur: usize, delta: i32, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        (cur as i32 + delta).clamp(0, (len - 1) as i32) as usize
    }

    fn middle_len(&self) -> usize {
        match self.left.get(self.left_selected) {
            Some(LeftEntry::OrDecision(group)) => self
                .graph
                .alternatives
                .iter()
                .find(|a| &a.group_id == group)
                .map(|a| a.candidates.len())
                .unwrap_or(0),
            Some(LeftEntry::WorldPkg(_)) | Some(LeftEntry::Transitive(_)) => self.flag_deltas.len(),
            None => 0,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.tab == Tab::BigLevers {
            if !self.levers.is_empty() {
                self.lever_selected =
                    Self::move_clamped(self.lever_selected, delta, self.levers.len());
                self.lever_table_state.select(Some(self.lever_selected));
            }
            return;
        }
        match self.pane {
            Pane::Left => {
                if !self.left.is_empty() {
                    let next = Self::move_clamped(self.left_selected, delta, self.left.len());
                    if next != self.left_selected || self.flag_deltas.is_empty() {
                        self.select_left_fast(next);
                    }
                }
            }
            Pane::Middle => {
                let len = self.middle_len();
                if len > 0 {
                    self.middle_selected = Self::move_clamped(self.middle_selected, delta, len);
                    self.rebuild_right_targets();
                    self.cut_plan = None;
                    self.right_scroll = 0;
                }
            }
            Pane::Right => {
                if !self.right_targets.is_empty() && delta.abs() == 1 {
                    let cur = self.right_target_selected.unwrap_or(0);
                    self.right_target_selected =
                        Some(Self::move_clamped(cur, delta, self.right_targets.len()));
                    self.preview_cut_for_selected_right_target();
                } else if delta < 0 {
                    self.right_scroll = self.right_scroll.saturating_sub((-delta) as u16);
                } else {
                    self.right_scroll = self.right_scroll.saturating_add(delta as u16);
                }
            }
        }
    }

    fn move_to_edge(&mut self, to_end: bool) {
        if self.tab == Tab::BigLevers {
            if !self.levers.is_empty() {
                self.lever_selected = if to_end { self.levers.len() - 1 } else { 0 };
                self.lever_table_state.select(Some(self.lever_selected));
            }
            return;
        }
        match self.pane {
            Pane::Left if !self.left.is_empty() => {
                self.select_left_fast(if to_end { self.left.len() - 1 } else { 0 });
            }
            Pane::Middle => {
                let len = self.middle_len();
                if len > 0 {
                    self.middle_selected = if to_end { len - 1 } else { 0 };
                    self.rebuild_right_targets();
                    self.cut_plan = None;
                    self.right_scroll = 0;
                }
            }
            Pane::Right => {
                if !self.right_targets.is_empty() {
                    self.right_target_selected = Some(if to_end {
                        self.right_targets.len() - 1
                    } else {
                        0
                    });
                    self.preview_cut_for_selected_right_target();
                } else {
                    self.right_scroll = if to_end { u16::MAX / 2 } else { 0 };
                }
            }
            _ => {}
        }
    }

    fn select_left_fast(&mut self, idx: usize) {
        if self.left.is_empty() {
            self.flag_deltas.clear();
            self.deltas_loading = false;
            self.middle_selected = 0;
            self.right_scroll = 0;
            self.middle_scroll.set(0);
            self.cut_plan = None;
            self.rebuild_right_targets();
            return;
        }
        self.left_selected = idx.min(self.left.len() - 1);
        self.left_list_state.select(Some(self.left_selected));
        match self.left.get(self.left_selected).cloned() {
            Some(LeftEntry::WorldPkg(cpn)) | Some(LeftEntry::Transitive(cpn)) => {
                let key = (cpn, overrides_fingerprint(&self.engine));
                if let Some(cached) = self.delta_cache.get(&key) {
                    self.flag_deltas = cached.clone();
                    self.deltas_loading = false;
                } else {
                    self.flag_deltas.clear();
                    self.deltas_loading = true;
                }
            }
            _ => {
                self.flag_deltas.clear();
                self.deltas_loading = false;
            }
        }
        self.middle_selected = 0;
        self.right_scroll = 0;
        self.middle_scroll.set(0);
        self.cut_plan = None;
        self.rebuild_right_targets();
    }

    fn rebuild_left_preserving(&mut self, prev_entry: Option<&LeftEntry>) {
        let mut world_entries = Vec::new();
        let mut seen = HashSet::new();
        for atom in &self.world {
            let cpn = atom_cpn(atom);
            if self.graph.nodes.contains_key(&cpn) && seen.insert(cpn.clone()) {
                world_entries.push(LeftEntry::WorldPkg(cpn));
            }
        }
        let mut or_entries: Vec<LeftEntry> = self
            .graph
            .alternatives
            .iter()
            .map(|a| LeftEntry::OrDecision(a.group_id.clone()))
            .collect();
        let mut transitive: Vec<String> = self
            .graph
            .nodes
            .keys()
            .filter(|cpn| !seen.contains(*cpn))
            .cloned()
            .collect();

        match self.left_sort {
            LeftSort::Default => {
                transitive.sort_by_key(|cpn| {
                    std::cmp::Reverse(self.graph.nodes.get(cpn).map(|n| n.binpkg).unwrap_or(0))
                });
            }
            LeftSort::ExclusivePackages => {
                let cmp_pkg = |a: &str, b: &str| {
                    let ca = self.node_costs.get(a);
                    let cb = self.node_costs.get(b);
                    cb.map(|c| c.exclusive_packages)
                        .cmp(&ca.map(|c| c.exclusive_packages))
                        .then_with(|| {
                            cb.map(|c| c.exclusive_metrics.download_bytes)
                                .cmp(&ca.map(|c| c.exclusive_metrics.download_bytes))
                        })
                        .then_with(|| a.cmp(b))
                };
                transitive.sort_by(|a, b| cmp_pkg(a, b));
                world_entries.sort_by(|a, b| cmp_pkg(&left_label(a), &left_label(b)));
            }
            LeftSort::ExclusiveSize => {
                let has_bin = self.engine.index.has_binhost;
                let key = |cpn: &str| -> (i64, String) {
                    match self.node_costs.get(cpn) {
                        Some(c) if has_bin => (c.exclusive_metrics.binpkg_bytes, cpn.to_string()),
                        Some(c) => (c.exclusive_metrics.download_bytes, cpn.to_string()),
                        None => (0, cpn.to_string()),
                    }
                };
                transitive.sort_by_key(|a| std::cmp::Reverse(key(a)));
                world_entries.sort_by(|a, b| key(&left_label(b)).cmp(&key(&left_label(a))));
            }
        }

        let mut left = Vec::new();
        left.extend(world_entries);
        left.append(&mut or_entries);
        for cpn in transitive {
            left.push(LeftEntry::Transitive(cpn));
        }
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            left.retain(|e| left_label(e).to_lowercase().contains(&f));
        }
        self.left = left;
        if let Some(prev) = prev_entry
            && let Some(pos) = self.left.iter().position(|e| e == prev)
        {
            self.left_selected = pos;
        } else {
            self.left_selected = self.left_selected.min(self.left.len().saturating_sub(1));
        }
        self.left_list_state.select(Some(self.left_selected));
    }

    fn cycle_left_sort(&mut self) {
        self.left_sort = match self.left_sort {
            LeftSort::Default => LeftSort::ExclusivePackages,
            LeftSort::ExclusivePackages => LeftSort::ExclusiveSize,
            LeftSort::ExclusiveSize => LeftSort::Default,
        };
        let prev = self.left.get(self.left_selected).cloned();
        self.rebuild_left_preserving(prev.as_ref());
    }

    fn apply_filter_only(&mut self) {
        let prev = self.selected_package();
        self.rebuild_left_preserving(None);
        if self.selected_package() != prev {
            self.select_left_fast(self.left_selected);
            return;
        }
        self.middle_selected = self
            .middle_selected
            .min(self.middle_len().saturating_sub(1));
    }

    fn rebuild_middle(&mut self) -> Result<()> {
        let prev_pkg = self.selected_package();
        let prev_flag = self
            .flag_deltas
            .get(self.middle_selected)
            .map(|d| d.flag.clone());
        self.flag_deltas.clear();

        if let Some(LeftEntry::WorldPkg(cpn) | LeftEntry::Transitive(cpn)) =
            self.left.get(self.left_selected).cloned()
        {
            let key = (cpn.clone(), overrides_fingerprint(&self.engine));
            if let Some(cached) = self.delta_cache.get(&key) {
                self.flag_deltas = cached.clone();
            } else if let Some(base) = &self.last_outcome {
                let deltas = package_deltas_with_base(&self.engine, &self.world, &cpn, base)?;
                self.delta_cache.insert(key, deltas.clone());
                self.flag_deltas = deltas;
            }
        }
        self.deltas_loading = false;

        if self.selected_package() == prev_pkg
            && let Some(f) = prev_flag
            && let Some(idx) = self.flag_deltas.iter().position(|d| d.flag == f)
        {
            self.middle_selected = idx;
        } else {
            self.middle_selected = 0;
        }
        if self.selected_package() != prev_pkg {
            self.right_scroll = 0;
            self.middle_scroll.set(0);
        }
        self.rebuild_right_targets();
        Ok(())
    }

    fn rebuild_right_targets(&mut self) {
        self.right_targets.clear();
        self.right_target_selected = None;
        self.cut_loading = false;
        let mut seen = HashSet::new();
        for chain in self.selected_blame().iter().take(4) {
            for link in chain {
                if seen.insert(link.required_by.clone()) {
                    self.right_targets.push(link.required_by.clone());
                }
            }
        }
        if let Some(delta) = self.flag_deltas.get(self.middle_selected) {
            for p in delta.added_packages.iter().take(8) {
                if seen.insert(p.clone()) {
                    self.right_targets.push(p.clone());
                }
            }
            for p in delta.removed_packages.iter().take(8) {
                if seen.insert(p.clone()) {
                    self.right_targets.push(p.clone());
                }
            }
            if let Some(pkg) = self.selected_package() {
                for (dep_cpn, others) in
                    shared_flag_deps(&self.engine, &self.graph, &pkg, &delta.flag)
                        .into_iter()
                        .take(8)
                {
                    if seen.insert(dep_cpn.clone()) {
                        self.right_targets.push(dep_cpn);
                    }
                    for o in others.into_iter().take(4) {
                        if seen.insert(o.clone()) {
                            self.right_targets.push(o);
                        }
                    }
                }
            }
        }
    }

    fn ensure_deltas(&mut self) -> Result<()> {
        if self.deltas_loading {
            self.rebuild_middle()?;
        }
        if self.cut_loading {
            self.cut_loading = false;
            if let Some(target) = self.selected_right_target().map(str::to_string) {
                self.load_cut_for_target(&target)?;
            }
        }
        Ok(())
    }

    fn load_cut_for_target(&mut self, target: &str) -> Result<bool> {
        if let Some(cached) = self.cut_cache.get(target) {
            self.cut_plan = Some(cached.clone());
            return Ok(true);
        }
        let Some(base) = &self.last_outcome else {
            return Ok(false);
        };
        if let Some(plan) = cut_package(&self.engine, &self.world, base, target)? {
            self.cut_cache.insert(target.to_string(), plan.clone());
            self.cut_plan = Some(plan);
            return Ok(true);
        }
        Ok(false)
    }

    fn compute_cut_plan(&mut self) -> Result<bool> {
        let target = if self.pane == Pane::Right {
            self.selected_right_target()
                .map(str::to_string)
                .or_else(|| self.selected_package())
        } else {
            self.selected_package()
        };
        let Some(pkg) = target else {
            return Ok(false);
        };
        self.right_scroll = 0;
        self.load_cut_for_target(&pkg)
    }

    fn apply_cut_blockers(&mut self) -> Result<bool> {
        let Some(plan) = self.cut_plan.clone() else {
            return Ok(false);
        };
        if plan.flag_blockers.is_empty() {
            return Ok(false);
        }
        self.push_undo();
        for fb in &plan.flag_blockers {
            self.engine
                .overrides
                .entry(fb.package.clone())
                .or_default()
                .insert(fb.flag.clone(), fb.want_on);
        }
        self.dirty = true;
        self.refresh()?;
        Ok(true)
    }

    fn toggle_selected_flag(&mut self) -> Result<bool> {
        let Some(package) = self.selected_package() else {
            return Ok(false);
        };
        let Some(delta) = self.flag_deltas.get(self.middle_selected) else {
            return Ok(false);
        };
        let flag = delta.flag.clone();
        if let Some(sim_conflict) = delta.conflict.clone() {
            if let Some(idx) = self
                .conflicts
                .iter()
                .position(|c| c.package == package || c.caused_by.contains(&package))
            {
                self.modal = Some(Modal::Conflict {
                    conflict: idx,
                    selected: 0,
                });
            } else {
                self.modal = Some(Modal::SimulatedConflict {
                    conflict: sim_conflict,
                    selected: 0,
                });
            }
            return Ok(false);
        }
        let current_on = self
            .graph
            .nodes
            .get(&package)
            .map(|n| n.use_flags.contains(&flag))
            .unwrap_or_else(|| {
                self.engine
                    .index
                    .select(&package)
                    .map(|p| self.engine.effective_use(p).contains(&flag))
                    .unwrap_or(false)
            });
        self.push_undo();
        self.engine
            .overrides
            .entry(package)
            .or_default()
            .insert(flag, !current_on);
        self.dirty = true;
        self.refresh()?;
        Ok(true)
    }

    fn cycle_or_decision(&mut self) -> Result<bool> {
        let Some(group) = self.selected_or_group() else {
            return Ok(false);
        };
        let Some((candidates, selected)) = self
            .graph
            .alternatives
            .iter()
            .find(|a| a.group_id == group)
            .map(|a| (a.candidates.len(), a.selected))
        else {
            return Ok(false);
        };
        if candidates == 0 {
            return Ok(false);
        }
        self.engine.or_decisions.insert(group, (selected + 1) % candidates);
        self.dirty = true;
        self.refresh()?;
        Ok(true)
    }

    fn toggle_big_lever(&mut self) -> Result<bool> {
        let Some(lever) = self.levers.get(self.lever_selected) else {
            return Ok(false);
        };
        let flag = lever.flag.clone();
        if self.engine.profile.is_platform_flag(&flag)
            || self.engine.profile.is_masked(&flag)
            || self.engine.profile.is_forced(&flag)
        {
            self.status = format!("{flag} ist kein schaltbarer Hebel");
            return Ok(false);
        }
        let holders: Vec<String> = self
            .graph
            .nodes
            .iter()
            .filter(|(_, n)| n.use_flags.contains(&flag))
            .map(|(cpn, _)| cpn.clone())
            .collect();
        if holders.is_empty() {
            return Ok(false);
        }
        self.push_undo();
        for cpn in holders {
            self.engine
                .overrides
                .entry(cpn)
                .or_default()
                .insert(flag.clone(), false);
        }
        self.dirty = true;
        self.refresh()?;
        Ok(true)
    }

    fn open_conflict_for_package(&mut self, package: &str) {
        if let Some(idx) = self
            .conflicts
            .iter()
            .position(|c| c.package == package || c.caused_by.contains(&package.to_string()))
        {
            self.modal = Some(Modal::Conflict {
                conflict: idx,
                selected: 0,
            });
        }
    }

    fn modal_selected_option(&self) -> Option<QuickSwitch> {
        match &self.modal {
            Some(Modal::Conflict { conflict, selected }) => self
                .conflicts
                .get(*conflict)
                .and_then(|c| c.options.get(*selected))
                .cloned(),
            Some(Modal::SimulatedConflict { conflict, selected }) => {
                conflict.options.get(*selected).cloned()
            }
            Some(Modal::Help) | None => None,
        }
    }

    fn apply_quick_switch(&mut self, option: &QuickSwitch) -> Result<()> {
        if option.changes.is_empty() {
            self.status = "Keine automatische Aenderung verfuegbar".into();
            self.modal = None;
            return Ok(());
        }
        self.push_undo();
        for (package, flag, want_on) in &option.changes {
            self.engine
                .overrides
                .entry(package.clone())
                .or_default()
                .insert(flag.clone(), *want_on);
        }
        self.modal = None;
        self.dirty = true;
        self.refresh()
    }

    fn move_modal(&mut self, delta: i32) {
        let len = match &self.modal {
            Some(Modal::Conflict { conflict, .. }) => self
                .conflicts
                .get(*conflict)
                .map(|c| c.options.len().max(1))
                .unwrap_or(1),
            Some(Modal::SimulatedConflict { conflict, .. }) => conflict.options.len().max(1),
            Some(Modal::Help) | None => return,
        };
        if let Some(Modal::Conflict { selected, .. } | Modal::SimulatedConflict { selected, .. }) =
            &mut self.modal
        {
            *selected = Self::move_clamped(*selected, delta, len);
        }
    }

    fn export_package_use(&mut self, path: &Path) -> Result<()> {
        let mut cpns: Vec<&String> = self.engine.overrides.keys().collect();
        cpns.sort();
        let mut lines = Vec::new();
        for cpn in cpns {
            let pkg = self
                .graph
                .nodes
                .get(cpn)
                .and_then(|n| self.engine.index.select_cpv(cpn, &n.cpv))
                .or_else(|| self.engine.index.select(cpn));
            let Some(pkg) = pkg else { continue };

            let profile_only = self
                .engine
                .profile
                .effective_defaults_without_package_use(&pkg.iuse_defaults);
            let final_active = self.engine.effective_use(pkg);
            let mut flags: Vec<&String> = pkg.iuse_defaults.keys().collect();
            flags.sort();

            let mut tokens = Vec::new();
            for flag in flags {
                if self.engine.profile.is_masked(flag)
                    || self.engine.profile.is_forced(flag)
                    || self.engine.profile.is_platform_flag(flag)
                    || self.engine.profile.is_hidden_expand_flag(flag)
                {
                    continue;
                }
                let want = final_active.contains(flag);
                if want != profile_only.contains(flag) {
                    tokens.push(if want {
                        flag.clone()
                    } else {
                        format!("-{flag}")
                    });
                }
            }
            if !tokens.is_empty() {
                lines.push(format!("{cpn} {}", tokens.join(" ")));
            }
        }
        let body = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        std::fs::write(path, body)
            .with_context(|| format!("Schreiben fehlgeschlagen: {}", path.display()))?;
        self.dirty = false;
        self.status = format!("package.use geschrieben: {}", path.display());
        Ok(())
    }
}

fn overrides_fingerprint(engine: &Engine<'_>) -> String {
    let mut parts = Vec::new();
    let mut cpns: Vec<&String> = engine.overrides.keys().collect();
    cpns.sort();
    for cpn in cpns {
        let mut flags: Vec<String> = engine.overrides[cpn]
            .iter()
            .map(|(f, on)| format!("{f}={on}"))
            .collect();
        flags.sort();
        parts.push(format!("{cpn}:{}", flags.join(",")));
    }
    let mut ors: Vec<String> = engine
        .or_decisions
        .iter()
        .map(|(g, s)| format!("{g}={s}"))
        .collect();
    ors.sort();
    parts.push(format!("or:{}", ors.join(",")));
    parts.join("|")
}

fn left_label(entry: &LeftEntry) -> String {
    match entry {
        LeftEntry::WorldPkg(cpn) => cpn.clone(),
        LeftEntry::OrDecision(group) => format!("|| {}", group.split("||").next().unwrap_or(group)),
        LeftEntry::Transitive(cpn) => format!("  {cpn}"),
    }
}

pub fn run(state: AppState<'_>, out_use: &Path) -> Result<()> {
    ratatui::run(|terminal| event_loop(terminal, state, out_use)).context("TUI-Loop")
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    mut state: AppState<'_>,
    out_use: &Path,
) -> Result<()> {
    let timeout = Duration::from_millis(80);
    loop {
        if let Ok(size) = terminal.size() {
            state.viewport_height = size.height;
        }
        terminal.draw(|frame| render(frame, &state))?;
        if !event::poll(timeout)? {
            state.ensure_deltas()?;
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(());
        }
        if !dispatch_key(&mut state, key, out_use)? {
            return Ok(());
        }
    }
}

fn dispatch_key(state: &mut AppState<'_>, key: KeyEvent, out_use: &Path) -> Result<bool> {
    if state.filtering {
        match key.code {
            KeyCode::Esc => {
                state.filtering = false;
                state.filter.clear();
                state.apply_filter_only();
            }
            KeyCode::Enter => state.filtering = false,
            KeyCode::Tab => {
                state.filtering = false;
                state.pane = Pane::Middle;
                state.ensure_deltas()?;
            }
            KeyCode::Up => {
                state.pane = Pane::Left;
                state.move_selection(-1);
            }
            KeyCode::Down => {
                state.pane = Pane::Left;
                state.move_selection(1);
            }
            KeyCode::PageUp => {
                state.pane = Pane::Left;
                state.move_selection(-state.page_step());
            }
            KeyCode::PageDown => {
                state.pane = Pane::Left;
                state.move_selection(state.page_step());
            }
            KeyCode::Home => {
                state.pane = Pane::Left;
                state.move_to_edge(false);
            }
            KeyCode::End => {
                state.pane = Pane::Left;
                state.move_to_edge(true);
            }
            KeyCode::Backspace => {
                state.filter.pop();
                state.apply_filter_only();
            }
            KeyCode::Char(c) => {
                state.filter.push(c);
                state.apply_filter_only();
            }
            _ => {}
        }
        return Ok(true);
    }

    if state.modal.is_some() {
        if matches!(state.modal, Some(Modal::Help))
            && matches!(
                key.code,
                KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Char('Q')
            )
        {
            state.modal = None;
            return Ok(true);
        }
        match key.code {
            KeyCode::Esc => state.modal = None,
            KeyCode::Up | KeyCode::Char('k') => state.move_modal(-1),
            KeyCode::Down | KeyCode::Char('j') => state.move_modal(1),
            KeyCode::Enter => {
                if let Some(opt) = state.modal_selected_option() {
                    state.apply_quick_switch(&opt)?;
                } else {
                    state.modal = None;
                }
            }
            _ => {}
        }
        return Ok(true);
    }

    match key.code {
        KeyCode::Char('q') => return Ok(false),
        KeyCode::Char('1') => state.set_tab(Tab::Packages)?,
        KeyCode::Char('2') => state.set_tab(Tab::BigLevers)?,
        KeyCode::Tab => state.cycle_pane(),
        KeyCode::Up | KeyCode::Char('k') => state.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => state.move_selection(1),
        KeyCode::PageUp => state.move_selection(-state.page_step()),
        KeyCode::PageDown => state.move_selection(state.page_step()),
        KeyCode::Home => state.move_to_edge(false),
        KeyCode::End => state.move_to_edge(true),
        KeyCode::Char(' ') => {
            if state.tab == Tab::BigLevers {
                state.toggle_big_lever()?;
            } else if state.pane == Pane::Left {
                state.pane = Pane::Middle;
                state.ensure_deltas()?;
            } else if state.pane == Pane::Middle {
                state.toggle_selected_flag()?;
            }
        }
        KeyCode::Enter => {
            if state.selected_or_group().is_some() {
                state.cycle_or_decision()?;
            } else if let Some(pkg) = state.selected_package() {
                state.open_conflict_for_package(&pkg);
            }
        }
        KeyCode::Char('/') => {
            state.filtering = true;
            state.filter.clear();
        }
        KeyCode::Char('?') | KeyCode::F(1) => state.modal = Some(Modal::Help),
        KeyCode::Char('o' | 'O') => state.cycle_left_sort(),
        KeyCode::Char('x' | 'X') => {
            state.compute_cut_plan()?;
        }
        KeyCode::Char('a' | 'A') => {
            if state.apply_cut_blockers()? {
                state.status = "Cut-Blocker angewendet".into();
            }
        }
        KeyCode::Char('u' | 'U') => {
            state.status = if state.pop_undo()? {
                "Undo".into()
            } else {
                "Nichts zum Zurueckrollen".into()
            };
        }
        KeyCode::Char('s' | 'S') => {
            if let Err(e) = state.export_package_use(out_use) {
                state.status = format!("Export-Fehler: {e:#}");
            }
        }
        KeyCode::Esc => {
            if !state.filter.is_empty() {
                state.filter.clear();
                state.apply_filter_only();
            } else if state.pane != Pane::Left {
                state.pane = Pane::Left;
            } else {
                return Ok(false);
            }
        }
        _ => {}
    }
    Ok(true)
}

fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    if area.width < 20 || area.height < 8 {
        frame.render_widget(Paragraph::new("Terminal zu klein"), area);
        return;
    }
    let footer_len = footer_height(state, area.width);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(footer_len),
        ])
        .split(area);

    render_tabs(frame, state, rows[0]);
    match state.tab {
        Tab::Packages => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Percentage(40),
                    Constraint::Percentage(30),
                ])
                .split(rows[1]);
            render_left(frame, state, cols[0]);
            render_middle(frame, state, cols[1]);
            render_right(frame, state, cols[2]);
        }
        Tab::BigLevers => render_levers(frame, state, rows[1]),
    }
    render_footer(frame, state, rows[2]);

    match &state.modal {
        Some(Modal::Conflict { conflict, selected }) => {
            if let Some(info) = state.conflicts.get(*conflict) {
                render_conflict_modal(frame, area, info, *selected);
            }
        }
        Some(Modal::SimulatedConflict { conflict, selected }) => {
            render_conflict_modal(frame, area, conflict, *selected);
        }
        Some(Modal::Help) => render_help_modal(frame, area),
        None => {}
    }
}

fn render_tabs(frame: &mut Frame, state: &AppState, area: Rect) {
    let tabs = Tabs::new(vec!["[1] Pakete & Alternativen", "[2] Global Big Levers"])
        .select(match state.tab {
            Tab::Packages => 0,
            Tab::BigLevers => 1,
        })
        .block(Block::default().borders(Borders::BOTTOM))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn render_left(frame: &mut Frame, state: &AppState, area: Rect) {
    let has_bin = state.engine.index.has_binhost;
    let items: Vec<ListItem> = state
        .left
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let mut label = left_label(entry);
            if state.left_sort != LeftSort::Default
                && let LeftEntry::WorldPkg(cpn) | LeftEntry::Transitive(cpn) = entry
                && let Some(cost) = state.node_costs.get(cpn)
            {
                let bytes = if has_bin {
                    cost.exclusive_metrics.binpkg_bytes
                } else {
                    cost.exclusive_metrics.download_bytes
                };
                label.push_str(&format!(
                    " ({} pkg, {})",
                    cost.exclusive_packages,
                    fmt_bytes(bytes)
                ));
            }
            let mut spans = vec![Span::raw(label)];
            if let LeftEntry::WorldPkg(cpn) | LeftEntry::Transitive(cpn) = entry
                && state.conflicts.iter().any(|c| &c.package == cpn)
            {
                spans.push(Span::styled(" CONFLICT", Style::default().fg(Color::Red)));
            }
            let mut item = ListItem::new(Line::from(spans));
            if i == state.left_selected {
                let bg = if state.pane == Pane::Left {
                    Color::DarkGray
                } else {
                    Color::Blue
                };
                item = item.style(Style::default().bg(bg));
            }
            item
        })
        .collect();

    let mut title = format!("Pakete ({})", state.left.len());
    if state.filtering {
        title.push_str(&format!(" [Filter: /{}_]", state.filter));
    } else if !state.filter.is_empty() {
        title.push_str(&format!(" [Filter: {}]", state.filter));
    }
    let list = List::new(items).highlight_symbol("> ").block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style(state.pane == Pane::Left)),
    );
    let mut list_state = state.left_list_state;
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_middle(frame: &mut Frame, state: &AppState, area: Rect) {
    let mut lines = Vec::new();
    match state.left.get(state.left_selected) {
        Some(LeftEntry::WorldPkg(cpn) | LeftEntry::Transitive(cpn)) => {
            if let Some(node) = state.graph.nodes.get(cpn) {
                let bin_str = if !state.engine.index.has_binhost {
                    "n/a".to_string()
                } else if node.has_binpkg {
                    fmt_bytes(node.binpkg as i64)
                } else {
                    "kein Binpkg (Source-Build)".to_string()
                };
                lines.push(Line::from(vec![
                    Span::styled(cpn.clone(), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!(
                        "  Binpkg: {bin_str}  Pkg-Quell-DL: {}",
                        fmt_bytes(node.download as i64)
                    )),
                ]));
                lines.push(Line::from(""));
            }
            if state.deltas_loading {
                lines.push(Line::from(vec![Span::styled(
                    "(rechne Deltas...)",
                    Style::default().fg(Color::Yellow),
                )]));
            }
            for (i, delta) in state.flag_deltas.iter().enumerate() {
                let checkbox = if delta.current { "[x]" } else { "[ ]" };
                let bin = if delta.has_binhost_data {
                    fmt_delta(delta.delta.binpkg_bytes, "bin")
                } else {
                    "n/a (ohne --binhost)".to_string()
                };
                let mut spans = vec![
                    Span::raw(format!("{checkbox} {} ", delta.flag)),
                    Span::styled(bin, Style::default().fg(delta_color(delta.delta.binpkg_bytes))),
                    Span::raw(format!(" / {}", fmt_delta(delta.delta.download_bytes, "dl"))),
                ];
                if !delta.added_packages.is_empty() || !delta.removed_packages.is_empty() {
                    spans.push(Span::raw(format!(
                        " (+{}/-{} pkg)",
                        delta.added_packages.len(),
                        delta.removed_packages.len()
                    )));
                }
                if delta.conflict.is_some() {
                    spans.push(Span::styled(" CONFLICT", Style::default().fg(Color::Red)));
                }
                let line = Line::from(spans);
                if i == state.middle_selected && state.pane == Pane::Middle {
                    lines.push(line.style(Style::default().bg(Color::DarkGray)));
                } else {
                    lines.push(line);
                }
            }
            if state.flag_deltas.is_empty() && !state.deltas_loading {
                lines.push(Line::from("(keine schaltbaren Flags)"));
            }
        }
        Some(LeftEntry::OrDecision(group)) => {
            if let Some(alt) = state.graph.alternatives.iter().find(|a| &a.group_id == group) {
                lines.push(Line::from(vec![Span::styled(
                    format!("|| Entscheidung: {}", alt.owner),
                    Style::default().add_modifier(Modifier::BOLD),
                )]));
                for (i, cand) in alt.candidates.iter().enumerate() {
                    let marker = if i == alt.selected { ">" } else { " " };
                    let line = Line::from(format!("{marker} {cand}"));
                    if i == state.middle_selected && state.pane == Pane::Middle {
                        lines.push(line.style(Style::default().bg(Color::DarkGray)));
                    } else {
                        lines.push(line);
                    }
                }
            }
        }
        None => lines.push(Line::from("(leer)")),
    }

    let inner_height = area.height.saturating_sub(2) as usize;
    let selected_line = state.middle_selected + 2 + usize::from(state.deltas_loading);
    let mut offset = state.middle_scroll.get() as usize;
    if inner_height > 0 {
        if selected_line < offset {
            offset = selected_line;
        } else if selected_line >= offset + inner_height {
            offset = selected_line - inner_height + 1;
        }
    } else {
        offset = 0;
    }
    state.middle_scroll.set(offset as u16);

    let para = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Details / USE-Flags")
                .border_style(border_style(state.pane == Pane::Middle)),
        )
        .scroll((offset as u16, 0));
    frame.render_widget(para, area);
}

fn render_right(frame: &mut Frame, state: &AppState, area: Rect) {
    if state.cut_plan.is_some() {
        let cut_lines = cut_plan_lines(state);
        let mut cut_h = (cut_lines.len() as u16 + 2).clamp(5, 14);
        if area.height > 8 {
            cut_h = cut_h.min(area.height - 6).max(3);
        }
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(cut_h)])
            .split(area);
        let top = Paragraph::new(Text::from(top_lines(state)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Blame / Pros-Cons")
                    .border_style(border_style(state.pane == Pane::Right)),
            )
            .scroll((effective_right_scroll(state, chunks[0]), 0))
            .wrap(Wrap { trim: true });
        frame.render_widget(top, chunks[0]);
        let bottom = Paragraph::new(Text::from(cut_lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Cut-Analyse")
                    .border_style(border_style(state.pane == Pane::Right)),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(bottom, chunks[1]);
        return;
    }

    let para = Paragraph::new(Text::from(top_lines(state)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Blame / Pros-Cons")
                .border_style(border_style(state.pane == Pane::Right)),
        )
        .scroll((state.right_scroll, 0))
        .wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

fn top_lines(state: &AppState) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        "Warum ist das hier?",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    let selected = state.selected_right_target();

    if let Some(pkg) = state.selected_package() {
        let pkg_conflicts: Vec<&ConflictInfo> = state
            .conflicts
            .iter()
            .filter(|c| c.package == pkg || c.caused_by.contains(&pkg))
            .collect();
        if !pkg_conflicts.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::styled(
                "Konflikte:",
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Red),
            ));
            for c in pkg_conflicts.iter().take(3) {
                lines.push(Line::styled(
                    format!("! {}", c.message),
                    Style::default().fg(Color::Red),
                ));
            }
        }
    }

    let chains = state.selected_blame();
    if chains.is_empty() {
        lines.push(Line::from("(kein Pfad — Welt-Paket oder leer)"));
    }
    for (i, chain) in chains.iter().take(4).enumerate() {
        lines.push(Line::from(format!("Kette {}:", i + 1)));
        for link in chain {
            let hl = selected == Some(link.required_by.as_str());
            let text = match &link.via_flag {
                Some(f) => format!("  {} -> {} [+{}]", link.required_by, link.package, f),
                None => format!("  {} -> {}", link.required_by, link.package),
            };
            lines.push(highlight_line(text, hl));
        }
    }

    if let Some(pkg) = state.selected_package()
        && let Some(delta) = state.flag_deltas.get(state.middle_selected)
    {
        let action = if delta.current {
            "Ausschalten von"
        } else {
            "Einschalten von"
        };
        if !delta.added_packages.is_empty() || !delta.removed_packages.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::styled(
                format!("{action} {} bewirkt:", delta.flag),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for p in delta.added_packages.iter().take(8) {
                lines.push(pkg_delta_line(
                    format!("  + {p}"),
                    selected == Some(p.as_str()),
                    Color::Red,
                ));
            }
            for p in delta.removed_packages.iter().take(8) {
                lines.push(pkg_delta_line(
                    format!("  - {p}"),
                    selected == Some(p.as_str()),
                    Color::Green,
                ));
            }
            if delta.added_packages.len() + delta.removed_packages.len() > 8 {
                lines.push(Line::from("  (...)"));
            }
        }

        let shared = shared_flag_deps(&state.engine, &state.graph, &pkg, &delta.flag);
        if !shared.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::styled(
                "Geteilte Abhaengigkeiten:",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for (dep, others) in shared.iter().take(8) {
                lines.push(highlight_line(
                    format!("  {dep}"),
                    selected == Some(dep.as_str()),
                ));
                for o in others.iter().take(4) {
                    lines.push(highlight_line(
                        format!("    gehalten von {o}"),
                        selected == Some(o.as_str()),
                    ));
                }
            }
        }

        if let Some(c) = &delta.conflict {
            lines.push(Line::from(""));
            lines.push(Line::styled(
                format!("Toggle-Konflikt ({}):", delta.flag),
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Red),
            ));
            lines.push(Line::styled(
                format!("! {}", c.message),
                Style::default().fg(Color::Red),
            ));
            for opt in c.options.iter().take(3) {
                lines.push(Line::from(format!("  Loesung: {}", opt.label)));
            }
        }
        if !delta.has_binhost_data {
            lines.push(Line::from("bin: n/a (ohne --binhost)"));
        }
        if let Some(text) = state.use_descriptions.describe(Some(&pkg), &delta.flag) {
            lines.push(Line::from(""));
            lines.push(Line::from(format!("{}: {text}", delta.flag)));
        }
    }
    lines
}

fn pkg_delta_line(text: String, marked: bool, color: Color) -> Line<'static> {
    if marked {
        let body = text.strip_prefix("  ").unwrap_or(&text);
        return Line::styled(
            format!("> {body}"),
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::DarkGray),
        );
    }
    Line::styled(text, Style::default().fg(color))
}

fn highlight_line(text: String, marked: bool) -> Line<'static> {
    if marked {
        let body = text.strip_prefix("  ").unwrap_or(&text);
        Line::styled(
            format!("> {body}"),
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::DarkGray),
        )
    } else {
        Line::from(text)
    }
}

fn effective_right_scroll(state: &AppState, area: Rect) -> u16 {
    let inner = area.height.saturating_sub(2) as usize;
    if inner == 0 {
        return 0;
    }
    let Some(sel) = state.selected_right_target() else {
        return state.right_scroll;
    };
    let marked = top_lines(state).iter().position(|l| {
        let s: String = l.spans.iter().map(|sp| sp.content.as_ref()).collect();
        s.starts_with("> ") && s.contains(sel)
    });
    let Some(idx) = marked else {
        return state.right_scroll;
    };
    let scroll = state.right_scroll as usize;
    if idx < scroll || idx >= scroll + inner {
        idx as u16
    } else {
        state.right_scroll
    }
}

fn cut_plan_lines(state: &AppState) -> Vec<Line<'static>> {
    let Some(plan) = &state.cut_plan else {
        return Vec::new();
    };
    let mut lines = vec![Line::styled(
        format!("Cut-Analyse fuer {}:", plan.target),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if plan.flag_blockers.is_empty() {
        lines.push(Line::from("keine Flag-Blocker"));
    } else {
        lines.push(Line::from("Flag-Blocker (Taste [a] wendet alle an):"));
        for b in plan.flag_blockers.iter().take(10) {
            lines.push(Line::from(format!(
                "  {} [{}{}]",
                b.package,
                if b.want_on { "+" } else { "-" },
                b.flag
            )));
        }
    }
    if plan.world_blockers.is_empty() {
        lines.push(Line::from("keine harten world-Blocker"));
    } else {
        lines.push(Line::styled(
            "Harte world-Blocker:",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        for w in plan.world_blockers.iter().take(10) {
            lines.push(Line::from(format!("  {w}")));
        }
    }
    let wfb = &plan.without_flag_blockers;
    lines.push(Line::from(format!(
        "Ohne Flag-Blocker: {} / {} (-{}/+{} pkg)",
        fmt_delta(wfb.metrics.binpkg_bytes, "bin"),
        fmt_delta(wfb.metrics.download_bytes, "dl"),
        wfb.removed_packages.len(),
        wfb.added_packages.len(),
    )));
    let wab = &plan.without_all_blockers;
    lines.push(Line::from(format!(
        "Ohne alle Blocker: {} / {} (-{}/+{} pkg)",
        fmt_delta(wab.metrics.binpkg_bytes, "bin"),
        fmt_delta(wab.metrics.download_bytes, "dl"),
        wab.removed_packages.len(),
        wab.added_packages.len(),
    )));
    lines
}

fn render_levers(frame: &mut Frame, state: &AppState, area: Rect) {
    let detail = lever_detail_lines(state);
    if detail.is_empty() || area.height < 14 {
        render_lever_table(frame, state, area);
        return;
    }
    let detail_h = ((detail.len() as u16) + 2).clamp(4, 9);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(detail_h)])
        .split(area);
    render_lever_table(frame, state, rows[0]);
    let para = Paragraph::new(Text::from(detail))
        .block(Block::default().borders(Borders::ALL).title("Hebel-Details"));
    frame.render_widget(para, rows[1]);
}

fn lever_detail_lines(state: &AppState) -> Vec<Line<'static>> {
    let Some(lever) = state.levers.get(state.lever_selected) else {
        return Vec::new();
    };
    let mut lines = vec![Line::styled(
        format!("Flag {}", lever.flag),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if let Some(text) = state.use_descriptions.describe(None, &lever.flag) {
        lines.push(Line::from(text.to_string()));
    }
    let shown: Vec<&str> = lever
        .affected_packages
        .iter()
        .take(6)
        .map(String::as_str)
        .collect();
    let mut summary = format!("Betroffene Pakete ({}): {}", lever.packages, shown.join(", "));
    if lever.affected_packages.len() > 6 {
        summary.push_str(", ...");
    }
    lines.push(Line::from(summary));
    if !lever.removed_packages.is_empty() {
        let shown: Vec<&str> = lever
            .removed_packages
            .iter()
            .take(6)
            .map(String::as_str)
            .collect();
        let mut s = format!(
            "Entfernt: {} ({})",
            shown.join(", "),
            fmt_bytes(lever.total.binpkg_bytes)
        );
        if lever.removed_packages.len() > 6 {
            s.push_str(", ...");
        }
        lines.push(Line::styled(s, Style::default().fg(Color::Green)));
    }
    if !lever.added_packages.is_empty() {
        let shown: Vec<&str> = lever
            .added_packages
            .iter()
            .take(6)
            .map(String::as_str)
            .collect();
        let mut s = format!("Hinzu: {}", shown.join(", "));
        if lever.added_packages.len() > 6 {
            s.push_str(", ...");
        }
        lines.push(Line::styled(s, Style::default().fg(Color::Red)));
    }
    lines
}

fn render_lever_table(frame: &mut Frame, state: &AppState, area: Rect) {
    let header = Row::new(vec![
        "Flag", "An?", "Pakete", "Pkg-Delta", "Bin-Delta", "DL-Delta", "Konflikte",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = state
        .levers
        .iter()
        .map(|lever| {
            Row::new(vec![
                TableCell::from(lever.flag.clone()),
                TableCell::from("[AUS]"),
                TableCell::from(lever.packages.to_string()),
                TableCell::from(format!("{:+}", lever.net_pkg_delta)),
                TableCell::from(fmt_bytes(lever.total.binpkg_bytes)),
                TableCell::from(fmt_bytes(lever.total.download_bytes)),
                TableCell::from(lever.conflicts.to_string()),
            ])
        })
        .collect();
    let widths = [
        Constraint::Min(10),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(10),
    ];
    let mut title = "Global Big Levers [Space toggelt]".to_string();
    if state.levers_dirty {
        title.push_str(" (veraltet — wird neu gerechnet)");
    }
    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ")
        .block(Block::default().borders(Borders::ALL).title(title));
    let mut table_state = state.lever_table_state;
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn footer_height(state: &AppState, width: u16) -> u16 {
    let w = (width as usize).max(20);
    let keys_len = KEYS_HELP.len()
        + if state.dirty { 16 } else { 0 }
        + if state.status.is_empty() { 0 } else { state.status.len() + 2 };
    let rows = totals_text(state).len().div_ceil(w) + keys_len.div_ceil(w);
    (rows.max(2) as u16 + 1).clamp(3, 7)
}

const KEYS_HELP: &str = "  [Space] Toggle  [Tab] Pane  [1/2] View  [/] Suche  [o] Sort  [x] Cut  [a] Apply  [u] Undo  [S] package.use  [?] Hilfe  [q] Quit";

fn totals_text(state: &AppState) -> String {
    let totals = state.graph.totals();
    let bin = if state.engine.index.has_binhost {
        let hits = state.graph.nodes.values().filter(|n| n.binpkg > 0).count();
        format!(
            "Binpkg {} ({}/{} Hits)",
            fmt_bytes(totals.binpkg_bytes),
            hits,
            state.graph.nodes.len()
        )
    } else {
        "Binpkg n/a (ohne --binhost)".to_string()
    };
    format!(
        "{bin}  Quell-DL {}  Build-Only {} bin / {} dl  Pakete {}  Konflikte {}",
        fmt_bytes(totals.download_bytes),
        fmt_bytes(state.graph.build_extra.binpkg_bytes),
        fmt_bytes(state.graph.build_extra.download_bytes),
        state.graph.nodes.len(),
        state.conflicts.len()
    )
}

fn render_footer(frame: &mut Frame, state: &AppState, area: Rect) {
    let top = Line::styled(
        totals_text(state),
        Style::default().add_modifier(Modifier::BOLD),
    );
    let mut key_spans = vec![Span::raw(KEYS_HELP)];
    if state.dirty {
        key_spans.push(Span::styled(
            "  *ungespeichert*",
            Style::default().fg(Color::Yellow),
        ));
    }
    if !state.status.is_empty() {
        key_spans.push(Span::raw(format!("  {}", state.status)));
    }
    let para = Paragraph::new(Text::from(vec![top, Line::from(key_spans)]))
        .block(Block::default().borders(Borders::TOP))
        .wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

fn render_help_modal(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::styled("Metriken lesen:", Style::default().add_modifier(Modifier::BOLD)),
        Line::from("Binpkg (Binhost, Trefferquote) / Quell-DL (Tarballs)"),
        Line::from("Build-Only (BDEPEND, bin+src) / Pkg-Quell-DL (nur Paket)"),
        Line::from("0 B-Delta: geteilte Abh. oder Compile-only-Flag"),
        Line::from("Rechte Pane: j/k waehlt Pakete + Auto-Cut [x]; PgUp/PgDn blaettert"),
        Line::from(""),
        Line::styled("Navigation:", Style::default().add_modifier(Modifier::BOLD)),
        Line::from("/ Suche (Esc loescht Filter), Tab Pane, Space Toggle"),
        Line::from("Enter Auswahl, 1/2 Tabs, j/k Scrollen, o Sort, x Cut, a Apply, u Undo"),
        Line::from(""),
        Line::styled("Ablauf:", Style::default().add_modifier(Modifier::BOLD)),
        Line::from("[S] package.use schreiben"),
        Line::from(""),
        Line::from("[Esc]/[?]/[q] schliesst Hilfe"),
    ];
    let width = (lines.iter().map(|l| l.width()).max().unwrap_or(40) as u16 + 4)
        .clamp(40, area.width.saturating_sub(4).max(40));
    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2).max(4));
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    let para = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Hilfe"))
        .wrap(Wrap { trim: true });
    frame.render_widget(para, popup);
}

fn render_conflict_modal(frame: &mut Frame, area: Rect, info: &ConflictInfo, selected: usize) {
    let width = area.width.saturating_sub(10).clamp(40, 100);
    let height = (info.options.len() as u16 + 6)
        .max(8)
        .min(area.height.saturating_sub(4));
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    let mut lines = vec![
        Line::styled(format!("Konflikt: {}", info.message), Style::default().fg(Color::Red)),
        Line::from(format!("Verursacher: {}", info.caused_by.join(", "))),
        Line::from(""),
    ];
    for (i, opt) in info.options.iter().enumerate() {
        let marker = if i == selected { ">" } else { " " };
        lines.push(Line::from(format!("{marker} {}", opt.label)));
    }
    if info.options.is_empty() {
        lines.push(Line::from("(keine automatische Aufloesung — manuell toggeln)"));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("[Enter] uebernehmen  [Esc] abbrechen"));
    let para = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Quick-Switch"));
    frame.render_widget(para, popup);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn border_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn fmt_bytes(bytes: i64) -> String {
    let abs = bytes.unsigned_abs();
    if abs >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if abs >= 1024 {
        format!("{:.1} kB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn fmt_delta(bytes: i64, unit: &str) -> String {
    if bytes > 0 {
        format!("+{} {unit}", fmt_bytes(bytes))
    } else {
        format!("{} {unit}", fmt_bytes(bytes))
    }
}

fn delta_color(bytes: i64) -> Color {
    if bytes < 0 {
        Color::Green
    } else if bytes > 0 {
        Color::Red
    } else {
        Color::DarkGray
    }
}
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
