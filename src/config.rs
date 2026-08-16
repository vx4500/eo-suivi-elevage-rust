use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let data_dir = std::env::var_os("ELEVAGE_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data"));
        std::fs::create_dir_all(&data_dir)?;

        let host = std::env::var("EO_HOST")
            .ok()
            .and_then(|v| v.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let port = std::env::var("EO_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8080);
        let db_path = data_dir.join("elevage.db");

        Ok(Self {
            bind: SocketAddr::new(host, port),
            db_path,
        })
    }
}
