use config::{Config, FileFormat, File};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Conf {
    #[serde(default)]
    pub retry: u64,
    #[serde(default)]
    pub idle_timeout: u64,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub log: LogConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LogConfig {
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub file: String,
}
impl Default for LogConfig {
    fn default() -> Self {
	LogConfig{level: "info".to_string(), file: "".to_string()}
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Account {
    pub host: String,
    pub user: String,
    pub pass: String,
    pub tls: bool,
    pub commands: Vec<String>,
    pub port: Option<u16>,
    pub name: Option<String>,
    pub folders: Option<Vec<String>>,
}
impl Default for Account {
    fn default() -> Self {
	Account{
	    folders: None,
	    commands: vec![],
	    host: "localhost".to_string(),
	    port: Some(993),
	    tls: true,
	    user: "".to_string(),
	    pass: "".to_string(),
	    name: Some("default".to_string()),
	}
    }
}

impl Conf {

    pub fn new(path: Option<&String>) -> Result<Self, config::ConfigError> {

	// load up defaults
	let mut s = Config::builder();

	let config_path: String = match path {
	    Some(path) => path.to_string(),
	    None => {
		let xdg_dirs = xdg::BaseDirectories::with_prefix("mbotrs").unwrap();
		match xdg_dirs.find_config_file("config.yaml") {
		    Some(path) => path.into_os_string().into_string().unwrap(),
		    None => String::default()
		}
	    }
	};

	if config_path.is_empty() {
	    info!("no configuration file found, using defaults");
	} else {
	    info!("loading configuration file from {}", config_path);
	    s = s.add_source(File::new(&config_path, FileFormat::Yaml));
	}

        let config = s.build()?;
	config.try_deserialize()
    }
}
