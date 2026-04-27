##
## Fish shell configuration
## Per-feature files live in ~/.config/fish/conf.d/ (auto-sourced by fish in
## alphanumeric order). This file only handles entry-shell concerns: D__SHELL,
## interactive guard, and machine-local pre/post hooks.
##

## Set and export the name of the current shell (always — also visible non-interactively)
set -gx D__SHELL fish

## Fail-safe against non-interactive shells
status is-interactive; or return

## Source the box-specific '.pre.fish' file (machine-local, not tracked)
test -r ~/.pre.fish; and source ~/.pre.fish

## conf.d/*.fish are auto-sourced by fish — nothing to do here.

## Source the box-specific '.post.fish' file (machine-local, not tracked)
test -r ~/.post.fish; and source ~/.post.fish

# OrbStack init is sourced from ~/.config/fish/conf.d/040-env.fish — do not re-inject.
