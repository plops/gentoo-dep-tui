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