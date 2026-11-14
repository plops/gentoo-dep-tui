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
