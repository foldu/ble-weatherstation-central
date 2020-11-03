use std::fmt;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct BluetoothAddress(u64);

const EXAMPLE_BLUETOOTH_ADDR: &str = "00:11:22:33:FF:EE";

impl std::str::FromStr for BluetoothAddress {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != EXAMPLE_BLUETOOTH_ADDR.len() {
            return Err(eyre::format_err!("Bluetooth addr `{}` too short", s));
        }

        let mut ret = 0;
        for segment in s.split(':') {
            println!("{}", segment);
            if segment.len() != 2 {
                return Err(eyre::format_err!("Invalid blueooth addr `{}`", s));
            }
            ret <<= 8;
            ret |= u64::from(u8::from_str_radix(s, 16)?);
        }

        Ok(Self(ret))
    }
}

impl fmt::Display for BluetoothAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in (0..=6).rev() {
            // FIXME: WRONG
            write!(f, "{}", (self.0 >> i) & 0xff)?;
            if i != 6 {
                f.write_str(":")?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn bluetooth_address_parsing() {
        EXAMPLE_BLUETOOTH_ADDR.parse::<BluetoothAddress>().unwrap();
    }
}
