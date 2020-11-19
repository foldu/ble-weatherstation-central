use crate::{bluetooth::BluetoothAddress, sensor::SensorState};
use askama::Template;
use derive_more::Constructor;

#[derive(Template, Constructor)]
#[template(path = "home.html")]
pub(crate) struct Home<'a> {
    sensors: &'a Vec<(BluetoothAddress, SensorEntry)>,
}

#[derive(Debug)]
pub(crate) struct SensorEntry {
    pub(crate) state: SensorState,
    pub(crate) label: Option<String>,
}

#[derive(Debug, Constructor, Template)]
#[template(path = "error.html")]
pub(crate) struct Error {
    code: warp::http::StatusCode,
}

#[derive(Debug, Constructor, Template)]
#[template(path = "detail.html")]
pub(crate) struct Detail {
    pub(crate) addr: BluetoothAddress,
}
