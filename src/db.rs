use crate::{
    bluetooth::BluetoothAddress,
    sensor::{RawSensorValues, SensorValues},
    timestamp::Timestamp,
};
use heed::{
    byteorder::BigEndian,
    types::{integer::U32, OwnedType, SerdeBincode},
    RoTxn,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    convert::TryFrom,
    fs,
    ops::Range,
    path::{Path, PathBuf},
    sync::{RwLock, RwLockReadGuard},
};

type BEU32 = U32<BigEndian>;

type LogDb =
    BTreeMap<BluetoothAddress, heed::Database<OwnedType<BEU32>, OwnedType<RawSensorValues>>>;

pub(crate) struct Db {
    env: heed::Env,
    addr_db: heed::Database<OwnedType<BluetoothAddress>, SerdeBincode<AddrDbEntry>>,
    sensor_log: RwLock<LogDb>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct AddrDbEntry {
    pub(crate) label: Option<String>,
}

// needed because of https://github.com/Kerollmops/heed/issues/20
pub struct RwTxn<'a, 'b>(Option<heed::RwTxn<'a, 'b>>);

thread_local! {
    static RW_TXN_RUNNING: RefCell<bool> = RefCell::new(false);
}

impl Drop for RwTxn<'_, '_> {
    fn drop(&mut self) {
        RW_TXN_RUNNING.with(|running| {
            *running.borrow_mut() = false;
        })
    }
}

impl<'a, 'b> RwTxn<'a, 'b> {
    pub(crate) fn commit(mut self) -> Result<(), Error> {
        self.0.take().unwrap().commit().map_err(Error::from)
    }

    fn inner_mut(&mut self) -> &mut heed::RwTxn<'a, 'b, ()> {
        self.0.as_mut().unwrap()
    }
}

pub(crate) struct LogTransaction<'a> {
    sensor_values: RwLockReadGuard<'a, LogDb>,
    txn: RwTxn<'a, 'a>,
}

impl<'a> LogTransaction<'a> {
    pub(crate) fn log(
        &mut self,
        addr: BluetoothAddress,
        timestamp: Timestamp,
        values: SensorValues,
    ) -> Result<(), heed::Error> {
        if let Some(db) = self.sensor_values.get(&addr) {
            db.put(
                self.txn.inner_mut(),
                &BEU32::new(timestamp.as_u32()),
                &values.into(),
            )?;
        }
        Ok(())
    }

    pub(crate) fn commit(self) -> Result<(), Error> {
        self.txn.commit()
    }
}

impl Db {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, Error> {
        let db_path = db_path.as_ref();
        fs::create_dir_all(&db_path).map_err(|source| Error::Create {
            path: db_path.to_owned(),
            source,
        })?;

        let env = heed::EnvOpenOptions::new().max_dbs(200).open(db_path)?;
        let addr_db = env.create_database(Some("addr"))?;
        let ret = Self {
            env,
            addr_db,
            sensor_log: RwLock::new(BTreeMap::new()),
        };

        {
            let txn = ret.read_txn()?;
            let mut sensor_log = ret.sensor_log.write().unwrap();
            for addr in ret.known_addrs(&txn)? {
                let addr = addr?;
                sensor_log.insert(addr, ret.env.create_database(Some(&addr.to_string()))?);
            }
        }

        Ok(ret)
    }

    pub fn read_txn(&self) -> Result<heed::RoTxn, Error> {
        self.env.read_txn().map_err(heed_err)
    }

    pub fn write_txn(&self) -> Result<RwTxn, Error> {
        RW_TXN_RUNNING.with(|running| {
            let mut running = running.borrow_mut();
            if *running {
                Err(Error::MultipleWriteTransaction)
            } else {
                *running = true;
                self.env
                    .write_txn()
                    .map(|txn| RwTxn(Some(txn)))
                    .map_err(Error::from)
            }
        })
    }

    pub fn log_txn(&self) -> Result<LogTransaction, Error> {
        Ok(LogTransaction {
            sensor_values: self.sensor_log.read().unwrap(),
            txn: self.write_txn()?,
        })
    }

    pub fn get_addr<'txn, T>(
        &self,
        txn: &'txn RoTxn<'_, T>,
        addr: BluetoothAddress,
    ) -> Result<Option<AddrDbEntry>, Error> {
        self.addr_db.get(txn, &addr).map_err(heed_err)
    }

    pub fn put_addr(
        &self,
        txn: &mut RwTxn<'_, '_>,
        addr: BluetoothAddress,
        data: &AddrDbEntry,
    ) -> Result<(), Error> {
        self.addr_db
            .put(txn.inner_mut(), &addr, data)
            .map_err(heed_err)
    }

    pub fn known_addrs<'txn, T>(
        &self,
        txn: &'txn RoTxn<'_, T>,
    ) -> Result<impl Iterator<Item = Result<BluetoothAddress, Error>> + 'txn, Error> {
        self.addr_db
            .iter(txn)
            .map(|it| it.map(|res| res.map(|(addr, _)| addr).map_err(heed_err)))
            .map_err(heed_err)
    }

    pub fn delete_addr(
        &self,
        txn: &mut RwTxn<'_, '_>,
        addr: BluetoothAddress,
    ) -> Result<bool, Error> {
        self.addr_db
            .delete(txn.inner_mut(), &addr)
            .map_err(heed_err)
    }

    pub fn get_log<T>(
        &self,
        txn: &RoTxn<'_, T>,
        addr: BluetoothAddress,
        range: Range<Timestamp>,
    ) -> Result<Option<Vec<(Timestamp, SensorValues)>>, Error> {
        let sensor_log = self.sensor_log.read().unwrap();
        let db = match sensor_log.get(&addr) {
            Some(db) => db,
            _ => return Ok(None),
        };

        let range = BEU32::new(range.start.as_u32())..BEU32::new(range.end.as_u32());

        let mut ret = Vec::new();
        for val in db.range(txn, &range)? {
            let (time, values) = val?;
            if let Ok(values) = SensorValues::try_from(values) {
                ret.push((Timestamp::from(time.get()), values));
            }
        }

        Ok(Some(ret))
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Can't create database dir in {}", path.display())]
    Create {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Multiple write transactions opened")]
    MultipleWriteTransaction,

    #[error("Error in database backend")]
    Heed(#[source] Box<dyn std::error::Error + Send + Sync>),
}

fn heed_err(e: heed::Error) -> Error {
    Error::Heed(format!("{}", e).into())
}

impl From<heed::Error> for Error {
    fn from(e: heed::Error) -> Self {
        heed_err(e)
    }
}

impl warp::reject::Reject for Error {}
