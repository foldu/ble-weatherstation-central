use crate::bluetooth::BluetoothAddress;
use heed::{
    types::{OwnedType, SerdeBincode},
    RoTxn, RwTxn,
};
use std::path::Path;

type DbBtAddr = zerocopy::U64<heed::byteorder::LE>;

pub(crate) struct Db {
    env: heed::Env,
    addr_db: heed::Database<OwnedType<DbBtAddr>, SerdeBincode<AddrDbEntry>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct AddrDbEntry {
    pub(crate) label: Option<String>,
}

impl Db {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, heed::Error> {
        let env = heed::EnvOpenOptions::new().max_dbs(2).open(db_path)?;
        let addr_db = env.create_database(Some("addr"))?;
        Ok(Self { env, addr_db })
    }

    pub fn read_txn(&self) -> Result<heed::RoTxn, heed::Error> {
        self.env.read_txn()
    }

    pub fn write_txn(&self) -> Result<heed::RwTxn, heed::Error> {
        self.env.write_txn()
    }

    pub fn get_addr<'txn, T>(
        &self,
        txn: &'txn RoTxn<'_, T>,
        addr: BluetoothAddress,
    ) -> Result<Option<AddrDbEntry>, heed::Error> {
        self.addr_db.get(txn, &DbBtAddr::new(addr.as_u64()))
    }

    pub fn put_addr<T>(
        &self,
        txn: &mut RwTxn<'_, '_, T>,
        addr: BluetoothAddress,
        data: &AddrDbEntry,
    ) -> Result<(), heed::Error> {
        self.addr_db.put(txn, &DbBtAddr::new(addr.as_u64()), data)
    }

    pub fn known_addrs<'txn, T>(
        &self,
        txn: &'txn RoTxn<'_, T>,
    ) -> Result<impl Iterator<Item = Result<BluetoothAddress, heed::Error>> + 'txn, heed::Error>
    {
        self.addr_db
            .iter(txn)
            .map(|it| it.map(|res| res.map(|(addr, _)| BluetoothAddress::from(addr.get()))))
    }
}
