use serde::Serialize;
use std::{
    convert::TryFrom,
    fmt::{self, Display},
};

#[derive(Copy, Clone, Debug, Serialize)]
pub(crate) struct Celsius(i16);

impl TryFrom<i16> for Celsius {
    type Error = eyre::Error;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        // can't represent the planck temperature with i16 so absolute zero is enough
        if value < -273_15 {
            Err(eyre::format_err!(
                "Received temperature lower than absolute zero: {}",
                value
            ))
        } else {
            Ok(Self(value))
        }
    }
}

impl Display for Celsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:0>2}°C", self.0 / 100, self.0 % 100)
    }
}

#[derive(Copy, Clone, Debug, Serialize)]
pub(crate) struct RelativeHumidity(u16);

impl Display for RelativeHumidity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:0>2}%", self.0 / 100, self.0 % 100)
    }
}

impl TryFrom<u16> for RelativeHumidity {
    type Error = eyre::Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 100_00 {
            Err(eyre::format_err!(
                "Invalid relative humidity, can't be higher than 100%, received {}",
                value
            ))
        } else {
            Ok(Self(value))
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize)]
pub(crate) struct Pascal(u32);

impl From<u32> for Pascal {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl Display for Pascal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}Pa", self.0)
    }
}

#[derive(Copy, Clone, Debug, Serialize)]
pub(crate) struct SensorValues {
    pub(crate) temperature: Celsius,
    pub(crate) pressure: Pascal,
    pub(crate) humidity: RelativeHumidity,
}

impl Display for SensorValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Humidity: {}, Temperature: {}, pressure: {}",
            self.humidity, self.temperature, self.pressure
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "state")]
pub(crate) enum SensorState {
    Connected(SensorValues),
    Unconnected,
}

#[cfg(test)]
mod test {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn relative_humidity_display() {
        assert_eq!(
            RelativeHumidity::try_from(80_01).unwrap().to_string(),
            "80.01%".to_string()
        );

        assert_eq!(
            RelativeHumidity::try_from(100_00).unwrap().to_string(),
            "100.00%".to_string()
        );
    }

    #[test]
    fn relative_humidity_convert() {
        assert!(RelativeHumidity::try_from(100_01).is_err());
        assert!(RelativeHumidity::try_from(140_00).is_err());
        assert!(RelativeHumidity::try_from(10_01).is_ok());
    }

    #[test]
    fn celsius_display() {
        assert_eq!(Celsius::try_from(100_00).unwrap().to_string(), "100.00°C")
    }

    #[test]
    fn celsius_convert() {
        assert!(Celsius::try_from(-320_00).is_err());
        assert!(Celsius::try_from(100_00).is_ok())
    }

    #[test]
    fn pascal_display() {
        assert_eq!(Pascal::from(1000).to_string(), "1000Pa".to_string())
    }
}
