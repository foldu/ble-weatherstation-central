mod templates;

use crate::{bluetooth::BluetoothAddress, db::AddrDbEntry};
use std::{future::Future, net::SocketAddr};
use warp::Filter;

// TODO: add error handling after warp 0.3
#[macro_use]
macro_rules! static_file {
    ($content_type:expr, $path:literal) => {{
        const BIN: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/", $path));
        warp::http::Response::builder()
            .header("Content-Type", $content_type)
            .status(warp::http::StatusCode::OK)
            .body(BIN)
    }};
}

pub(crate) fn serve(
    ctx: super::Context,
    addr: SocketAddr,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> (std::net::SocketAddr, impl warp::Future) {
    let ctx = warp::any().map({
        let ctx = ctx.clone();
        move || ctx.clone()
    });

    let home = warp::get()
        .and(warp::path::end())
        .and(ctx.clone())
        .and_then(show_sensors);

    let change_label = warp::put()
        .and(warp::path!("api" / "change_label"))
        .and(ctx.clone())
        .and(warp::filters::body::json())
        .and_then(change_label);

    let forget = warp::delete()
        .and(warp::path!("api" / "forget"))
        .and(ctx.clone())
        .and(warp::filters::body::json())
        .and_then(forget);

    let get_state = warp::get()
        .and(warp::path!("api" / "state"))
        .and(ctx.clone())
        .and_then(get_state);

    let script = warp::get()
        .and(warp::path!("static" / "script.js"))
        .map(|| static_file!("application/javascript", "script.js"));

    let css = warp::get()
        .and(warp::path!("static" / "style.css"))
        .map(|| static_file!("text/css", "style.css"));

    let pure = warp::get()
        .and(warp::path!("static" / "pure-min.css"))
        .map(|| static_file!("text/css", "pure-min.css"));

    let cors = warp::cors()
        .allow_methods(vec!["GET", "PUT", "DELETE", "HEAD"])
        .build();

    let routes = home
        .or(change_label)
        .or(get_state)
        .or(forget)
        .or(script)
        .or(css)
        .or(pure)
        .with(cors);

    warp::serve(routes).bind_with_graceful_shutdown(addr, shutdown)
}

async fn show_sensors(ctx: super::Context) -> Result<impl warp::Reply, std::convert::Infallible> {
    let sensors = ctx.sensors.read().await;
    let mut display = Vec::with_capacity(sensors.len());
    let txn = ctx.db.read_txn().unwrap();
    for (addr, state) in sensors.iter() {
        let label = match ctx.db.get_addr(&txn, *addr).unwrap() {
            Some(entry) => entry.label,
            None => None,
        };
        display.push((
            *addr,
            templates::SensorEntry {
                state: *state,
                label,
            },
        ))
    }

    Ok(askama_warp::reply(&templates::Home::new(&display), "html"))
}

#[derive(serde::Deserialize)]
struct ChangeLabel {
    addr: BluetoothAddress,
    new_label: Option<String>,
}

async fn change_label(
    ctx: super::Context,
    req: ChangeLabel,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let mut txn = ctx.db.write_txn().unwrap();
    let entry = AddrDbEntry {
        label: req.new_label,
    };
    ctx.db.put_addr(&mut txn, req.addr, &entry).unwrap();
    //if ctx.db.get_addr(&txn, req.addr).unwrap().is_some() {
    //    let entry = AddrDbEntry {
    //        label: req.new_label,
    //    };
    //    ctx.db.put_addr(&mut txn, req.addr, &entry).unwrap();
    //}
    txn.commit().unwrap();

    Ok(warp::reply::with_status("", warp::http::StatusCode::OK))
}

#[derive(serde::Deserialize)]
struct Forget {
    addr: BluetoothAddress,
}

async fn forget(
    ctx: super::Context,
    req: Forget,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let mut txn = ctx.db.write_txn().unwrap();
    ctx.db.delete_addr(&mut txn, req.addr).unwrap();
    txn.commit().unwrap();
    Ok(warp::reply::with_status("", warp::http::StatusCode::OK))
}

async fn get_state(ctx: super::Context) -> Result<impl warp::Reply, std::convert::Infallible> {
    let sensors = ctx.sensors.read().await;
    #[derive(serde::Serialize)]
    struct ReplyEntry {
        state: crate::sensor::SensorState,
        label: Option<String>,
    }
    let txn = ctx.db.read_txn().unwrap();

    let reply = sensors
        .iter()
        .map(|(addr, state)| {
            let db_entry = ctx.db.get_addr(&txn, *addr).unwrap();
            (
                addr,
                ReplyEntry {
                    state: *state,
                    label: db_entry.and_then(|entry| entry.label),
                },
            )
        })
        .collect::<Vec<_>>();

    Ok(warp::reply::json(&reply))
}
