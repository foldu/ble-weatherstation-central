use crate::sensor::SensorState;
use askama::Template;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "home.html")]
pub(crate) struct Home<'a> {
    sensors: &'a Vec<(Uuid, SensorEntry)>,
}

pub(crate) struct SensorEntry {
    pub(crate) state: SensorState,
    pub(crate) label: Option<String>,
}

impl<'a> Home<'a> {
    pub(crate) fn new(sensors: &'a Vec<(Uuid, SensorEntry)>) -> Self {
        Self { sensors }
    }
}
