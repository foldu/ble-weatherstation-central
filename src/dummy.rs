use crate::{
    bluetooth::BluetoothAddress,
    sensor::{Celsius, Pascal, RelativeHumidity, SensorState, SensorValues},
};
use futures_util::stream::Stream;
use rand::Rng;
use std::{collections::BTreeMap, convert::TryFrom, future::Future, time::Duration};
use tokio::sync::mpsc;

struct FluctuatingSensor {
    humidity: u16,
    temperature: i16,
    pressure: u32,
}

impl Default for FluctuatingSensor {
    fn default() -> Self {
        Self {
            humidity: 50_00,
            // TODO: better default for pressure
            pressure: 100000,
            temperature: 20_00,
        }
    }
}

fn clamp<T>(n: T, lo: T, hi: T) -> T
where
    T: Ord + Copy,
{
    if n < lo {
        lo
    } else if n > hi {
        hi
    } else {
        n
    }
}

impl Iterator for FluctuatingSensor {
    type Item = SensorValues;

    fn next(&mut self) -> Option<Self::Item> {
        let mut rng = rand::thread_rng();
        self.temperature = clamp(self.temperature + rng.gen_range(-1_00, 1_00), 0_00, 30_00);
        self.pressure = clamp(
            // TODO: better range for pressure
            if rng.gen::<bool>() {
                self.pressure + rng.gen_range(100, 1000)
            } else {
                self.pressure - rng.gen_range(100, 1000)
            },
            10000,
            100000,
        );
        self.humidity = clamp(
            if rng.gen::<bool>() {
                self.humidity + rng.gen_range(0_20, 0_90)
            } else {
                self.humidity - rng.gen_range(0_20, 0_90)
            },
            20_00,
            90_00,
        );
        Some(SensorValues {
            humidity: RelativeHumidity::try_from(self.humidity).unwrap(),
            pressure: Pascal::from(self.pressure),
            temperature: Celsius::try_from(self.temperature).unwrap(),
        })
    }
}

pub(crate) fn dummy_sensor() -> (
    impl Future<Output = ()>,
    impl Stream<Item = BTreeMap<BluetoothAddress, SensorState>> + Sync + Send,
) {
    let (mut tx, rx) = mpsc::channel(1);
    let dummy_task = async move {
        let mut map = BTreeMap::new();
        let addr = BluetoothAddress::parse_str("00:00:00:00:00:00").unwrap();

        for value in FluctuatingSensor::default() {
            map.insert(addr, SensorState::Connected(value));
            if let Err(_) = tx.send(map.clone()).await {
                break;
            }
            tokio::time::delay_for(Duration::from_secs(30)).await;
        }
    };

    (dummy_task, rx)
}
