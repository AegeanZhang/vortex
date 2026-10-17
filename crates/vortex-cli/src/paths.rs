use std::{env, ffi::OsString, io, path::PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct AppPaths {
    pub(crate) home: PathBuf,
    pub(crate) cwd: PathBuf,
    pub(crate) user_config: PathBuf,
    pub(crate) project_config: PathBuf,
    pub(crate) log_dir: PathBuf,
}

impl AppPaths {
    pub(crate) fn resolve() -> io::Result<Self> {
        let explicit = env::var_os("VORTEX_HOME");
        // 显式覆盖有效时，不依赖系统主目录是否可以查询成功。
        let user_home = if explicit.is_none() {
            env::home_dir()
        } else {
            None
        };
        Self::from_inputs(explicit, user_home, env::current_dir()?)
    }

    fn from_inputs(
        explicit: Option<OsString>,
        user_home: Option<PathBuf>,
        cwd: PathBuf,
    ) -> io::Result<Self> {
        let home = match explicit {
            Some(value) if value.is_empty() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "VORTEX_HOME 不能为空",
                ));
            }
            Some(value) => PathBuf::from(value),
            None => {
                let root = user_home.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "无法确定用户主目录, 请显式设置 VORTEX_HOME",
                    )
                })?;
                if !root.is_absolute() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "用户主目录不是有效的绝对路径",
                    ));
                }
                root.join(".vortex")
            }
        };
        if !home.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "VORTEX_HOME 必须是绝对路径",
            ));
        }
        if !cwd.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "启动工作目录必须是绝对路径",
            ));
        }
        Ok(Self {
            user_config: home.join("config.toml"),
            project_config: cwd.join(".vortex").join("project.toml"),
            log_dir: home.join("logs"),
            home,
            cwd,
        })
    }
}
