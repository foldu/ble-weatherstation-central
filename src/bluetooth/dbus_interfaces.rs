use zbus::dbus_proxy;

#[dbus_proxy(interface = "org.bluez.Adapter1")]
pub trait Adapter1 {
    /// GetDiscoveryFilters method
    fn get_discovery_filters(&self) -> zbus::Result<Vec<String>>;

    /// RemoveDevice method
    fn remove_device(&self, device: &zvariant::ObjectPath) -> zbus::Result<()>;

    /// SetDiscoveryFilter method
    fn set_discovery_filter(
        &self,
        properties: std::collections::HashMap<&str, zvariant::Value>,
    ) -> zbus::Result<()>;

    /// StartDiscovery method
    fn start_discovery(&self) -> zbus::Result<()>;

    /// StopDiscovery method
    fn stop_discovery(&self) -> zbus::Result<()>;

    /// Address property
    #[dbus_proxy(property)]
    fn address(&self) -> zbus::fdo::Result<String>;

    /// AddressType property
    #[dbus_proxy(property)]
    fn address_type(&self) -> zbus::fdo::Result<String>;

    /// Alias property
    #[dbus_proxy(property)]
    fn alias(&self) -> zbus::fdo::Result<String>;
    #[DBusProxy(property)]
    fn set_alias(&self, value: &str) -> zbus::fdo::Result<()>;

    /// Class property
    #[dbus_proxy(property)]
    fn class(&self) -> zbus::fdo::Result<u32>;

    /// Discoverable property
    #[dbus_proxy(property)]
    fn discoverable(&self) -> zbus::fdo::Result<bool>;
    #[DBusProxy(property)]
    fn set_discoverable(&self, value: bool) -> zbus::fdo::Result<()>;

    /// DiscoverableTimeout property
    #[dbus_proxy(property)]
    fn discoverable_timeout(&self) -> zbus::fdo::Result<u32>;
    #[DBusProxy(property)]
    fn set_discoverable_timeout(&self, value: u32) -> zbus::fdo::Result<()>;

    /// Discovering property
    #[dbus_proxy(property)]
    fn discovering(&self) -> zbus::fdo::Result<bool>;

    /// Modalias property
    #[dbus_proxy(property)]
    fn modalias(&self) -> zbus::fdo::Result<String>;

    /// Name property
    #[dbus_proxy(property)]
    fn name(&self) -> zbus::fdo::Result<String>;

    /// Pairable property
    #[dbus_proxy(property)]
    fn pairable(&self) -> zbus::fdo::Result<bool>;
    #[DBusProxy(property)]
    fn set_pairable(&self, value: bool) -> zbus::fdo::Result<()>;

    /// PairableTimeout property
    #[dbus_proxy(property)]
    fn pairable_timeout(&self) -> zbus::fdo::Result<u32>;
    #[DBusProxy(property)]
    fn set_pairable_timeout(&self, value: u32) -> zbus::fdo::Result<()>;

    /// Powered property
    #[dbus_proxy(property)]
    fn powered(&self) -> zbus::fdo::Result<bool>;
    #[DBusProxy(property)]
    fn set_powered(&self, value: bool) -> zbus::fdo::Result<()>;

    /// UUIDs property
    #[dbus_proxy(property)]
    fn uuids(&self) -> zbus::fdo::Result<Vec<String>>;
}

#[dbus_proxy(interface = "org.bluez.Device1")]
pub trait Device1 {
    /// CancelPairing method
    fn cancel_pairing(&self) -> zbus::Result<()>;

    /// Connect method
    fn connect(&self) -> zbus::Result<()>;

    /// ConnectProfile method
    fn connect_profile(&self, uuid: &str) -> zbus::Result<()>;

    /// Disconnect method
    fn disconnect(&self) -> zbus::Result<()>;

    /// DisconnectProfile method
    fn disconnect_profile(&self, uuid: &str) -> zbus::Result<()>;

    /// Pair method
    fn pair(&self) -> zbus::Result<()>;

    // Adapter property
    //#[dbus_proxy(property)]
    //fn adapter(&self) -> zbus::fdo::Result<zvariant::OwnedObjectPath>;

    /// Address property
    #[dbus_proxy(property)]
    fn address(&self) -> zbus::fdo::Result<String>;

    /// AddressType property
    #[dbus_proxy(property)]
    fn address_type(&self) -> zbus::fdo::Result<String>;

    /// Alias property
    #[dbus_proxy(property)]
    fn alias(&self) -> zbus::fdo::Result<String>;
    #[DBusProxy(property)]
    fn set_alias(&self, value: &str) -> zbus::fdo::Result<()>;

    /// Appearance property
    #[dbus_proxy(property)]
    fn appearance(&self) -> zbus::fdo::Result<u16>;

    /// Blocked property
    #[dbus_proxy(property)]
    fn blocked(&self) -> zbus::fdo::Result<bool>;
    #[DBusProxy(property)]
    fn set_blocked(&self, value: bool) -> zbus::fdo::Result<()>;

    /// Class property
    #[dbus_proxy(property)]
    fn class(&self) -> zbus::fdo::Result<u32>;

    /// Connected property
    #[dbus_proxy(property)]
    fn connected(&self) -> zbus::fdo::Result<bool>;

    /// Icon property
    #[dbus_proxy(property)]
    fn icon(&self) -> zbus::fdo::Result<String>;

    /// LegacyPairing property
    #[dbus_proxy(property)]
    fn legacy_pairing(&self) -> zbus::fdo::Result<bool>;

    // ManufacturerData property
    //#[dbus_proxy(property)]
    //fn manufacturer_data(
    //    &self,
    //) -> zbus::fdo::Result<std::collections::HashMap<u16, zvariant::OwnedValue>>;

    /// Modalias property
    #[dbus_proxy(property)]
    fn modalias(&self) -> zbus::fdo::Result<String>;

    /// Name property
    #[dbus_proxy(property)]
    fn name(&self) -> zbus::fdo::Result<String>;

    /// Paired property
    #[dbus_proxy(property)]
    fn paired(&self) -> zbus::fdo::Result<bool>;

    /// RSSI property
    #[dbus_proxy(property)]
    fn rssi(&self) -> zbus::fdo::Result<i16>;

    // ServiceData property
    //#[dbus_proxy(property)]
    //fn service_data(
    //    &self,
    //) -> zbus::fdo::Result<std::collections::HashMap<String, zvariant::OwnedValue>>;

    /// ServicesResolved property
    #[dbus_proxy(property)]
    fn services_resolved(&self) -> zbus::fdo::Result<bool>;

    /// Trusted property
    #[dbus_proxy(property)]
    fn trusted(&self) -> zbus::fdo::Result<bool>;
    #[DBusProxy(property)]
    fn set_trusted(&self, value: bool) -> zbus::fdo::Result<()>;

    /// TxPower property
    #[dbus_proxy(property)]
    fn tx_power(&self) -> zbus::fdo::Result<i16>;

    /// UUIDs property
    #[dbus_proxy(property)]
    fn uuids(&self) -> zbus::fdo::Result<Vec<String>>;
}

#[dbus_proxy(interface = "org.bluez.Battery1")]
pub trait Battery1 {
    /// Percentage property
    #[dbus_proxy(property)]
    fn percentage(&self) -> zbus::fdo::Result<u8>;
}

#[dbus_proxy(interface = "org.bluez.GattService1")]
pub trait GattService1 {
    // Device property
    //#[dbus_proxy(property)]
    //fn device(&self) -> zbus::fdo::Result<zvariant::OwnedObjectPath>;

    // Includes property
    //#[dbus_proxy(property)]
    //fn includes(&self) -> zbus::fdo::Result<Vec<zvariant::OwnedObjectPath>>;

    /// Primary property
    #[dbus_proxy(property)]
    fn primary(&self) -> zbus::fdo::Result<bool>;

    /// UUID property
    #[dbus_proxy(property)]
    fn uuid(&self) -> zbus::fdo::Result<String>;
}

#[dbus_proxy(interface = "org.bluez.GattCharacteristic1")]
pub trait GattCharacteristic1 {
    /// AcquireNotify method
    fn acquire_notify(
        &self,
        options: std::collections::HashMap<&str, zvariant::Value>,
    ) -> zbus::Result<(std::os::unix::io::RawFd, u16)>;

    /// AcquireWrite method
    fn acquire_write(
        &self,
        options: std::collections::HashMap<&str, zvariant::Value>,
    ) -> zbus::Result<(std::os::unix::io::RawFd, u16)>;

    /// ReadValue method
    fn read_value(
        &self,
        options: std::collections::HashMap<&str, zvariant::Value>,
    ) -> zbus::Result<Vec<u8>>;

    /// StartNotify method
    fn start_notify(&self) -> zbus::Result<()>;

    /// StopNotify method
    fn stop_notify(&self) -> zbus::Result<()>;

    /// WriteValue method
    fn write_value(
        &self,
        value: &[u8],
        options: std::collections::HashMap<&str, zvariant::Value>,
    ) -> zbus::Result<()>;

    /// Flags property
    #[dbus_proxy(property)]
    fn flags(&self) -> zbus::fdo::Result<Vec<String>>;

    /// NotifyAcquired property
    #[dbus_proxy(property)]
    fn notify_acquired(&self) -> zbus::fdo::Result<bool>;

    /// Notifying property
    #[dbus_proxy(property)]
    fn notifying(&self) -> zbus::fdo::Result<bool>;

    // Service property
    //#[dbus_proxy(property)]
    //fn service(&self) -> zbus::fdo::Result<zvariant::OwnedObjectPath>;

    /// UUID property
    #[dbus_proxy(property)]
    fn uuid(&self) -> zbus::fdo::Result<String>;

    /// Value property
    #[dbus_proxy(property)]
    fn value(&self) -> zbus::fdo::Result<Vec<u8>>;

    /// WriteAcquired property
    #[dbus_proxy(property)]
    fn write_acquired(&self) -> zbus::fdo::Result<bool>;
}
