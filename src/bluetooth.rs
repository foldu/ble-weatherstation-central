mod address;
mod dbus_interfaces;
pub use address::BluetoothAddress;
use tokio::sync::RwLock;

use crate::sensor::{Celsius, Pascal, RelativeHumidity};
use byteorder::ByteOrder;
use dbus_interfaces::{Adapter1Proxy, Battery1Proxy, Device1Proxy, GattCharacteristic1Proxy};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    future::Future,
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;
use zbus::fdo::ObjectManagerProxy;
use zvariant::{Array, ObjectPath, OwnedObjectPath, OwnedValue};

use crate::sensor::SensorValues;

pub struct Bluez {
    dbus: zbus::Connection,
}

struct ConstUuid {
    s: &'static str,
    u: Uuid,
}

const BLE_GATT_SERVICE_ENVIRONMENTAL_SENSING: ConstUuid = ConstUuid {
    s: "0000180f-0000-1000-8000-00805f9b34fb",
    u: Uuid::from_u128(0x180F00001000800000805F9B34FB),
};

const BLE_GATT_SERVICE_WEATHERSTATION: ConstUuid = ConstUuid {
    s: "e7364bd3-a1c5-4924-847d-3a9cd6e343ef",
    u: Uuid::from_u128(307333589004604602091860631388298626031),
};

struct Weatherstation {
    battery_path: OwnedObjectPath,
    temperature_path: OwnedObjectPath,
    humidity_path: OwnedObjectPath,
    pressure_path: OwnedObjectPath,
}

fn env_sensing_chr<'a>(device_path: &str, chr: &str) -> OwnedObjectPath {
    ObjectPath::try_from(format!("{}/service000a/{}", device_path, chr).as_str())
        .unwrap()
        .into()
}

impl Weatherstation {
    fn from_device_path(device_path: OwnedObjectPath) -> Self {
        Self {
            pressure_path: env_sensing_chr(&device_path, "char000f"),
            humidity_path: env_sensing_chr(&device_path, "char000d"),
            temperature_path: env_sensing_chr(&device_path, "char000b"),
            battery_path: device_path,
        }
    }

    fn read_values(&self, dbus: &zbus::Connection) -> Result<SensorValues, eyre::Error> {
        let temperature =
            Self::read_with(dbus, &self.temperature_path, byteorder::BigEndian::read_i16)?;

        let pressure = Self::read_with(dbus, &self.pressure_path, byteorder::BigEndian::read_u32)?;

        let humidity = Self::read_with(dbus, &self.humidity_path, byteorder::BigEndian::read_u16)?;

        Ok(SensorValues {
            temperature: Celsius::try_from(temperature)?,
            pressure: Pascal::from(pressure),
            humidity: RelativeHumidity::try_from(humidity)?,
        })
    }

    fn read_with<T, F>(
        dbus: &zbus::Connection,
        path: &OwnedObjectPath,
        mut f: F,
    ) -> Result<T, zbus::Error>
    where
        F: FnMut(&[u8]) -> T,
    {
        let value = GattCharacteristic1Proxy::new_for(dbus, "org.bluez", path)?
            .read_value(HashMap::new())?;
        Ok(f(&value))
    }
}

pub(crate) fn create_bluetooth_tasks(
    ctx: crate::Context,
) -> Result<
    (
        impl Future<Output = Result<(), eyre::Error>> + Send,
        impl Future<Output = Result<(), eyre::Error>> + Send,
    ),
    eyre::Error,
> {
    let dbus = zbus::Connection::new_system()?;
    let connected_devices = Arc::new(RwLock::new(BTreeMap::new()));
    let poll_task = {
        let connected_devices = connected_devices.clone();
        async move {
            let bluez_object_proxy = ObjectManagerProxy::new_for(&dbus, "org.bluez", "/")?;
            loop {
                let wait_duration = {
                    let mut wait_duration = Duration::from_secs(30);
                    let objs = bluez_object_proxy.get_managed_objects()?;
                    let mut connected_devices_r = connected_devices.read().await;
                    for (object_path, interfaces) in objs {
                        if let Some(obj) = interpret_object(&object_path, interfaces) {
                            match obj {
                                BluezObject::Interface { discovering: false } => {
                                    tokio::task::block_in_place(|| {
                                        Adapter1Proxy::new_for(
                                            &dbus,
                                            "org.bluez",
                                            object_path.as_str(),
                                        )?
                                        .start_discovery()
                                    })?;
                                }
                                BluezObject::WeatherstationDevice {
                                    connected: false, ..
                                } => {
                                    tokio::task::block_in_place(|| {
                                        Device1Proxy::new_for(
                                            &dbus,
                                            "org.bluez",
                                            object_path.as_str(),
                                        )?
                                        .connect()
                                    })?;
                                    // takes about 8 secs to connect
                                    wait_duration = Duration::from_secs(10);
                                }
                                BluezObject::WeatherstationDevice {
                                    services_resolved: true,
                                    address,
                                    ..
                                } if !connected_devices_r.contains_key(&address) => {
                                    let ws = Weatherstation::from_device_path(object_path);
                                    drop(connected_devices_r);
                                    connected_devices.write().await.insert(address, ws);
                                    connected_devices_r = connected_devices.read().await;
                                }
                                _ => {}
                            }
                        }
                    }

                    wait_duration
                };

                tokio::time::delay_for(wait_duration).await;
            }

            Ok(())
        }
    };

    let sensor_read_task = {
        let connected_devices = connected_devices.clone();
        async move {
            loop {
                {
                    let connected_devices = connected_devices.read().await;
                    for (id, ws) in &*connected_devices {}
                }
                tokio::time::delay_for(Duration::from_secs(31)).await;
            }
            Ok(())
        }
    };

    Ok((poll_task, sensor_read_task))
}

#[derive(Debug)]
enum CharacteristicKind {
    Pressure,
    Temperature,
    Humidity,
}

#[derive(Debug)]
enum BluezObject {
    Interface {
        discovering: bool,
    },

    WeatherstationDevice {
        address: BluetoothAddress,
        connected: bool,
        services_resolved: bool,
    },

    WeatherstationCharacteristic(CharacteristicKind),
}

fn interpret_object(
    object_path: &OwnedObjectPath,
    interfaces: HashMap<String, HashMap<String, OwnedValue>>,
) -> Option<BluezObject> {
    static PATH_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"^/org/bluez/(?P<intf>[^/]+)(/dev_(?P<device>[^/]+))?$"#).unwrap()
    });

    let path = object_path.as_str().strip_prefix("/org/bluez/").split('/');

    match path {
        [interface, device, "service000a", chr] => {}

        [interface, device] => {}

        [interface] => {
            let bluez_adapter = interfaces.get("org.bluez.Adapter1")?;
            let discovering = *bluez_adapter.get("Discovering")?.downcast_ref::<bool>()?;
            Some(BluezObject::Interface { discovering })
        }
    }

    let caps = PATH_RE.captures(object_path.as_str())?;
    let device = caps.name("intf").unwrap();
    match caps.name("device") {
        Some(dev) => {
            let dev = dev.as_str().replace('_', ":");
            let bluez_device = interfaces.get("org.bluez.Device1")?;
            let uuid_array = bluez_device.get("UUIDs")?.downcast_ref::<Array>()?;

            let (mut environmental_sensing, mut weatherstation) = (false, false);
            for uuid in uuid_array.get() {
                if let zvariant::Value::Str(s) = uuid {
                    if s.as_str() == BLE_GATT_SERVICE_ENVIRONMENTAL_SENSING.s {
                        environmental_sensing = true;
                    } else if s.as_str() == BLE_GATT_SERVICE_WEATHERSTATION.s {
                        weatherstation = true;
                    }
                }
            }
            if !(environmental_sensing && weatherstation) {
                return None;
            }
            let connected = *bluez_device.get("Connected")?.downcast_ref::<bool>()?;
            let address = bluez_device
                .get("Address")?
                .downcast_ref::<zvariant::Str>()?;
            let services_resolved = *bluez_device
                .get("ServicesResolved")?
                .downcast_ref::<bool>()?;

            Some(BluezObject::WeatherstationDevice {
                connected,
                address: BluetoothAddress::parse_str(address.as_str()).ok()?,
                services_resolved,
            })
        }
        None => {
            let bluez_adapter = interfaces.get("org.bluez.Adapter1")?;
            let discovering = *bluez_adapter.get("Discovering")?.downcast_ref::<bool>()?;
            Some(BluezObject::Interface { discovering })
        }
    }
}
