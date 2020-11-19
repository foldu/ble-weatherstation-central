use nix::time::{clock_gettime, ClockId};

#[repr(transparent)]
#[derive(
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Copy,
    Clone,
    zerocopy::FromBytes,
    zerocopy::AsBytes,
    serde::Serialize,
    Debug,
)]
pub(crate) struct Timestamp(u32);

#[cfg(target_os = "linux")]
const REALTIME_CLOCK: ClockId = ClockId::CLOCK_REALTIME_COARSE;

#[cfg(not(target_os = "linux"))]
const REALTIME_CLOCK: ClockId = ClockId::CLOCK_REALTIME;

impl Timestamp {
    pub const UNIX_EPOCH: Timestamp = Timestamp(0);
    pub const ONE_DAY: Timestamp = Timestamp(60 * 60 * 24);

    pub fn now() -> Self {
        // as u32 only causes problems after Sun 07 Feb 2106 07:28:15 AM CET
        // but I guess this won't be used after that
        Self(clock_gettime(REALTIME_CLOCK).unwrap().tv_sec() as u32)
    }

    pub fn bottoming_sub(self, rhs: Self) -> Self {
        Self(self.0.checked_sub(rhs.0).unwrap_or(0))
    }
}
