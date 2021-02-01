mod bluetooth;
mod config;
mod db;
mod dummy;
mod http;
mod opt;
mod sensor;
mod tasks;
mod timestamp;

use crate::{bluetooth::BluetoothAddress, dummy::dummy_sensor, opt::Opt};
use clap::Clap;
use config::Config;
use eyre::Context as _;
use futures_util::stream::{self, Stream};
use sensor::SensorState;
use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};
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

    let update_task = task::spawn(tasks::update(ctx.clone(), stream::select_all(sources)));

    if let Some(ref options) = config.mqtt_options {
        let (cxn, _) =
            tokio_mqtt::Connection::connect(options, "ble-weatherstation-central", 60).await?;
        task::spawn(tasks::mqtt_publish(ctx.clone(), cxn));
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
