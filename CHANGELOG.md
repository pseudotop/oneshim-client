# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Note
This is the initial codebase after history consolidation.
Previous development history is preserved in the `legacy/pre-release` branch.

## Version Management Rules

### Release Workflow
1. Update `version` in `Cargo.toml` workspace section
2. Add changelog entry under the new version heading
3. Commit: `release: v{version}`
4. Tag: `git tag v{version}` — triggers CI/CD release pipeline
5. Push: `git push origin main --tags`

### Versioning Policy
- **Patch** (0.0.x): Bug fixes, CI/CD fixes, documentation
- **Minor** (0.x.0): New features, new crates, API changes
- **Major** (x.0.0): Breaking changes, architecture redesign

### Changelog Entry Format
Each version entry must include:
- **Added**: New features or capabilities
- **Changed**: Changes to existing functionality
- **Fixed**: Bug fixes
- **Removed**: Removed features or capabilities

---

[Unreleased]: https://github.com/pseudotop/oneshim-client/compare/main...HEAD
