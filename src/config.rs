use directories_next::ProjectDirs;
use eyre::Context;
use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    num::NonZeroU8,
    path::PathBuf,
};
use tokio_mqtt as mqtt;

#[derive(serde::Deserialize)]
struct EnvConfig {
    mqtt_server_url: Option<url::Url>,
    mqtt_cert_file: Option<PathBuf>,
    #[serde(default = "default_host")]
    pub host: IpAddr,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    pub demo: Option<NonZeroU8>,
}

fn default_host() -> IpAddr {
    Ipv4Addr::LOCALHOST.into()
}

fn default_port() -> u16 {
    8080
}

fn default_db_path() -> PathBuf {
    let dirs = ProjectDirs::from("org", "foldu", env!("CARGO_PKG_NAME"))
        .ok_or_else(|| eyre::format_err!("Could not get project directories"))
        .unwrap();
    dirs.data_dir()
        .join(concat!(env!("CARGO_PKG_NAME"), ".mdb"))
}

pub(crate) struct Config {
    pub mqtt_options: Option<mqtt::ConnectOptions>,
    pub host: IpAddr,
    pub port: u16,
    pub db_path: PathBuf,
    pub demo: Option<NonZeroU8>,
}

impl Config {
    pub fn from_env() -> Result<Self, eyre::Error> {
        let env_config: EnvConfig =
            envy::from_env().context("Could not read config from environment")?;
        let mqtt_options = env_config.mqtt_server_url.as_ref().map(|url| -> Result<_, eyre::Error> {
            let ssl = if url.scheme() == "mqtts" {
                let cert_path = env_config
                    .mqtt_cert_file
                    .as_ref()
                    .ok_or_else(|| eyre::format_err!("Need a cert file for mqtts url but environment variable MQTT_CERT_FILE was not set"))?;
                let pem = fs::read(&cert_path)
                    .with_context(|| eyre::format_err!("Could not read cert pem from {}", cert_path.display()))?;
                mqtt::Ssl::WithCert(pem)
            } else {
                mqtt::Ssl::None
            };
            mqtt::ConnectOptions::new(&url, ssl).map_err(|e| e.into())
        }).transpose()?;

        Ok(Self {
            mqtt_options,
            host: env_config.host,
            port: env_config.port,
            db_path: env_config.db_path,
            demo: env_config.demo,
        })
    }
}
