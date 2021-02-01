use serde::{
    de::{self, Visitor},
    Deserialize, Serialize, Serializer,
};
use std::fmt;

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BluetoothAddress(u64);

const EXAMPLE_BLUETOOTH_ADDR: &str = "00:11:22:33:FF:EE";

impl From<u64> for BluetoothAddress {
    fn from(n: u64) -> Self {
        Self(n)
    }
}

impl std::str::FromStr for BluetoothAddress {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != EXAMPLE_BLUETOOTH_ADDR.len() {
            return Err(eyre::format_err!("Bluetooth addr `{}` too short", s));
        }

        let mut ret = 0;
        for segment in s.split(':') {
            if segment.len() != 2 {
                return Err(eyre::format_err!("Invalid blueooth addr `{}`", s));
            }
            ret <<= 8;
            ret |= u64::from(u8::from_str_radix(segment, 16)?);
        }

        Ok(Self(ret))
    }
}

impl fmt::Display for BluetoothAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0;
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            (n >> (5 << 3)) as u8,
            (n >> (4 << 3)) as u8,
            (n >> (3 << 3)) as u8,
            (n >> (2 << 3)) as u8,
            (n >> (1 << 3)) as u8,
            (n >> (0 << 3)) as u8,
        )
    }
}

impl BluetoothAddress {
    pub fn parse_str(s: &str) -> Result<Self, eyre::Error> {
        s.parse()
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for BluetoothAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BtAddrVisitor;
        impl<'de> Visitor<'de> for BtAddrVisitor {
            type Value = BluetoothAddress;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("A bluetooth address")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                BluetoothAddress::parse_str(value).map_err(|e| de::Error::custom(e.to_string()))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(BluetoothAddress(value))
            }
        }

        deserializer.deserialize_any(BtAddrVisitor)
    }
}

impl Serialize for BluetoothAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn bluetooth_address_parse_roundtrip() {
        let addr = EXAMPLE_BLUETOOTH_ADDR.parse::<BluetoothAddress>().unwrap();
        assert_eq!(addr.to_string(), EXAMPLE_BLUETOOTH_ADDR.to_owned())
    }

    #[test]
    fn bluetooth_address_serialize_roundtrip() {
        let addr = EXAMPLE_BLUETOOTH_ADDR.parse::<BluetoothAddress>().unwrap();
        #[derive(Serialize, Deserialize)]
        struct Test {
            addr: BluetoothAddress,
        }
        let json = serde_json::to_string(&Test { addr }).unwrap();
        assert_eq!(addr, serde_json::from_str::<Test>(&json).unwrap().addr)
    }
}
