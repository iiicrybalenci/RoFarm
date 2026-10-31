use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub portable: bool,
}

impl AppPaths {
    pub fn discover() -> Self {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join("portable").exists() || dir.join("RoFarmData").exists() {
                    return Self { root: dir.join("RoFarmData"), portable: true };
                }
            }
        }
        let root = dirs::data_local_dir()
            .or_else(dirs::data_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RoFarm");
        Self { root, portable: false }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), portable: false }
    }

    pub fn vault(&self) -> PathBuf {
        self.root.join("vault.rofarmvault")
    }

    pub fn audit(&self) -> PathBuf {
        self.root.join("audit.jsonl")
    }

    pub fn events(&self) -> PathBuf {
        self.root.join("events.jsonl")
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn thumbnails(&self) -> PathBuf {
        self.root.join("cache").join("thumbnails")
    }

    pub fn screenshots(&self) -> PathBuf {
        self.root.join("screenshots")
    }

    pub fn plugins(&self) -> PathBuf {
        self.root.join("plugins")
    }

    pub fn workspaces(&self) -> PathBuf {
        self.root.join("workspaces")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [
            self.root.clone(),
            self.logs(),
            self.backups(),
            self.cache(),
            self.thumbnails(),
            self.screenshots(),
            self.plugins(),
            self.workspaces(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn harden(&self) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            let user = std::env::var("USERNAME").unwrap_or_default();
            if !user.is_empty() {
                let grant = format!("{user}:(OI)(CI)F");
                let _ = std::process::Command::new("icacls")
                    .arg(&self.root)
                    .arg("/inheritance:r")
                    .arg("/grant:r")
                    .arg(grant)
                    .creation_flags(0x0800_0000)
                    .output();
            }
        }
        Ok(())
    }

    pub fn is_inside_root(&self, path: &Path) -> bool {
        let Ok(root) = self.root.canonicalize() else { return false };
        path.canonicalize().map(|p| p.starts_with(root)).unwrap_or(false)
    }
}

#[cfg(windows)]
trait NoConsoleWindow {
    fn creation_flags(&mut self, flags: u32) -> &mut Self;
}

#[cfg(windows)]
impl NoConsoleWindow for std::process::Command {
    fn creation_flags(&mut self, flags: u32) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            CommandExt::creation_flags(self, flags);
        }
        self
    }
}
