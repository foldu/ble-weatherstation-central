use crate::{bluetooth::BluetoothAddress, db, sensor::SensorState, timestamp::Timestamp};
use std::{collections::BTreeMap, fmt::Write, time::Duration};
use tokio_stream::{Stream, StreamExt};

pub(crate) async fn mqtt_publish(
    ctx: super::Context,
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

pub(crate) async fn update(
    ctx: super::Context,
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
                                ctx.db.put_addr(&mut txn, addr, &db::AddrDbEntry::default())?;
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
