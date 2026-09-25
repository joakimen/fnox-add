# fnox-add

[![CI](https://github.com/joakimen/fnox-add/actions/workflows/ci.yml/badge.svg)](https://github.com/joakimen/fnox-add/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fjoakimen%2Ffnox-add%2Fmain%2Frust-toolchain.toml&query=%24.toolchain.channel&label=rust)](rust-toolchain.toml)
[![License](https://img.shields.io/github/license/joakimen/fnox-add)](LICENSE)

Fuzzy-select secret references from a personal catalog and write them into a
project's `fnox.toml`.

Requires the [`fnox`](https://github.com/jdx/fnox) CLI on your PATH.

## Install

With [mise](https://mise.jdx.dev):

```sh
mise use -g github:joakimen/fnox-add
```

Or download `fnox-add-aarch64-apple-darwin.tar.gz` from the
[releases page](https://github.com/joakimen/fnox-add/releases) and put the binary
on your PATH. Each release is signed with build provenance:

```sh
gh attestation verify fnox-add-aarch64-apple-darwin.tar.gz --repo joakimen/fnox-add
```

Or build from source:

```sh
cargo install --path .
```

## Usage

```sh
fnox-add --init          # scaffold a starter config file
fnox-add                 # select secrets across all groups
fnox-add --group work    # restrict to one group
fnox-add --dry-run       # print the commands without running them
fnox-add --target path/to/fnox.toml
```

Select entries in the picker and confirm. Each one runs `fnox set` to write the
reference into the project's `fnox.toml`.

## Keys

Type to fuzzy-filter across env names, groups and references; matched
characters are highlighted. The filter line takes the usual readline keys.
Colors follow the terminal palette and are disabled when `NO_COLOR` is set.

| Key | Action |
| --- | --- |
| `tab` | Toggle selection of the highlighted entry and move down |
| `↑` / `↓`, `ctrl-p` / `ctrl-n` | Move the highlight |
| `enter` | Confirm the selection |
| `esc`, `ctrl-c` | Cancel |
| `ctrl-a` / `ctrl-e` | Start / end of line |
| `ctrl-b` / `ctrl-f` | Back / forward one character |
| `ctrl-w` | Delete the previous word |
| `ctrl-u` / `ctrl-k` | Delete to start / end of line |

## Configuration

The catalog lives at `$XDG_CONFIG_HOME/fnox-add/config.toml`, or
`~/.config/fnox-add/config.toml` when `XDG_CONFIG_HOME` is unset. Override the
path with `--config` or `FNOX_ADD_CONFIG`. Run `fnox-add --init` to scaffold it.

```toml
[groups.personal]
provider = "onepass"
secrets = [
  "GITHUB_TOKEN:api-keys/github/credential",
  "SONAR_TOKEN:api-keys/sonarcloud/credential",
]

[groups.work]
provider = "onepass-work"
secrets = [
  "ARTIFACTORY_TOKEN:work-vault/artifactory/token",
]
```

- Each entry is `ENV_NAME:reference`, split on the first `:`.
- A bare `vault/item/field` reference is stored as `op://vault/item/field`. A
  reference with a scheme (`op://…`, `aws://…`) is stored verbatim.
- `provider` names an existing fnox provider. fnox-add never creates providers.
