use super::error::ProjectLoadError;
use super::model::{DependencyKind, DependencySpec, WorkspaceManifest};
use std::path::{Path, PathBuf};
use toml::Value;
use toml::value::Table;

pub(crate) const MANIFEST_FILE_NAME: &str = "corex.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManifestRole {
    Workspace(WorkspaceManifest),
    Project(RawProjectManifest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawProjectManifest {
    pub(crate) name: String,
    pub(crate) library_declared: bool,
    pub(crate) library_name: Option<String>,
    pub(crate) binaries: Vec<RawBinaryTarget>,
    pub(crate) dependencies: Vec<DependencySpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawBinaryTarget {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

pub(crate) fn parse_manifest_toml(
    manifest_path: &Path,
    source: &str,
) -> Result<ManifestRole, ProjectLoadError> {
    let value = source.parse::<Value>().map_err(|error| {
        ProjectLoadError::InvalidManifest {
            manifest_path: manifest_path.to_path_buf(),
            message: error.to_string(),
        }
    })?;

    let Some(root) = value.as_table() else {
        return Err(ProjectLoadError::InvalidManifest {
            manifest_path: manifest_path.to_path_buf(),
            message: "manifest root must be a TOML table".to_string(),
        });
    };

    let has_workspace = root.contains_key("workspace");
    let has_project = root.contains_key("project");
    match (has_workspace, has_project) {
        (true, false) => Ok(ManifestRole::Workspace(parse_workspace_manifest(
            root,
            manifest_path,
        )?)),
        (false, true) => Ok(ManifestRole::Project(parse_project_manifest(
            root,
            manifest_path,
        )?)),
        (true, true) => Err(ProjectLoadError::AmbiguousManifestRole {
            manifest_path: manifest_path.to_path_buf(),
        }),
        (false, false) => Err(ProjectLoadError::UnsupportedManifestShape {
            manifest_path: manifest_path.to_path_buf(),
            message: "manifest must declare either [workspace] or [project]"
                .to_string(),
        }),
    }
}

fn parse_workspace_manifest(
    root: &Table,
    manifest_path: &Path,
) -> Result<WorkspaceManifest, ProjectLoadError> {
    let workspace = required_table(root, "workspace", manifest_path)?;
    let name = required_string(workspace, "name", "workspace", manifest_path)?;

    let members = match workspace.get("members") {
        None => Vec::new(),
        Some(value) => {
            let Some(member_values) = value.as_array() else {
                return invalid_manifest(
                    manifest_path,
                    "`workspace.members` must be an array of paths",
                );
            };

            let mut parsed_members = Vec::with_capacity(member_values.len());
            for member in member_values {
                let Some(member_str) = member.as_str() else {
                    return invalid_manifest(
                        manifest_path,
                        "`workspace.members` entries must be strings",
                    );
                };
                if member_str.trim().is_empty() {
                    return invalid_manifest(
                        manifest_path,
                        "`workspace.members` entries cannot be empty",
                    );
                }
                parsed_members.push(PathBuf::from(member_str));
            }
            parsed_members
        }
    };

    Ok(WorkspaceManifest { name, members })
}

fn parse_project_manifest(
    root: &Table,
    manifest_path: &Path,
) -> Result<RawProjectManifest, ProjectLoadError> {
    let project = required_table(root, "project", manifest_path)?;
    let name = required_string(project, "name", "project", manifest_path)?;

    let (library_declared, library_name) =
        parse_library_decl(root.get("lib"), manifest_path)?;
    let binaries = parse_bins(root.get("bin"), manifest_path)?;
    let mut dependencies =
        parse_dependencies(root.get("dependencies"), manifest_path)?;
    dependencies.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(RawProjectManifest {
        name,
        library_declared,
        library_name,
        binaries,
        dependencies,
    })
}

fn parse_library_decl(
    value: Option<&Value>,
    manifest_path: &Path,
) -> Result<(bool, Option<String>), ProjectLoadError> {
    let Some(value) = value else {
        return Ok((false, None));
    };

    if value.is_array() {
        return Err(ProjectLoadError::MultipleLibrariesDeclared {
            manifest_path: manifest_path.to_path_buf(),
        });
    }

    let Some(lib_table) = value.as_table() else {
        return invalid_manifest(
            manifest_path,
            "`lib` must be a table when present",
        );
    };

    let library_name =
        optional_string(lib_table, "name", "lib", manifest_path)?;
    Ok((true, library_name))
}

fn parse_bins(
    value: Option<&Value>,
    manifest_path: &Path,
) -> Result<Vec<RawBinaryTarget>, ProjectLoadError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };

    let Some(entries) = value.as_array() else {
        return invalid_manifest(
            manifest_path,
            "`bin` must be an array of tables",
        );
    };

    let mut bins = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(table) = entry.as_table() else {
            return invalid_manifest(
                manifest_path,
                "each `bin` entry must be a table",
            );
        };
        let name = required_string(table, "name", "bin", manifest_path)?;
        let path = required_string(table, "path", "bin", manifest_path)?;
        bins.push(RawBinaryTarget {
            name,
            path: PathBuf::from(path),
        });
    }

    Ok(bins)
}

fn parse_dependencies(
    value: Option<&Value>,
    manifest_path: &Path,
) -> Result<Vec<DependencySpec>, ProjectLoadError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };

    let Some(table) = value.as_table() else {
        return invalid_manifest(
            manifest_path,
            "`dependencies` must be a table",
        );
    };

    let mut dependencies = Vec::with_capacity(table.len());
    for (name, spec_value) in table {
        if name.trim().is_empty() {
            return invalid_manifest(
                manifest_path,
                "dependency name cannot be empty",
            );
        }

        let Some(spec_table) = spec_value.as_table() else {
            return invalid_manifest(
                manifest_path,
                &format!("dependency `{name}` must be a table"),
            );
        };

        let path = optional_string(
            spec_table,
            "path",
            &format!("dependencies.{name}"),
            manifest_path,
        )?;
        let git = optional_string(
            spec_table,
            "git",
            &format!("dependencies.{name}"),
            manifest_path,
        )?;

        let kind = match (path, git) {
            (Some(path), None) => DependencyKind::Path {
                path: PathBuf::from(path),
            },
            (None, Some(git)) => DependencyKind::Git { git },
            (Some(_), Some(_)) => {
                return invalid_manifest(
                    manifest_path,
                    &format!(
                        "dependency `{name}` cannot declare both `path` and `git`"
                    ),
                );
            }
            (None, None) => {
                return invalid_manifest(
                    manifest_path,
                    &format!(
                        "dependency `{name}` must declare either `path` or `git`"
                    ),
                );
            }
        };

        dependencies.push(DependencySpec {
            name: name.clone(),
            kind,
        });
    }

    Ok(dependencies)
}

fn required_table<'a>(
    table: &'a Table,
    key: &str,
    manifest_path: &Path,
) -> Result<&'a Table, ProjectLoadError> {
    let Some(value) = table.get(key) else {
        return invalid_manifest(
            manifest_path,
            &format!("missing required section `{key}`"),
        );
    };
    let Some(value_table) = value.as_table() else {
        return invalid_manifest(
            manifest_path,
            &format!("`{key}` must be a table"),
        );
    };
    Ok(value_table)
}

fn required_string(
    table: &Table,
    key: &str,
    context: &str,
    manifest_path: &Path,
) -> Result<String, ProjectLoadError> {
    let Some(value) = table.get(key) else {
        return invalid_manifest(
            manifest_path,
            &format!("missing `{key}` in `{context}`"),
        );
    };
    let Some(value_str) = value.as_str() else {
        return invalid_manifest(
            manifest_path,
            &format!("`{context}.{key}` must be a string"),
        );
    };
    if value_str.trim().is_empty() {
        return invalid_manifest(
            manifest_path,
            &format!("`{context}.{key}` cannot be empty"),
        );
    }
    Ok(value_str.to_string())
}

fn optional_string(
    table: &Table,
    key: &str,
    context: &str,
    manifest_path: &Path,
) -> Result<Option<String>, ProjectLoadError> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };
    let Some(value_str) = value.as_str() else {
        return invalid_manifest(
            manifest_path,
            &format!("`{context}.{key}` must be a string"),
        );
    };
    if value_str.trim().is_empty() {
        return invalid_manifest(
            manifest_path,
            &format!("`{context}.{key}` cannot be empty"),
        );
    }
    Ok(Some(value_str.to_string()))
}

fn invalid_manifest<T>(
    manifest_path: &Path,
    message: &str,
) -> Result<T, ProjectLoadError> {
    Err(ProjectLoadError::InvalidManifest {
        manifest_path: manifest_path.to_path_buf(),
        message: message.to_string(),
    })
}
