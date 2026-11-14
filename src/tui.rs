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
