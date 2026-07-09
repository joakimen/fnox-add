//! Pure catalog logic: parsing the config, flattening it into selectable items,
//! and building the `fnox set` argument list. Kept free of I/O so it is easy to
//! unit-test.

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

/// The personal catalog: named groups of known secrets.
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub groups: BTreeMap<String, Group>,
}

/// A group ties a set of `ENV:reference` entries to one existing fnox provider.
#[derive(Debug, Deserialize)]
pub struct Group {
    pub provider: String,
    #[serde(default)]
    pub secrets: Vec<String>,
}

/// One selectable secret, resolved from a catalog entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub env: String,
    pub reference: String,
    pub group: String,
    pub provider: String,
}

impl fmt::Display for Item {
    /// The line shown in the fuzzy finder.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}  [{}]  {}", self.env, self.group, self.reference)
    }
}

/// Parse the catalog TOML.
pub fn parse_config(data: &str) -> Result<Config> {
    toml::from_str(data).context("parsing catalog TOML")
}

/// Split an `ENV_NAME:reference` entry on the first colon, trimming whitespace.
pub fn parse_entry(entry: &str) -> Result<(String, String)> {
    let (env, reference) = entry
        .split_once(':')
        .ok_or_else(|| anyhow!("malformed entry {entry:?}: expected ENV_NAME:reference"))?;
    let env = env.trim().to_string();
    let reference = reference.trim().to_string();
    if env.is_empty() || reference.is_empty() {
        bail!("malformed entry {entry:?}: empty env name or reference");
    }
    Ok((env, reference))
}

/// Turn a catalog reference into the value fnox stores. A bare `vault/item/field`
/// is treated as 1Password (`op://…`); a reference with a scheme is kept verbatim.
pub fn value_for_ref(reference: &str) -> String {
    if reference.contains("://") {
        reference.to_string()
    } else {
        format!("op://{reference}")
    }
}

/// Flatten the catalog into a deterministic item list, optionally filtered to one
/// group. `BTreeMap` iteration yields groups in sorted order.
pub fn build_items(cfg: &Config, group_filter: Option<&str>) -> Result<Vec<Item>> {
    if let Some(g) = group_filter
        && !cfg.groups.contains_key(g)
    {
        bail!("unknown group {g:?}");
    }

    let mut items = Vec::new();
    for (name, group) in &cfg.groups {
        if group_filter.is_some_and(|g| g != name) {
            continue;
        }
        if group.provider.is_empty() {
            bail!("group {name:?} has no provider");
        }
        for entry in &group.secrets {
            let (env, reference) = parse_entry(entry).with_context(|| format!("group {name:?}"))?;
            items.push(Item {
                env,
                reference,
                group: name.clone(),
                provider: group.provider.clone(),
            });
        }
    }
    Ok(items)
}

/// Build the exact `fnox set` argument list for an item. When `target` is set it
/// pins the destination file with `-c`; otherwise fnox writes `./fnox.toml`.
pub fn build_set_args(item: &Item, target: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "set".to_string(),
        item.env.clone(),
        value_for_ref(&item.reference),
        "--provider".to_string(),
        item.provider.clone(),
    ];
    if let Some(t) = target {
        args.push("-c".to_string());
        args.push(t.to_string());
    }
    args
}

/// Resolve the catalog config path from the given inputs (pure, so it is testable
/// without mutating process environment). Precedence: explicit flag, then
/// `FNOX_ADD_CONFIG`, then `$XDG_CONFIG_HOME/fnox-add/config.toml`, then
/// `~/.config/fnox-add/config.toml`.
pub fn resolve_config_path(
    flag: Option<&str>,
    env_config: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf> {
    if let Some(f) = flag.filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(f));
    }
    if let Some(e) = env_config.filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(e));
    }
    let base = match xdg_config_home.filter(|s| !s.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => {
            let home = home
                .filter(|s| !s.is_empty())
                .context("locating home dir")?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(base.join("fnox-add").join("config.toml"))
}

/// Starter catalog written by `--init`. Every example line is commented out, so a
/// freshly created file parses as an empty catalog until the user fills it in.
pub const CONFIG_TEMPLATE: &str = r#"# fnox-add catalog: named groups of ENV_NAME:reference entries, each bound to an
# existing fnox provider. Uncomment and edit to define your own.
#
# [groups.personal]
# provider = "onepass"
# secrets = [
#   "GITHUB_TOKEN:api-keys/github/credential",
#   "SONAR_TOKEN:api-keys/sonarcloud/credential",
# ]
#
# [groups.work]
# provider = "onepass-work"
# secrets = [
#   "ARTIFACTORY_TOKEN:work-vault/artifactory/token",
# ]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entry_variants() {
        assert_eq!(
            parse_entry("SONAR_TOKEN:api-keys/sonarcloud/credential").unwrap(),
            (
                "SONAR_TOKEN".to_string(),
                "api-keys/sonarcloud/credential".to_string()
            )
        );
        // trims whitespace
        assert_eq!(
            parse_entry("  GH : api-keys/github/credential ").unwrap(),
            ("GH".to_string(), "api-keys/github/credential".to_string())
        );
        // only splits on the first colon
        assert_eq!(
            parse_entry("X:op://vault/item/field").unwrap(),
            ("X".to_string(), "op://vault/item/field".to_string())
        );
        assert!(parse_entry("no-colon").is_err());
        assert!(parse_entry(":ref").is_err());
        assert!(parse_entry("ENV:").is_err());
    }

    #[test]
    fn value_for_ref_prefixes_only_bare_refs() {
        assert_eq!(
            value_for_ref("api-keys/sonarcloud/credential"),
            "op://api-keys/sonarcloud/credential"
        );
        assert_eq!(
            value_for_ref("op://api-keys/github/credential"),
            "op://api-keys/github/credential"
        );
        assert_eq!(value_for_ref("aws://secret/name"), "aws://secret/name");
    }

    #[test]
    fn build_set_args_with_and_without_target() {
        let item = Item {
            env: "SONAR_TOKEN".to_string(),
            reference: "api-keys/sonarcloud/credential".to_string(),
            group: "personal".to_string(),
            provider: "onepass".to_string(),
        };
        assert_eq!(
            build_set_args(&item, None),
            vec![
                "set",
                "SONAR_TOKEN",
                "op://api-keys/sonarcloud/credential",
                "--provider",
                "onepass"
            ]
        );
        assert_eq!(
            build_set_args(&item, Some("./fnox.toml")),
            vec![
                "set",
                "SONAR_TOKEN",
                "op://api-keys/sonarcloud/credential",
                "--provider",
                "onepass",
                "-c",
                "./fnox.toml"
            ]
        );
    }

    fn sample() -> Config {
        parse_config(
            r#"
[groups.personal]
provider = "onepass"
secrets = [
  "SONAR_TOKEN:api-keys/sonarcloud/credential",
  "GH_TOKEN:api-keys/github/credential",
]
[groups.work]
provider = "onepass-work"
secrets = ["ARTIFACTORY_TOKEN:work/artifactory/token"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn build_items_all_groups_sorted() {
        let items = build_items(&sample(), None).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].group, "personal");
        assert_eq!(items[2].group, "work");
        assert_eq!(items[2].provider, "onepass-work");
    }

    #[test]
    fn build_items_group_filter() {
        let items = build_items(&sample(), Some("work")).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].env, "ARTIFACTORY_TOKEN");
    }

    #[test]
    fn build_items_errors() {
        assert!(build_items(&sample(), Some("nope")).is_err());

        let no_provider =
            parse_config("[groups.x]\nprovider = \"\"\nsecrets = [\"A:v/i/f\"]").unwrap();
        assert!(build_items(&no_provider, None).is_err());

        let bad_entry =
            parse_config("[groups.x]\nprovider = \"p\"\nsecrets = [\"no-colon\"]").unwrap();
        assert!(build_items(&bad_entry, None).is_err());
    }

    #[test]
    fn resolve_config_path_precedence() {
        // explicit flag wins
        assert_eq!(
            resolve_config_path(
                Some("/flag.toml"),
                Some("/env.toml"),
                Some("/xdg"),
                Some("/home")
            )
            .unwrap(),
            PathBuf::from("/flag.toml")
        );
        // env over xdg
        assert_eq!(
            resolve_config_path(None, Some("/env.toml"), Some("/xdg"), Some("/home")).unwrap(),
            PathBuf::from("/env.toml")
        );
        // xdg over home
        assert_eq!(
            resolve_config_path(None, None, Some("/xdg"), Some("/home")).unwrap(),
            PathBuf::from("/xdg/fnox-add/config.toml")
        );
        // falls back to ~/.config
        assert_eq!(
            resolve_config_path(None, None, None, Some("/home/tester")).unwrap(),
            PathBuf::from("/home/tester/.config/fnox-add/config.toml")
        );
        // empty strings are ignored
        assert_eq!(
            resolve_config_path(Some(""), Some(""), Some(""), Some("/home/tester")).unwrap(),
            PathBuf::from("/home/tester/.config/fnox-add/config.toml")
        );
    }

    #[test]
    fn config_template_parses_as_empty_catalog() {
        let cfg = parse_config(CONFIG_TEMPLATE).unwrap();
        assert!(cfg.groups.is_empty());
    }
}
