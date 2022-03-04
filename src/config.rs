use dirs::home_dir;
use serde::Deserialize;
use std::io::Write;

use crate::process_manager::{Cmd, CmdName, CmdTable};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    TomlError(#[from] toml::de::Error),

    #[error("failed to get $HOME dir")]
    FaildToGetHome,

    #[error("{0}")]
    InvalidPath(PathBuf),
}

type Result<T> = std::result::Result<T, ConfigError>;

const DEFAULT_OUTPUT_SIZE: usize = 4 * 1024;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub cmds: Vec<CmdConfig>,
}

#[derive(Debug, Deserialize)]
pub struct CmdConfig {
    pub name: CmdName,
    pub cmd: String,
    pub output_size: Option<usize>,
    pub auto_trailing_newline: Option<bool>,
    pub join_input_newline_with: Option<String>,
    pub truncate_line_regex: Option<String>,
    pub remove_empty_line: bool,
    pub no_empty_input: bool,
    pub timeout_sec: Option<u64>,
    pub wait_output_timeout_milli_sec: Option<u64>,
}

impl Config {
    pub fn load_from_default_path_or_create() -> Result<Self> {
        let config_path = Self::default_config_path()?;

        if !config_path.exists() {
            Self::create_default_toml(&config_path)?;
        }

        let config_file_contents = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(config_file_contents.as_ref())?;

        Ok(config)
    }

    fn create_default_toml(config_path: &PathBuf) -> Result<()> {
        let dir = config_path
            .parent()
            .ok_or_else(|| ConfigError::InvalidPath(config_path.to_path_buf()))?;

        tracing::debug!("creating default config at {:?}", config_path);
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }
        let mut config_file = fs::File::create(config_path)?;
        config_file.write_all(DEFAULT_CONFIG.as_bytes())?;

        Ok(())
    }

    fn default_config_path() -> Result<PathBuf> {
        let mut dir = home_dir().ok_or(ConfigError::FaildToGetHome)?;
        dir.push(".config/dairi/config.toml");
        Ok(dir)
    }
    pub fn as_cmd_table(&self) -> CmdTable {
        let mut cmd_table = CmdTable::new();

        for CmdConfig {
            name,
            cmd,
            output_size,
            auto_trailing_newline,
            join_input_newline_with,
            truncate_line_regex,
            remove_empty_line,
            no_empty_input,
            timeout_sec,
            wait_output_timeout_milli_sec: wait_output_timeout_sec,
        } in self.cmds.iter()
        {
            cmd_table.insert(
                name.clone(),
                Cmd::new(
                    name.clone(),
                    cmd.clone(),
                    output_size.unwrap_or(DEFAULT_OUTPUT_SIZE),
                    auto_trailing_newline.unwrap_or(false),
                    join_input_newline_with.clone(),
                    truncate_line_regex.clone(),
                    *remove_empty_line,
                    *no_empty_input,
                    *timeout_sec,
                    *wait_output_timeout_sec,
                ),
            );
        }

        cmd_table
    }
}

const DEFAULT_CONFIG: &str = r##"
[[cmds]]
name = "julia"
cmd = "julia"
output_size = 4096
join_input_newline_with = ";"
auto_trailing_newline = true
truncate_line_regex = "#.*"
remove_empty_line = true
no_empty_input = true
timeout_sec = 120
wait_output_timeout_milli_sec = 500

"##;

#[cfg(test)]
mod test {

    use super::*;
    #[test]
    fn test_parse_default_config() {
        let config: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(config.cmds.len(), 1);
        assert_eq!(config.cmds[0].cmd, "julia");
        assert_eq!(config.cmds[0].name, "julia");
    }
}
