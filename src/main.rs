mod bluetooth;
mod db;
mod dummy;
mod http;
mod sensor;

use crate::bluetooth::BluetoothAddress;
use clap::Clap;
use directories_next::ProjectDirs;
use eyre::Context as _;
use futures_util::{
    stream::{self, Stream},
    StreamExt,
};
use sensor::SensorState;
use std::{collections::BTreeMap, num::NonZeroU8, path::PathBuf, sync::Arc};
use tokio::{signal::unix, sync::RwLock};
use unix::SignalKind;

type UpdateSource = dyn Stream<Item = BTreeMap<BluetoothAddress, SensorState>> + Unpin + Send;

async fn run(args: Opt) -> Result<(), eyre::Error> {
    let ctx = Context::create(&args)?;

    let (stopped_tx, stopped_rx) = flume::bounded(1);
    let (bluetooth_thread, bluetooth_failed, bluetooth_update) =
        bluetooth::bluetooth_thread(stopped_rx);

    let mut sources: Vec<Box<UpdateSource>> = Vec::new();

    sources.push(Box::new(bluetooth_update.into_stream()));
    if let Some(n) = args.demo {
        tracing::info!("Simulating {} dummy sensors", n);
        for i in 0..n.get() {
            let (dummy_task, dummy_stream) =
                crate::dummy::dummy_sensor(BluetoothAddress::from(u64::from(i)));
            tokio::task::spawn(dummy_task);
            sources.push(Box::new(dummy_stream));
        }
    }

    let mut updates = stream::select_all(sources);

    tokio::task::spawn({
        let ctx = ctx.clone();
        async move {
            while let Some(update) = updates.next().await {
                ctx.sensors.write().await.extend(update);
            }
        }
    });

    let term = unix::signal(SignalKind::terminate()).unwrap();
    let int = unix::signal(SignalKind::interrupt()).unwrap();
    let shutdown = async move {
        let mut signal = stream::select(term, int).map(|_| ());
        tokio::select! {
            _ = bluetooth_failed => {
            }
            _ = signal.next() => {
                drop(stopped_tx);
            }
        }
    };

    let (addr, svr) = http::serve(ctx, shutdown);
    tracing::info!("Started server on {}", addr);

    svr.await;

    bluetooth_thread.join().expect("Bluetooth thread crashed")?;

    Ok(())
}

/// Central server for a number of ble-weatherstations
#[derive(Clap)]
struct Opt {
    /// Run with n dummy sensors
    #[clap(short, long)]
    demo: Option<NonZeroU8>,

    /// Path to database
    #[clap(long)]
    db_path: Option<PathBuf>,
}

fn main() -> Result<(), eyre::Error> {
    let args = Opt::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .core_threads(2)
        .enable_all()
        .build()?;

    rt.block_on(run(args))
}

#[derive(derive_more::Deref, Clone)]
pub(crate) struct Context(Arc<ContextInner>);

impl Context {
    pub fn create(args: &Opt) -> Result<Self, eyre::Error> {
        let db_path = match args.db_path {
            Some(ref path) => path.clone(),
            None => {
                let dirs = ProjectDirs::from("org", "foldu", env!("CARGO_PKG_NAME"))
                    .ok_or_else(|| eyre::format_err!("Could not get project directories"))?;
                dirs.data_dir()
                    .join(concat!(env!("CARGO_PKG_NAME"), ".mdb"))
            }
        };

        std::fs::create_dir_all(&db_path).with_context(|| {
            format!(
                "Failed creating database directory in {}",
                db_path.display()
            )
        })?;

        let db = db::Db::open(&db_path)
            .with_context(|| format!("Opening database in {}", db_path.display()))?;

        let mut sensors = BTreeMap::new();
        {
            let txn = db.read_txn()?;

            for addr in db.known_addrs(&txn)? {
                let addr = addr?;
                sensors.insert(addr, sensor::SensorState::Unconnected);
            }
        }

        Ok(Self(Arc::new(ContextInner {
            db,
            sensors: RwLock::new(sensors),
        })))
    }
}

pub(crate) struct ContextInner {
    pub(crate) sensors: RwLock<BTreeMap<BluetoothAddress, sensor::SensorState>>,
    pub(crate) db: db::Db,
}
