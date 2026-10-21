//! 固定启动工作区，以目录句柄访问相对路径。
use std::{
    io,
    path::{Component, Path},
    sync::Arc,
};

use cap_fs_ext::DirExt;
use cap_std::{ambient_authority, fs::Dir};

use vortex_core::ToolError;

const MAX_PATH_BYTES: usize = 4096;
const MAX_COMPONENTS: usize = 32;

#[derive(Clone)]
pub struct WorkspaceFs {
    root: Arc<Dir>,
}

impl WorkspaceFs {
    pub fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        if !root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "工作区根目录必须为绝对路径",
            ));
        }
        let root = Dir::open_ambient_dir(root, ambient_authority())?;
        Ok(Self {
            root: Arc::new(root),
        })
    }

    pub(crate) fn open_directory(&self, path: &str) -> Result<(Dir, String), ToolError> {
        let components = normalize_components(path)?;
        let normalized = if components.is_empty() {
            ".".to_owned()
        } else {
            components.join("/")
        };

        let mut directory = self
            .root
            .try_clone()
            .map_err(|error| execution_error("无法打开工作区句柄", error))?;
        for component in components {
            directory = directory
                .open_dir_nofollow(component)
                .map_err(|error| execution_error("目录不可访问、不是目录或包含符号链接", error))?;
        }
        Ok((directory, normalized))
    }
}

fn normalize_components(path: &str) -> Result<Vec<&str>, ToolError> {
    let invalid = || ToolError::InvalidArguments("path 必须为允许的工作区相对路径".into());
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.chars().any(char::is_control)
        || path.contains(['\\', ':'])
    {
        return Err(invalid());
    }

    let mut components = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => {
                let name = name.to_str().ok_or_else(invalid)?;
                if !is_supported_name(name) || is_blocked_name(name) {
                    return Err(invalid());
                }
                components.push(name);
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(invalid());
            }
        }
    }
    if components.len() > MAX_COMPONENTS {
        return Err(invalid());
    }
    Ok(components)
}

pub(crate) fn is_blocked_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(name.as_str(), ".git" | ".vortex" | ".env") || name.starts_with(".env.")
}

pub(crate) fn is_supported_name(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().any(char::is_control)
        && !name.contains(['\\', ':'])
        && !name.ends_with([' ', '.'])
}

pub(crate) fn execution_error(context: &str, error: io::Error) -> ToolError {
    // 不把底层错误中的宿主绝对路径发给模型。
    ToolError::Execution(format!("{context}: {:?}", error.kind()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn normalizes_relative_paths_and_preserves_spaces() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("my project/src")).unwrap();
        let workspace = WorkspaceFs::open(root.path()).unwrap();
        assert_eq!(workspace.open_directory(".").unwrap().1, ".");
        assert_eq!(
            workspace.open_directory("./my project/./src/").unwrap().1,
            "my project/src",
        );
    }

    #[test]
    fn rejects_unsafe_and_blocked_paths() {
        let root = tempdir().unwrap();
        let workspace = WorkspaceFs::open(root.path()).unwrap();
        for path in [
            "",
            "..",
            "a/../b",
            "/tmp",
            "C:/tmp",
            r"a\b",
            "a\nfile",
            ".git",
            ".GIT/objects",
            "a/.vortex",
            ".env",
            ".env.example",
            "a. ",
        ] {
            assert!(
                matches!(
                    workspace.open_directory(path),
                    Err(ToolError::InvalidArguments(_))
                ),
                "should reject {path:?}"
            );
        }
        let deep = std::iter::repeat_n("a", MAX_COMPONENTS + 1)
            .collect::<Vec<_>>()
            .join("/");
        assert!(matches!(
            workspace.open_directory(&deep),
            Err(ToolError::InvalidArguments(_))
        ));
        assert!(matches!(
            workspace.open_directory(&"a".repeat(MAX_PATH_BYTES + 1)),
            Err(ToolError::InvalidArguments(_))
        ));
    }

    #[test]
    fn rejects_relative_root_and_does_not_leak_absolute_paths() {
        assert!(WorkspaceFs::open(".").is_err());
        let root = tempdir().unwrap();
        fs::write(root.path().join("file"), "fixture").unwrap();
        let workspace = WorkspaceFs::open(root.path()).unwrap();
        for path in ["missing", "file"] {
            let error = workspace.open_directory(path).err().unwrap().to_string();
            assert!(!error.contains(root.path().to_str().unwrap()));
        }
    }

    #[test]
    fn normalization_limits_are_inclusive_and_byte_based() {
        let deepest = std::iter::repeat_n("a", MAX_COMPONENTS)
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(
            normalize_components(&deepest).unwrap().len(),
            MAX_COMPONENTS
        );

        let exact = format!("{}a", "中".repeat(MAX_PATH_BYTES / 3));
        assert_eq!(exact.len(), MAX_PATH_BYTES);
        assert!(normalize_components(&exact).is_ok());
        assert!(matches!(
            normalize_components(&format!("{exact}a")),
            Err(ToolError::InvalidArguments(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_internal_external_and_intermediate_symlinks() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::create_dir_all(root.path().join("real/sub")).unwrap();
        symlink(root.path().join("real"), root.path().join("inside")).unwrap();
        symlink(outside.path(), root.path().join("outside")).unwrap();
        let workspace = WorkspaceFs::open(root.path()).unwrap();
        for path in ["inside", "inside/sub", "outside"] {
            assert!(workspace.open_directory(path).is_err());
        }
    }
}
