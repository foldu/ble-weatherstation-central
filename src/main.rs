mod bluetooth;
mod http;
mod sensor;
mod templates;

use eyre::Error;
use std::{collections::BTreeMap, convert::TryFrom, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

const BLE_GATT_SERVICE_ENVIRONMENTAL_SENSING_UUID: &str = "0000180f-0000-1000-8000-00805f9b34fb";

async fn run() -> Result<(), Error> {
    let ctx = Context::create()?;
    let bluetooth_task = bluetooth::bluetooth_task()?;
    bluetooth_task.await?;
    return Ok(());
    // FIXME:
    {
        let mut sensors = ctx.sensors.write().await;
        sensors.insert(
            Uuid::parse_str(BLE_GATT_SERVICE_ENVIRONMENTAL_SENSING_UUID).unwrap(),
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

fn main() -> Result<(), Error> {
    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .core_threads(2)
        .enable_all()
        .build()?;

    rt.block_on(run())
}

#[derive(derive_more::Deref, Clone)]
struct Context(Arc<ContextInner>);

impl Context {
    pub fn create() -> Result<Self, eyre::Error> {
        // FIXME:
        let db_path = "zerocopy.mdb";
        std::fs::create_dir_all(db_path)?;
        let env = heed::EnvOpenOptions::new().max_dbs(2).open(db_path)?;
        let uuid_db = env.create_database(Some("uuid"))?;
        Ok(Self(Arc::new(ContextInner {
            env,
            uuid_db,
            sensors: Default::default(),
        })))
    }
}

pub(crate) type DbUuid = heed::zerocopy::U128<heed::byteorder::BE>;

pub(crate) struct ContextInner {
    pub(crate) sensors: RwLock<BTreeMap<Uuid, sensor::SensorState>>,
    pub(crate) env: heed::Env,
    pub(crate) uuid_db:
        heed::Database<heed::types::OwnedType<DbUuid>, heed::types::SerdeBincode<UuidDbEntry>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UuidDbEntry {
    label: Option<String>,
}
