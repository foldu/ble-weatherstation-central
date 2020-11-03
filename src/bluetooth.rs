mod address;
mod dbus_interfaces;
pub use address::BluetoothAddress;

use dbus_interfaces::{Adapter1Proxy, Battery1Proxy, Device1Proxy};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{collections::HashMap, future::Future};
use uuid::Uuid;
use zbus::fdo::ObjectManagerProxy;
use zvariant::{Array, OwnedObjectPath, OwnedValue};

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

pub fn bluetooth_task() -> Result<impl Future<Output = Result<(), eyre::Error>>, eyre::Error> {
    let dbus = zbus::Connection::new_system()?;
    Ok(async move {
        let bluez_object_proxy = ObjectManagerProxy::new_for(&dbus, "org.bluez", "/")?;
        let objs = bluez_object_proxy.get_managed_objects()?;
        for (object_path, interfaces) in objs {
            if let Some(obj) = interpret_object(&object_path, interfaces) {
                match obj {
                    BluezObject::Interface { discovering: false } => {
                        Adapter1Proxy::new_for(&dbus, "org.bluez", object_path.as_str())?
                            .start_discovery()?;
                    }
                    BluezObject::WeatherstationDevice {
                        connected: false, ..
                    } => {
                        Device1Proxy::new_for(&dbus, "org.bluez", object_path.as_str())?
                            .connect()?;
                    }
                    BluezObject::WeatherstationDevice {
                        services_resolved: true,
                        address,
                        ..
                    } => {
                        // TODO:
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })
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
}

fn interpret_object(
    object_path: &OwnedObjectPath,
    interfaces: HashMap<String, HashMap<String, OwnedValue>>,
) -> Option<BluezObject> {
    static PATH_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"^/org/bluez/(?P<intf>[^/]+)(/dev_(?P<device>[^/]+))?$"#).unwrap()
    });

    let caps = PATH_RE.captures(object_path.as_str())?;
    let device = caps.name("intf").unwrap();
    match caps.name("device") {
        Some(dev) => {
            let dev = dev.as_str().replace('_', ":");
            let bluez_device = interfaces.get("org.bluez.Device1")?;
            let uuid_array = bluez_device.get("UUIDs")?.downcast_ref::<Array>()?;

            // kind of overkill
            bitflags::bitflags! {
                struct Contains: u8 {
                    const None = 0;
                    const EnvironmentalSensing = 1 << 0;
                    const Weatherstation = 1 << 1;
                    const All = Self::EnvironmentalSensing.bits | Self::Weatherstation.bits;
                }
            }
            let mut contains_flags = Contains::None;
            for uuid in uuid_array.get() {
                if let zvariant::Value::Str(s) = uuid {
                    if s.as_str() == BLE_GATT_SERVICE_ENVIRONMENTAL_SENSING.s {
                        contains_flags |= Contains::EnvironmentalSensing;
                    } else if s.as_str() == BLE_GATT_SERVICE_WEATHERSTATION.s {
                        contains_flags |= Contains::Weatherstation;
                    }
                }
            }
            if contains_flags != Contains::All {
                return None;
            }
            let connected = *bluez_device.get("Connected")?.downcast_ref::<bool>()?;
            let services_resolved = *bluez_device
                .get("ServicesResolved")?
                .downcast_ref::<bool>()?;

            Some(BluezObject::WeatherstationDevice {
                connected,
                address: BluetoothAddress(0),
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
