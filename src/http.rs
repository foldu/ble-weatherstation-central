use crate::{templates, DbUuid};
use futures_util::stream::{self, StreamExt};
use tokio::signal::unix::{signal, SignalKind};
use uuid::Uuid;
use warp::Filter;

pub(crate) fn serve(ctx: super::Context) -> (std::net::SocketAddr, impl warp::Future) {
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

    let get_state = warp::get()
        .and(warp::path!("api" / "state"))
        .and(ctx.clone())
        .and_then(get_state);

    let term = signal(SignalKind::terminate()).unwrap();
    let int = signal(SignalKind::interrupt()).unwrap();
    let shutdown = async move {
        stream::select(term, int).next().await;
    };

    let routes = home.or(change_label).or(get_state);

    warp::serve(routes).bind_with_graceful_shutdown(([127, 0, 0, 1], 42069), shutdown)
}

async fn show_sensors(ctx: super::Context) -> Result<impl warp::Reply, std::convert::Infallible> {
    let sensors = ctx.sensors.read().await;
    let mut display = Vec::with_capacity(sensors.len());
    let r_txn = ctx.env.read_txn().unwrap();
    for (uuid, state) in sensors.iter() {
        match ctx
            .uuid_db
            .get(&r_txn, &super::DbUuid::new(uuid.as_u128()))
            .unwrap()
        {
            Some(entry) => display.push((
                *uuid,
                templates::SensorEntry {
                    state: *state,
                    label: entry.label.clone(),
                },
            )),
            None => {}
        }
    }

    Ok(askama_warp::reply(&templates::Home::new(&display), "html"))
}

#[derive(serde::Deserialize)]
struct ChangeLabel {
    uuid: Uuid,
    new_label: Option<String>,
}

async fn change_label(
    ctx: super::Context,
    req: ChangeLabel,
) -> Result<impl warp::Reply, std::convert::Infallible> {
    let mut w_txn = ctx.env.write_txn().unwrap();
    let db_uuid = DbUuid::new(req.uuid.as_u128());
    if ctx.uuid_db.get(&w_txn, &db_uuid).unwrap().is_some() {
        ctx.uuid_db
            .put(
                &mut w_txn,
                &db_uuid,
                &super::UuidDbEntry {
                    label: req.new_label,
                },
            )
            .unwrap();
    }

    Ok(warp::reply::with_status("", warp::http::StatusCode::OK))
}

async fn get_state(ctx: super::Context) -> Result<impl warp::Reply, std::convert::Infallible> {
    let sensors = ctx.sensors.read().await;
    #[derive(serde::Serialize)]
    struct ReplyEntry {
        state: crate::sensor::SensorState,
        label: Option<String>,
    }
    let r_txn = ctx.env.read_txn().unwrap();

    let reply = sensors
        .iter()
        .map(|(uuid, state)| {
            let db_entry = ctx
                .uuid_db
                .get(&r_txn, &DbUuid::new(uuid.as_u128()))
                .unwrap();
            (
                uuid,
                ReplyEntry {
                    state: *state,
                    label: db_entry.and_then(|entry| entry.label),
                },
            )
        })
        .collect::<Vec<_>>();

    Ok(warp::reply::json(&reply))
}
