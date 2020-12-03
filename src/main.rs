mod bluetooth;
mod db;
mod dummy;
mod http;
mod mqtt;
mod opt;
mod sensor;
mod timestamp;

use crate::{bluetooth::BluetoothAddress, dummy::dummy_sensor, opt::Opt};
use clap::Clap;
use db::AddrDbEntry;
use directories_next::ProjectDirs;
use eyre::Context as _;
use futures_util::{
    stream::{self, Stream},
    StreamExt,
};
use sensor::SensorState;
use std::{collections::BTreeMap, fmt::Write, net::SocketAddr, sync::Arc, time::Duration};
use timestamp::Timestamp;
use tokio::{signal::unix, sync::RwLock, task};
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
            let (dummy_task, dummy_stream) = dummy_sensor(BluetoothAddress::from(u64::from(i)));
            task::spawn(dummy_task);
            sources.push(Box::new(dummy_stream));
        }
    }

    let update_task = task::spawn(update_task(ctx.clone(), stream::select_all(sources)));

    if let Some(ref url) = args.mqtt_server_url {
        let options = mqtt::ConnectOptions::new(&url, mqtt::Ssl::None)?;
        let (cxn, _) =
            mqtt::Connection::connect(&options, "ble-weatherstation-central", 60).await?;
        task::spawn(mqtt_publish_task(ctx.clone(), cxn));
    }

    let term = unix::signal(SignalKind::terminate()).unwrap();
    let int = unix::signal(SignalKind::interrupt()).unwrap();
    let shutdown = async move {
        let mut signal = stream::select(term, int).map(|_| ());
        tokio::select! {
            // TODO: unify crash error cases
            Err(e) = update_task => {
                tracing::error!("Update task failed: {}", e);
            }
            _ = bluetooth_failed => {
            }
            _ = signal.next() => {
                drop(stopped_tx);
            }
        }
    };

    let (addr, svr) = http::serve(ctx, SocketAddr::from((args.host, args.port)), shutdown);
    tracing::info!("Started server on {}", addr);

    svr.await;

    bluetooth_thread.join().expect("Bluetooth thread crashed")?;

    Ok(())
}

async fn mqtt_publish_task(ctx: Context, mut cxn: mqtt::Connection) -> Result<(), mqtt::Error> {
    let mut topic_buf = String::new();
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    while let Some(_) = interval.next().await {
        let sensors = ctx.sensors.read().await;
        for (addr, state) in &*sensors {
            if let SensorState::Connected(values) = state {
                topic_buf.clear();
                write!(topic_buf, "sensors/weatherstation/{}", addr).unwrap();
                // TODO: figure out what happens when mqtt server dies
                if let Err(e) = cxn
                    .publish_json(::mqtt::TopicName::new(topic_buf.clone()).unwrap(), &values)
                    .await
                {
                    tracing::error!("Failed publishing to mqtt server: {}", e);
                }
            }
        }
    }

    Ok(())
}

async fn update_task(
    ctx: Context,
    mut updates: impl Stream<Item = BTreeMap<BluetoothAddress, SensorState>> + Unpin,
) -> Result<(), db::Error> {
    let mut interval = tokio::time::interval(Duration::from_secs(1 * 60));
    loop {
        // TODO: make both arms a function
        tokio::select! {
            _ = interval.next() => {
                let sensors = ctx.sensors.read().await;
                let now = Timestamp::now();
                let mut txn  = ctx.db.log_txn()?;
                for (addr, state) in &*sensors {
                    if let SensorState::Connected(values) = state {
                        txn.log(*addr, now, *values)?;
                    }
                }
                txn.commit()?;
            }
            update = updates.next() => {
                match update {
                    Some(update) => {
                        let mut new_sensors = Vec::new();
                        {
                            let txn = ctx.db.read_txn()?;
                            for &addr in update.keys() {
                                if ctx.db.get_addr(&txn, addr)?.is_none() {
                                    new_sensors.push(addr);
                                    tracing::info!("Memorized new sensor {}", addr);
                                }
                            }
                        }
                        if !new_sensors.is_empty() {
                            let mut txn = ctx.db.write_txn()?;
                            for addr in new_sensors {
                                ctx.db.put_addr(&mut txn, addr, &AddrDbEntry::default())?;
                            }
                            txn.commit()?;
                        }

                        ctx.sensors.write().await.extend(update);
                    }
                    None => break Ok(()),
                }
            }
        }
    }
}

fn main() -> Result<(), eyre::Error> {
    let args = Opt::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
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
