use clap::Clap;

/// Central server for a number of ble-weatherstations
#[derive(Clap)]
pub(crate) struct Opt {
    //    /// Run with n dummy sensors
//    #[clap(long)]
//    pub(crate) demo: Option<NonZeroU8>,
//
//    /// Path to database
//    #[clap(long)]
//    pub(crate) db_path: Option<PathBuf>,
//
//    /// Port to listen on
//    #[clap(long, default_value = "8080")]
//    pub(crate) port: u16,
//
//    /// Host to bind to
//    #[clap(long, default_value = "127.0.0.1")]
//    pub(crate) host: IpAddr,
//
//    /// mqtt server url
//    #[clap(long)]
//    pub(crate) mqtt_server_url: Option<url::Url>,
}
