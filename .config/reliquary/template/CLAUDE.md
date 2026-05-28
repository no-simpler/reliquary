# Relic template

This directory is the skeleton a new in-house relic is scaffolded from.
Copy it (`cp -r ~/.config/reliquary/template ~/.config/relics/<name>`) and
fill in the blanks.

For the full graduation reference, see `~/.config/reliquary/GRADUATION.md`.

## What to fill in

1. **`relic.sh`** — set `NAME`, `RUNTIME`, etc.
2. **`src/`** — put your source here.
3. **`entrypoints/<name>`** — symlink to the executable in `src/` you want
   published onto `$PATH`. Filename = published name.
4. **`tests/`** — optional; add tests in your runtime's idiom.
5. **`scripts/`** — optional; only add `publish.sh` / `test.sh` /
   `update.sh` if you need to override the defaults from
   `~/.config/reliquary/lib/relic.sh`.
6. **`CLAUDE.md`** — replace this file with project-specific agent context.

## Publish

```bash
source ~/.config/reliquary/lib/relic.sh
relic::publish ~/.config/relics/<name>
```
