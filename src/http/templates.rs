use crate::{bluetooth::BluetoothAddress, sensor::SensorState};
use askama::Template;

#[derive(Template)]
#[template(path = "home.html")]
pub(crate) struct Home<'a> {
    sensors: &'a Vec<(BluetoothAddress, SensorEntry)>,
}

#[derive(Debug)]
pub(crate) struct SensorEntry {
    pub(crate) state: SensorState,
    pub(crate) label: Option<String>,
}

impl<'a> Home<'a> {
    pub(crate) fn new(sensors: &'a Vec<(BluetoothAddress, SensorEntry)>) -> Self {
        Self { sensors }
    }
}
