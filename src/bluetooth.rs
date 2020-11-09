mod address;
mod dbus_interfaces;
pub use address::BluetoothAddress;
use tokio::sync::oneshot;

use crate::sensor::{Celsius, Pascal, RelativeHumidity, SensorState};
use byteorder::ByteOrder;
use dbus_interfaces::{Adapter1Proxy, Device1Proxy, GattCharacteristic1Proxy};
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;
use zbus::fdo::ObjectManagerProxy;
use zvariant::{Array, ObjectPath, OwnedObjectPath, OwnedValue};

use crate::sensor::SensorValues;

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
    device_path: OwnedObjectPath,
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
            device_path,
        }
    }

    fn read_values(&self, dbus: &zbus::Connection) -> Result<SensorValues, eyre::Error> {
        let temperature = Self::read_with(
            dbus,
            &self.temperature_path,
            byteorder::LittleEndian::read_i16,
        )?;

        let pressure =
            Self::read_with(dbus, &self.pressure_path, byteorder::LittleEndian::read_u32)?;

        let humidity =
            Self::read_with(dbus, &self.humidity_path, byteorder::LittleEndian::read_u16)?;

        Ok(SensorValues {
            temperature: Celsius::try_from(temperature)?,
            pressure: Pascal::from(pressure),
            humidity: RelativeHumidity::try_from(humidity)?,
        })
    }

    fn disconnect(&self, dbus: &zbus::Connection) -> Result<(), zbus::Error> {
        Device1Proxy::new_for(dbus, "org.bluez", self.device_path.as_str())?.disconnect()
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

pub(crate) fn bluetooth_thread(
    stop: flume::Receiver<()>,
) -> (
    thread::JoinHandle<Result<(), eyre::Error>>,
    oneshot::Receiver<()>,
    flume::Receiver<BTreeMap<BluetoothAddress, SensorState>>,
) {
    let (tx, rx) = flume::bounded(1);
    let poll_fn = move || -> Result<(), eyre::Error> {
        let dbus = zbus::Connection::new_system()?;
        let mut connected_devices = BTreeMap::new();
        let bluez_object_proxy = ObjectManagerProxy::new_for(&dbus, "org.bluez", "/")?;
        loop {
            let poll_started = Instant::now();
            let objs = bluez_object_proxy.get_managed_objects()?;
            let mut sleep_time = Duration::from_secs(31);
            for (object_path, interfaces) in objs {
                if let Some(obj) = interpret_object(&object_path, interfaces) {
                    match obj {
                        BluezObject::Interface {
                            discovering: false,
                            interface,
                        } => {
                            Adapter1Proxy::new_for(&dbus, "org.bluez", object_path.as_str())?
                                .start_discovery()?;
                            tracing::info!("Started discovery for interface {}", interface);
                            sleep_time = Duration::from_secs(10);
                        }
                        BluezObject::WeatherstationDevice {
                            connected: false,
                            address,
                            ..
                        } => {
                            match Device1Proxy::new_for(&dbus, "org.bluez", object_path.as_str())?
                                .connect()
                            {
                                Ok(()) => {}
                                Err(zbus::Error::MethodError(_, _, _)) => {
                                    tracing::warn!("Could not connect to {}", address);
                                }
                                Err(e) => {
                                    return Err(e.into());
                                }
                            };
                        }
                        BluezObject::WeatherstationDevice {
                            services_resolved: true,
                            address,
                            ..
                        } if !connected_devices.contains_key(&address) => {
                            tracing::info!("Connected new device {}", address);
                            let ws = Weatherstation::from_device_path(object_path);
                            connected_devices.insert(address, ws);
                        }
                        _ => {}
                    }
                }
            }

            let mut state = BTreeMap::new();
            for (addr, ws) in &connected_devices {
                let sensor_values = ws.read_values(&dbus)?;
                state.insert(*addr, SensorState::Connected(sensor_values));
            }

            let _ = tx.send(state);

            match stop.recv_timeout(
                sleep_time
                    .checked_sub(poll_started.elapsed())
                    .unwrap_or(Duration::from_secs(0)),
            ) {
                Ok(()) | Err(flume::RecvTimeoutError::Disconnected) => {
                    // TODO: parallelize this, takes about 2 seconds per device
                    tracing::info!("Disconnecting devices");
                    for (addr, ws) in connected_devices {
                        tracing::info!("Disconnecting {}", addr);
                        ws.disconnect(&dbus)?;
                    }
                    break Ok(());
                }
                _ => {}
            }
        }
    };

    let (error_tx, error_rx) = oneshot::channel();
    let thread_handle = thread::spawn(move || -> Result<(), eyre::Error> {
        match poll_fn() {
            Err(e) => {
                error_tx.send(()).unwrap();
                Err(e)
            }
            a => a,
        }
    });

    (thread_handle, error_rx, rx)
}

#[derive(Debug)]
enum BluezObject<'a> {
    Interface {
        discovering: bool,
        interface: &'a str,
    },

    WeatherstationDevice {
        address: BluetoothAddress,
        connected: bool,
        services_resolved: bool,
    },
}

fn interpret_object(
    object_path: &OwnedObjectPath,
    interfaces: HashMap<String, HashMap<String, OwnedValue>>,
) -> Option<BluezObject> {
    let path = object_path
        .as_str()
        .strip_prefix("/org/bluez/")?
        .split('/')
        .collect::<Vec<_>>();

    match path.as_slice() {
        [_interface, _device] => {
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

        [interface] => {
            let bluez_adapter = interfaces.get("org.bluez.Adapter1")?;
            let discovering = *bluez_adapter.get("Discovering")?.downcast_ref::<bool>()?;
            Some(BluezObject::Interface {
                discovering,
                interface,
            })
        }
        _ => None,
    }
}
