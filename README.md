# fnox-add

![CI](https://github.com/joakimen/fnox-add/actions/workflows/ci.yml/badge.svg)

Fuzzy-select secret references from a personal catalog and write them into the
current project's `fnox.toml` via `fnox set`.

## Prerequisites

- The [`fnox`](https://github.com/jdx/fnox) CLI on your PATH.

## Install

With [mise](https://mise.jdx.dev) (recommended):

```sh
mise use -g ubi:joakimen/fnox-add
```

Or download the latest `fnox-add-aarch64-apple-darwin.tar.gz` from the
[releases page](https://github.com/joakimen/fnox-add/releases) and put the
extracted `fnox-add` binary on your PATH.

Or build from source with a Rust toolchain via [rustup](https://rustup.rs):

```sh
cargo install --path .
```

## Build, test, run

Everything goes through Cargo. A `Makefile` wraps the common commands, but the
raw `cargo` commands work too.

| Task     | Make            | Cargo                                   |
| -------- | --------------- | --------------------------------------- |
| build    | `make build`    | `cargo build --release`                 |
| test     | `make test`     | `cargo test`                            |
| lint     | `make lint`     | `cargo clippy --all-targets -- -D warnings` |
| format   | `make fmt`      | `cargo fmt`                             |
| install  | `make install`  | `cargo install --path .`                |
| run      | `make run ARGS="--dry-run"` | `cargo run -- --dry-run`     |

`make build` runs format-check + clippy + tests before building. The release
binary lands in `target/release/fnox-add`; `make install` copies it to
`~/.cargo/bin/fnox-add`.

## Catalog config

`~/.config/fnox-add/config.toml` (override with `--config` or `FNOX_ADD_CONFIG`):

```toml
[groups.personal]
provider = "onepass"                 # an existing fnox provider (account + vault live there)
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

- **Entry format**: `ENV_NAME:reference`, split on the first `:`.
- **Reference**: a bare `vault/item/field` is stored as `op://vault/item/field`;
  a reference that already has a scheme (`op://…`, `aws://…`) is stored verbatim.
- **Provider**: each group names an already-configured fnox provider; this tool
  never creates providers.

## Usage

```sh
fnox-add                 # fuzzy multi-select across all groups
fnox-add --group work    # restrict to one group
fnox-add --dry-run       # print the fnox commands without running them
fnox-add --target path/to/fnox.toml
```

In the picker: type to filter, `Space` to toggle a selection, `↑/↓` to move,
`Enter` to confirm, `Esc` to abort. Each selection runs:

```sh
fnox set <ENV> op://<reference> --provider <group-provider>
```

`fnox set` creates/updates `./fnox.toml` in the current directory.

## Project layout

- `src/config.rs` — pure logic (parsing, item building, arg construction) with unit
  tests; no I/O, so it is easy to test.
- `src/main.rs` — the shell: CLI parsing (`clap`), the picker (`inquire`), and
  running `fnox` (`std::process::Command`).
