use clap::Clap;

/// Central server for a number of ble-weatherstations
#[derive(Clap)]
pub(crate) struct Opt {
    /// number of worker threads, has no effect on the current_thread runtime
    #[clap(short, long)]
    pub workers: Option<std::num::NonZeroUsize>,
    /// kind of executor, either `current_thread` or `multi_thread`
    #[clap(short, long, default_value = "multi_thread")]
    pub executor: Rt,
}

pub(crate) enum Rt {
    MultiThread,
    CurrentThread,
}

impl std::str::FromStr for Rt {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "current_thread" => Ok(Self::CurrentThread),
            "multi_thread" => Ok(Self::MultiThread),
            _ => Err(String::from(
                "Executor must be either `current_thread` or `multi_thread`",
            )),
        }
    }
}
