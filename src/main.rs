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