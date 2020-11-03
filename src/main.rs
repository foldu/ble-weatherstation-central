mod bluetooth;
mod db;
mod http;
mod sensor;

use crate::bluetooth::BluetoothAddress;
use std::{collections::BTreeMap, convert::TryFrom, sync::Arc};
use tokio::sync::RwLock;

async fn run() -> Result<(), eyre::Error> {
    let ctx = Context::create()?;
    let (poll, read) = bluetooth::create_bluetooth_tasks(ctx.clone())?;
    tokio::task::spawn(poll);
    tokio::task::spawn(read);
    // FIXME:
    {
        let mut sensors = ctx.sensors.write().await;
        sensors.insert(
            BluetoothAddress::parse_str("00:00:00:00:00:00").unwrap(),
            sensor::SensorState::Connected(sensor::SensorValues {
                temperature: sensor::Celsius::try_from(10_00).unwrap(),
                humidity: sensor::RelativeHumidity::try_from(100_00).unwrap(),
                pressure: sensor::Pascal::from(1000),
            }),
        );
    }

    let (_addr, svr) = http::serve(ctx);

    svr.await;

    Ok(())
}

fn main() -> Result<(), eyre::Error> {
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
