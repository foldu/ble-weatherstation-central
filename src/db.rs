use crate::bluetooth::BluetoothAddress;
use heed::{
    types::{OwnedType, SerdeBincode},
    RoTxn, RwTxn,
};
use std::path::{Path, PathBuf};

type DbBtAddr = zerocopy::U64<heed::byteorder::LE>;

pub(crate) struct Db {
    env: heed::Env,
    addr_db: heed::Database<OwnedType<DbBtAddr>, SerdeBincode<AddrDbEntry>>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct AddrDbEntry {
    pub(crate) label: Option<String>,
}

impl Db {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, Error> {
        let db_path = db_path.as_ref();
        std::fs::create_dir_all(&db_path).map_err(|source| Error::Create {
            path: db_path.to_owned(),
            source,
        })?;
        let env = heed::EnvOpenOptions::new().max_dbs(2).open(db_path)?;
        let addr_db = env.create_database(Some("addr"))?;
        Ok(Self { env, addr_db })
    }

    pub fn read_txn(&self) -> Result<heed::RoTxn, Error> {
        self.env.read_txn().map_err(Error::Heed)
    }

    pub fn write_txn(&self) -> Result<heed::RwTxn, Error> {
        self.env.write_txn().map_err(Error::Heed)
    }

    pub fn get_addr<'txn, T>(
        &self,
        txn: &'txn RoTxn<'_, T>,
        addr: BluetoothAddress,
    ) -> Result<Option<AddrDbEntry>, Error> {
        self.addr_db
            .get(txn, &DbBtAddr::new(addr.as_u64()))
            .map_err(Error::Heed)
    }

    pub fn put_addr<T>(
        &self,
        txn: &mut RwTxn<'_, '_, T>,
        addr: BluetoothAddress,
        data: &AddrDbEntry,
    ) -> Result<(), Error> {
        self.addr_db
            .put(txn, &DbBtAddr::new(addr.as_u64()), data)
            .map_err(Error::Heed)
    }

    pub fn known_addrs<'txn, T>(
        &self,
        txn: &'txn RoTxn<'_, T>,
    ) -> Result<impl Iterator<Item = Result<BluetoothAddress, Error>> + 'txn, Error> {
        self.addr_db
            .iter(txn)
            .map(|it| {
                it.map(|res| {
                    res.map(|(addr, _)| BluetoothAddress::from(addr.get()))
                        .map_err(Error::Heed)
                })
            })
            .map_err(Error::Heed)
    }

    pub fn delete_addr<T>(
        &self,
        txn: &mut RwTxn<'_, '_, T>,
        addr: BluetoothAddress,
    ) -> Result<bool, Error> {
        self.addr_db
            .delete(txn, &DbBtAddr::new(addr.as_u64()))
            .map_err(Error::Heed)
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Can't create database dir in {}", path.display())]
    Create {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Error in database backend")]
    Heed(#[from] heed::Error),
}

impl warp::reject::Reject for Error {}

impl From<Error> for warp::Rejection {
    fn from(e: Error) -> Self {
        warp::reject::custom(e)
    }
}
