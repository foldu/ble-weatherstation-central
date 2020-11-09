mod bluetooth;
mod db;
mod http;
mod sensor;

use crate::bluetooth::BluetoothAddress;
use futures_util::{stream, StreamExt};
use std::{collections::BTreeMap, sync::Arc};
use tokio::{signal::unix, sync::RwLock};
use unix::SignalKind;

async fn run() -> Result<(), eyre::Error> {
    let ctx = Context::create()?;
    let (stopped_tx, stopped_rx) = flume::bounded(1);

    let (bluetooth_thread, bluetooth_failed, state_update) =
        bluetooth::bluetooth_thread(stopped_rx);

    tokio::task::spawn({
        let ctx = ctx.clone();
        async move {
            while let Ok(update) = state_update.recv_async().await {
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

fn main() -> Result<(), eyre::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .core_threads(2)
        .enable_all()
        .build()?;

    rt.block_on(run())
}

#[derive(derive_more::Deref, Clone)]
pub(crate) struct Context(Arc<ContextInner>);

impl Context {
    pub fn create() -> Result<Self, eyre::Error> {
        // FIXME:
        let db_path = "zerocopy.mdb";
        std::fs::create_dir_all(db_path)?;
        let db = db::Db::open(db_path)?;

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
