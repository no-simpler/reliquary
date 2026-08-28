#!/usr/bin/env python3
"""The TOML half of the relic manifest reader.

Invoked by ``reliquary/lib/relic.sh`` with one or more relic directories, it
parses each ``relic.toml`` and emits shell assignments the caller ``eval``s. The
bash half emits the identical record for a legacy ``relic.sh``, so everything
upstream sees one format and one schema regardless of which parser ran.

Batched on purpose. An interpreter start is ~22ms and `relic list` reads every
manifest two or three times; per-read startup would put a second on it.

Record, terminated by ``__RELIC_END``, every field always present so no value
leaks from the record before it:

    __RELIC_DIR='…'         the directory this record answers for
    __RELIC_ERROR='…'       empty when the manifest parsed
    NAME='…' DESCRIPTION='…' RUNTIME='…' RUNTIME_EXEMPTION='…'
    MIN_RUNTIME_VERSION='…' DOCKER='0'
    ENTRYPOINTS=( … ) BREW_DEPS=( … ) EXTERNAL_DEPS=( … )
    __RELIC_END=1

Required fields are not checked here. Both parsers feed one caller, and a rule
enforced in the shared caller cannot come to mean two things.

bash-3.2 safe by construction: ``shlex.quote`` output, plain scalars and plain
arrays. `12-publish-relics.sh` is sourced into stock macOS bash.
"""

import os
import shlex
import sys

# `tomllib` is 3.11+. macOS ships 3.9.6 at /usr/bin/python3, so any bootstrap
# that has not put Homebrew's python ahead of it arrives here — and a bare
# import raises ModuleNotFoundError, whose traceback reads as a broken relic
# rather than as the wrong interpreter. Name the interpreter and the remedy,
# above the import, so the message survives the version that cannot run it.
if sys.version_info < (3, 11):
    _found = ".".join(str(part) for part in sys.version_info[:3])
    sys.exit(
        f"relic: the manifest reader needs python >= 3.11 for tomllib; this is "
        f"{_found} at {sys.executable}. Put Homebrew's python3 ahead of "
        f"/usr/bin on PATH."
    )

import tomllib

STRINGS = {
    "name": "NAME",
    "description": "DESCRIPTION",
    "runtime": "RUNTIME",
    "runtime-exemption": "RUNTIME_EXEMPTION",
    "min-runtime-version": "MIN_RUNTIME_VERSION",
}
ARRAYS = {
    "entrypoints": "ENTRYPOINTS",
    "brew-deps": "BREW_DEPS",
    "external-deps": "EXTERNAL_DEPS",
}
FLAGS = {"docker": "DOCKER"}

KNOWN = set(STRINGS) | set(ARRAYS) | set(FLAGS)


def parse(path):
    """The `[relic]` table as bash field names, or `ValueError` naming the key.

    Unknown keys are refused. A sourced manifest never could, so a typo'd
    `BREW_DEP=` was silently ignored; here it is a failure at the first read.
    Unknown *tables* are left alone — `[relic]` exists to leave room beside it.
    """
    with open(path, "rb") as handle:
        document = tomllib.load(handle)

    table = document.get("relic")
    if table is None:
        raise ValueError("no [relic] table")
    if not isinstance(table, dict):
        raise ValueError("[relic] is not a table")

    unknown = sorted(set(table) - KNOWN)
    if unknown:
        raise ValueError("unknown key(s) in [relic]: " + ", ".join(unknown))

    fields = {}
    for key, name in STRINGS.items():
        value = table.get(key, "")
        if not isinstance(value, str):
            raise ValueError(f"{key} must be a string")
        fields[name] = value
    for key, name in ARRAYS.items():
        value = table.get(key, [])
        if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
            raise ValueError(f"{key} must be an array of strings")
        fields[name] = value
    for key, name in FLAGS.items():
        value = table.get(key, False)
        if not isinstance(value, bool):
            raise ValueError(f"{key} must be a boolean")
        fields[name] = "1" if value else "0"
    return fields


def record(directory, fields=None, error=""):
    """One record, as the lines a caller evaluates."""
    fields = fields or {}
    lines = [
        f"__RELIC_DIR={shlex.quote(directory)}",
        f"__RELIC_ERROR={shlex.quote(error)}",
    ]
    for name in list(STRINGS.values()) + list(FLAGS.values()):
        lines.append(f"{name}={shlex.quote(fields.get(name, '0' if name == 'DOCKER' else ''))}")
    for name in ARRAYS.values():
        items = " ".join(shlex.quote(v) for v in fields.get(name, []))
        lines.append(f"{name}=( {items} )")
    lines.append("__RELIC_END=1")
    return "\n".join(lines)


def main(directories):
    for directory in directories:
        path = os.path.join(directory, "relic.toml")
        try:
            print(record(directory, fields=parse(path)))
        except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
            print(record(directory, error=f"{path}: {error}"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
