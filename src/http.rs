mod templates;

use crate::{
    bluetooth::BluetoothAddress, db::AddrDbEntry, sensor::SensorValues, timestamp::Timestamp,
};
use std::{future::Future, net::SocketAddr};
use warp::{http::StatusCode, reject, Filter};

// TODO: add better error handling after warp 0.3

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

    let log = warp::get()
        .and(warp::path!("api" / "log"))
        .and(ctx.clone())
        .and_then(get_log);

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
        .or(log)
        .or(css)
        .or(pure)
        .with(cors)
        // TODO: split into html rejection replies and json api rejection replies
        .recover(handle_rejection);

    warp::serve(routes).bind_with_graceful_shutdown(addr, shutdown)
}

async fn handle_rejection(
    rejection: warp::Rejection,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let response = warp::http::Response::builder();
    let render_error = |code: StatusCode| {
        response
            .status(code)
            .body(askama::Template::render(&templates::Error::new(code)).unwrap())
            .unwrap()
    };

    if let Some(db_error) = rejection.find::<crate::db::Error>() {
        let e: &dyn std::error::Error = db_error;
        tracing::error!(e);
        Ok(render_error(StatusCode::INTERNAL_SERVER_ERROR))
    } else if rejection.is_not_found() {
        Ok(render_error(StatusCode::NOT_FOUND))
    } else if let Some(_) = rejection.find::<reject::MethodNotAllowed>() {
        Ok(render_error(StatusCode::METHOD_NOT_ALLOWED))
    } else {
        tracing::error!("Unhandled rejection {:?}", rejection);
        // FIXME:
        Ok(render_error(StatusCode::IM_A_TEAPOT))
    }
}

async fn show_sensors(ctx: super::Context) -> Result<impl warp::Reply, warp::Rejection> {
    let sensors = ctx.sensors.read().await;
    let mut display = Vec::with_capacity(sensors.len());
    let txn = ctx.db.read_txn()?;
    for (addr, state) in sensors.iter() {
        let label = match ctx.db.get_addr(&txn, *addr)? {
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
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut txn = ctx.db.write_txn()?;
    let entry = AddrDbEntry {
        label: req.new_label,
    };
    ctx.db.put_addr(&mut txn, req.addr, &entry)?;
    txn.commit()?;

    Ok(warp::reply::with_status("", StatusCode::OK))
}

#[derive(serde::Deserialize)]
struct Forget {
    addr: BluetoothAddress,
}

async fn forget(ctx: super::Context, req: Forget) -> Result<impl warp::Reply, warp::Rejection> {
    ctx.sensors.write().await.remove(&req.addr);
    let mut txn = ctx.db.write_txn()?;
    ctx.db.delete_addr(&mut txn, req.addr)?;
    txn.commit()?;
    Ok(warp::reply::with_status("", StatusCode::OK))
}

async fn get_state(ctx: super::Context) -> Result<impl warp::Reply, warp::Rejection> {
    let sensors = ctx.sensors.read().await;
    #[derive(serde::Serialize)]
    struct ReplyEntry {
        state: crate::sensor::SensorState,
        label: Option<String>,
    }
    let txn = ctx.db.read_txn()?;

    let reply = sensors
        .iter()
        .map(|(addr, state)| {
            let db_entry = ctx.db.get_addr(&txn, *addr)?;
            Ok((
                addr,
                ReplyEntry {
                    state: *state,
                    label: db_entry.and_then(|entry| entry.label),
                },
            ))
        })
        .collect::<Result<Vec<_>, crate::db::Error>>()?;

    Ok(warp::reply::json(&reply))
}

async fn get_log(ctx: super::Context) -> Result<impl warp::Reply, warp::Rejection> {
    let txn = ctx.db.read_txn()?;
    let log = ctx.db.get_log(
        &txn,
        BluetoothAddress::from(0),
        Timestamp::UNIX_EPOCH..Timestamp::now(),
    )?;

    #[derive(serde::Serialize)]
    struct Entry {
        time: Timestamp,
        values: SensorValues,
    }

    Ok(warp::reply::json(
        &log.unwrap()
            .into_iter()
            .map(|(time, values)| Entry { time, values })
            .collect::<Vec<_>>(),
    ))
}
