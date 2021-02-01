mod bluetooth;
mod config;
mod db;
mod dummy;
mod http;
mod opt;
mod sensor;
mod timestamp;

use crate::{bluetooth::BluetoothAddress, dummy::dummy_sensor, opt::Opt};
use clap::Clap;
use config::Config;
use db::AddrDbEntry;
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

fn main() -> Result<(), eyre::Error> {
    let args = Opt::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    match args.executor {
        opt::Rt::MultiThread => {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            let rt = if let Some(n) = args.workers {
                builder.worker_threads(n.get())
            } else {
                &mut builder
            }
            .enable_all()
            .build()?;

            rt.block_on(run())
        }
        opt::Rt::CurrentThread => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            rt.block_on(run())
        }
    }
}

type UpdateSource = dyn Stream<Item = BTreeMap<BluetoothAddress, SensorState>> + Unpin + Send;

async fn run() -> Result<(), eyre::Error> {
    let config = Config::from_env()?;
    let ctx = Context::create(&config)?;

    let (stopped_tx, stopped_rx) = flume::bounded(1);
    let (bluetooth_thread, bluetooth_failed, bluetooth_update) =
        bluetooth::bluetooth_thread(stopped_rx);

    let mut sources: Vec<Box<UpdateSource>> = Vec::new();

    sources.push(Box::new(bluetooth_update.into_stream()));
    if let Some(n) = config.demo {
        tracing::info!("Simulating {} dummy sensors", n);
        for i in 0..n.get() {
            let (dummy_task, dummy_stream) = dummy_sensor(BluetoothAddress::from(u64::from(i)));
            task::spawn(dummy_task);
            sources.push(Box::new(dummy_stream));
        }
    }

    let update_task = task::spawn(update_task(ctx.clone(), stream::select_all(sources)));

    if let Some(ref options) = config.mqtt_options {
        let (cxn, _) =
            tokio_mqtt::Connection::connect(options, "ble-weatherstation-central", 60).await?;
        task::spawn(mqtt_publish_task(ctx.clone(), cxn));
    }

    let mut term = unix::signal(SignalKind::terminate()).unwrap();
    let mut int = unix::signal(SignalKind::interrupt()).unwrap();
    let shutdown = async move {
        // FIXME:
        let signal = async move {
            tokio::select! {
                _ = term.recv() => (),
                _ = int.recv() => ()
            }
        };

        tokio::select! {
            // TODO: unify crash error cases
            Err(e) = update_task => {
                tracing::error!("Update task failed: {}", e);
            }
            _ = bluetooth_failed => {
            }
            _ = signal => {
                drop(stopped_tx);
            }
        }
    };

    let (addr, svr) = http::serve(ctx, SocketAddr::from((config.host, config.port)), shutdown);
    tracing::info!("Started server on {}", addr);

    svr.await;

    bluetooth_thread.join().expect("Bluetooth thread crashed")?;

    Ok(())
}

async fn mqtt_publish_task(
    ctx: Context,
    mut cxn: tokio_mqtt::Connection,
) -> Result<(), tokio_mqtt::Error> {
    let mut topic_buf = String::new();
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    let mut json_buf = Vec::new();
    loop {
        interval.tick().await;
        let sensors = ctx.sensors.read().await;
        for (addr, state) in &*sensors {
            if let SensorState::Connected(values) = state {
                topic_buf.clear();
                write!(topic_buf, "sensors/weatherstation/{}", addr).unwrap();
                serde_json::to_writer(std::io::Cursor::new(&mut json_buf), &values).unwrap();
                // TODO: figure out what happens when mqtt server dies
                if let Err(e) = cxn
                    .publish(
                        tokio_mqtt::TopicName::new(topic_buf.clone()).unwrap(),
                        json_buf.clone(),
                    )
                    .await
                {
                    tracing::error!("Failed publishing to mqtt server: {}", e);
                }
            }
        }
    }
}

async fn update_task(
    ctx: Context,
    mut updates: impl Stream<Item = BTreeMap<BluetoothAddress, SensorState>> + Unpin,
) -> Result<(), db::Error> {
    let mut interval = tokio::time::interval(Duration::from_secs(1 * 60));
    loop {
        // TODO: make both arms a function
        tokio::select! {
            _ = interval.tick() => {
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

#[derive(derive_more::Deref, Clone)]
pub(crate) struct Context(Arc<ContextInner>);

impl Context {
    pub fn create(config: &Config) -> Result<Self, eyre::Error> {
        let db = db::Db::open(&config.db_path)
            .with_context(|| format!("Opening database in {}", config.db_path.display()))?;

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
