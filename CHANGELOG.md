# Changelog

All notable changes to whetstone will be documented in this file.

## [Unreleased]

### Added

- **Model upgrade prompt**: When launching Claude Code, whetstone now checks
  the effective model (project settings > global settings > default). If the
  model is not the latest (`claude-opus-4-7`), an interactive prompt offers
  four options:
  1. Keep current model and continue
  2. Use the latest model for this session only
  3. Set the latest model project-wide (`.claude/settings.local.json`)
  4. Set the latest model globally (`~/.claude/settings.json`)

  The prompt is skipped in non-interactive mode, when the user explicitly
  passes `--model claude-opus-4-7`, or when settings already specify the
  latest model. Selecting "Keep current" dismisses the prompt until the
  next whetstone update.

## [2.3.2] - 2025-05-26

### Fixed

- Release workflow: give verify-release job explicit repo context so `gh release` commands work without a local git checkout.

## [2.3.1] - 2025-05-26

### Fixed

- Justfile release recipe corrections.

## [2.3.0] - 2025-05-25

### Added

- `whetstone version` command showing component versions with outdated indicators.
- TUI setup wizard.
- Suppressed noisy installer output during `whetstone update`.
