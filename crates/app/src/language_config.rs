use std::collections::BTreeMap;

use serde::Deserialize;

use crate::language::{LanguageError, LanguageServerSpec, LanguageServers};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanguageFile {
    #[serde(default, rename = "language-server")]
    servers: BTreeMap<String, ServerDefinition>,
    #[serde(default, rename = "language")]
    languages: Vec<LanguageDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerDefinition {
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LanguageDefinition {
    name: String,
    #[serde(default, rename = "file-types")]
    file_types: Vec<String>,
    #[serde(default, rename = "language-servers")]
    servers: Vec<String>,
}

/// Parse the supported Helix-style configuration subset into owned registry data.
pub fn parse_language_servers(input: &str) -> Result<LanguageServers, LanguageError> {
    let file: LanguageFile = toml::from_str(input)
        .map_err(|error| LanguageError(format!("invalid language configuration: {error}")))?;
    let mut registry = LanguageServers::default();
    for language in file.languages {
        if language.name.trim().is_empty() {
            return Err(LanguageError("language name must not be empty".to_string()));
        }
        if language.file_types.is_empty()
            || language
                .file_types
                .iter()
                .any(|extension| extension.trim().is_empty())
        {
            return Err(LanguageError(format!(
                "language `{}` requires non-empty file-types",
                language.name
            )));
        }
        if language.servers.len() != 1 {
            return Err(LanguageError(format!(
                "language `{}` requires exactly one language server; multiple providers are not supported yet",
                language.name
            )));
        }
        let server_name = &language.servers[0];
        let server = file.servers.get(server_name).ok_or_else(|| {
            LanguageError(format!(
                "language `{}` references unknown language server `{server_name}`",
                language.name
            ))
        })?;
        if server.command.trim().is_empty() {
            return Err(LanguageError(format!(
                "language server `{server_name}` requires a non-empty command"
            )));
        }
        registry.register(
            LanguageServerSpec::new(&language.name, &server.command, server.args.clone())
                .with_extensions(language.file_types),
        )?;
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::language::LanguageServerRegistry;

    use super::*;

    #[test]
    fn parses_external_typescript_and_python_servers() {
        let registry = parse_language_servers(
            r#"
[language-server.typescript-language-server]
command = "typescript-language-server"
args = ["--stdio"]

[language-server.ruff]
command = "ruff"
args = ["server"]

[[language]]
name = "typescript"
file-types = ["ts", "tsx"]
language-servers = ["typescript-language-server"]

[[language]]
name = "python"
file-types = ["py", "pyi"]
language-servers = ["ruff"]
"#,
        )
        .unwrap();

        let typescript = registry.find_for_path(Path::new("component.tsx")).unwrap();
        assert_eq!(typescript.command(), "typescript-language-server");
        assert_eq!(typescript.args(), &[String::from("--stdio")]);
        let python =
            LanguageServerRegistry::find_for_path(&registry, Path::new("main.py")).unwrap();
        assert_eq!(python.command(), "ruff");
        assert_eq!(python.args(), &[String::from("server")]);
    }

    #[test]
    fn rejects_unknown_empty_duplicate_and_multi_server_configuration() {
        for (input, expected) in [
            (
                r#"
[language-server.ts]
command = "typescript-language-server"
[[language]]
name = "typescript"
file-types = ["ts"]
language-servers = ["missing"]
"#,
                "unknown language server",
            ),
            (
                r#"
[language-server.ts]
command = " "
[[language]]
name = "typescript"
file-types = ["ts"]
language-servers = ["ts"]
"#,
                "non-empty command",
            ),
            (
                r#"
[language-server.ts]
command = "typescript-language-server"
[[language]]
name = "typescript"
file-types = []
language-servers = ["ts"]
"#,
                "non-empty file-types",
            ),
            (
                r#"
[language-server.ts]
command = "typescript-language-server"
[[language]]
name = "typescript"
file-types = ["ts"]
language-servers = ["ts", "ts"]
"#,
                "multiple providers are not supported yet",
            ),
            (
                r#"
[language-server.ts]
command = "typescript-language-server"
[[language]]
name = "typescript"
file-types = ["ts"]
language-servers = ["ts"]
[[language]]
name = "typescript"
file-types = ["tsx"]
language-servers = ["ts"]
"#,
                "duplicate language server",
            ),
        ] {
            let error = parse_language_servers(input).unwrap_err();
            assert!(
                error.0.contains(expected),
                "{} did not contain {expected}",
                error.0
            );
        }
    }
}
