# fnox-add

![CI](https://github.com/joakimen/fnox-add/actions/workflows/ci.yml/badge.svg)

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

Select entries in the picker and confirm; each one runs `fnox set` to write the
reference into the project's `fnox.toml`.

## Configuration

The catalog lives at `~/.config/fnox-add/config.toml` (override with `--config` or
`FNOX_ADD_CONFIG`). Run `fnox-add --init` to scaffold it.

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
- A bare `vault/item/field` reference is stored as `op://vault/item/field`; a
  reference with a scheme (`op://…`, `aws://…`) is stored verbatim.
- `provider` names an existing fnox provider; fnox-add never creates providers.
