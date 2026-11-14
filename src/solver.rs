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
