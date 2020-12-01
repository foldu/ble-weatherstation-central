mod codec;

use codec::{MqttDecoder, MqttEncoder};
use futures_util::SinkExt;
use mqtt::{
    control::ConnectReturnCode,
    packet::{
        ConnectPacket, Packet, PingreqPacket, PingrespPacket, PublishPacket,
        QoSWithPacketIdentifier, VariablePacket,
    },
    Encodable,
};
use std::{convert::TryFrom, io, num::NonZeroU16, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    stream::{Stream, StreamExt},
    sync::{mpsc, Mutex},
    task,
};
use tokio_util::codec::{FramedRead, FramedWrite};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Can't connect to {url}")]
    Connect { url: String, source: io::Error },

    #[error("mqtt server refused connection")]
    ConnectionRefused { return_code: ConnectReturnCode },

    #[error("IO error from underlying stream")]
    Io(#[from] io::Error),

    #[error("Could not decode mqtt packet")]
    Decode(#[from] mqtt::packet::VariablePacketError),

    #[error("Received unexpected packet")]
    UnexpectedPacket,

    #[error("Could not serialize data as json")]
    Serialize(#[from] serde_json::Error),

    #[error("Invalid mqtt url {url}, for more information see https://github.com/mqtt/mqtt.org/wiki/URI-Scheme")]
    InvalidUrl { url: Url },

    #[error("mqtts currently not supported")]
    NotSupported,
}

pub(crate) struct Connection {
    buf: Vec<u8>,
    sink: PacketSink,
}

enum Scheme {
    Mqtt,
    MqttS,
}

pub(crate) struct ConnectOptions {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

impl TryFrom<&Url> for ConnectOptions {
    type Error = Error;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        let invalid_url = || Error::InvalidUrl { url: url.clone() };
        let port = match url.scheme() {
            "mqtt" => {
                tracing::warn!("Using non ssl mqtt");
                1883
            }
            "mqtts" => {
                return Err(Error::NotSupported);
                8883
            }
            _ => return Err(invalid_url()),
        };

        let port = url.port().unwrap_or(port);

        Ok(ConnectOptions {
            port,
            host: url
                .host_str()
                .map(ToOwned::to_owned)
                .ok_or_else(invalid_url)?,
            username: if url.username().is_empty() {
                None
            } else {
                Some(url.username().to_string())
            },
            password: url.password().map(ToOwned::to_owned),
        })
    }
}

impl Connection {
    pub async fn connect(
        // see: https://github.com/mqtt/mqtt.org/wiki/URI-Scheme
        url: &Url,
        client_id: &str,
        keep_alive: u16,
    ) -> Result<
        (
            Self,
            impl Stream<Item = (String, Vec<u8>)> + Send + Unpin + Sync,
        ),
        Error,
    > {
        let options = ConnectOptions::try_from(url).unwrap();
        let stream = TcpStream::connect((options.host, options.port))
            .await
            .map_err(|e| Error::Connect {
                url: url.to_string(),
                source: e,
            })?;

        let (r, w) = stream.into_split();
        let r: Box<dyn AsyncRead + Unpin + Send> = Box::new(r);
        let mut r = FramedRead::new(r, MqttDecoder::default());
        let sink = PacketSink::new(w);

        let mut buf = Vec::new();
        let mut packet = ConnectPacket::new("MQTT", client_id);
        packet.set_user_name(options.username);
        packet.set_password(options.password);
        packet.set_clean_session(true);
        packet.set_keep_alive(keep_alive);
        packet.encode(&mut buf).unwrap();
        sink.send_packet(packet).await?;

        match r.next().await.unwrap() {
            Ok(VariablePacket::ConnackPacket(packet)) => match packet.connect_return_code() {
                ConnectReturnCode::ConnectionAccepted => {}
                return_code => return Err(Error::ConnectionRefused { return_code }),
            },
            _ => {
                return Err(Error::UnexpectedPacket);
            }
        }

        let (pub_tx, pub_rx) = mpsc::channel(1);

        task::spawn(driver_task(sink.clone(), r, pub_tx));

        if let Ok(keep_alive) = NonZeroU16::try_from(keep_alive) {
            task::spawn(ping_task(sink.clone(), keep_alive));
        }

        Ok((Self { sink, buf }, pub_rx))
    }

    pub async fn publish_json(
        &mut self,
        topic_name: mqtt::TopicName,
        msg: &impl serde::Serialize,
    ) -> Result<(), Error> {
        let packet = PublishPacket::new(
            topic_name,
            QoSWithPacketIdentifier::Level0,
            serde_json::to_string(msg).unwrap(),
        );

        self.sink.send_packet(packet).await?;

        Ok(())
    }

    //pub async fn subscribe_many(
    //    &mut self,
    //    topic_filter: Vec<(TopicFilter, QualityOfService)>,
    //) -> Result<(), Error> {
    //    let packet = SubscribePacket::new(0, topic_filter);
    //}
}

async fn ping_task(sink: PacketSink, keep_alive: NonZeroU16) {
    let mut interval = tokio::time::interval(Duration::from_secs(u64::from(keep_alive.get())));
    while let Some(_) = interval.next().await {
        if let Err(e) = sink.send_packet(PingreqPacket::new()).await {
            tracing::error!("Failed sending ping packet: {}", e)
        }
    }
}

async fn driver_task(
    sink: PacketSink,
    mut r: FramedRead<Box<dyn AsyncRead + Unpin + Send>, MqttDecoder>,
    pub_tx: mpsc::Sender<(String, Vec<u8>)>,
) {
    while let Some(packet) = r.next().await {
        match packet {
            Ok(VariablePacket::PingreqPacket(_)) => {
                sink.send_packet(PingrespPacket::new()).await;
            }
            Ok(VariablePacket::PingrespPacket(_)) => {}
            Ok(VariablePacket::SubackPacket(sub_ack)) => {
                let id = sub_ack.packet_identifier();
                // TODO:
            }
            Ok(VariablePacket::PublishPacket(packet)) => {
                let topic = packet.topic_name().to_string();
                // don't care when recv dropped, just sent it into the trash
                let _ = pub_tx.send((topic, packet.payload())).await;
            }
            Ok(other) => {
                tracing::error!("Received unexpected packet {:#?}", other);
            }
            Err(e) => {
                tracing::error!("mqtt driver task failed to decode package: {}", e);
            }
        }
    }
    tracing::error!("PacketSink stream stopped");
}

#[derive(Clone)]
struct PacketSink(Arc<Mutex<FramedWrite<Box<dyn AsyncWrite + Unpin + Send>, MqttEncoder>>>);

impl PacketSink {
    fn new<W>(sink: W) -> Self
    where
        W: AsyncWrite + 'static + Unpin + Send,
    {
        Self(Arc::new(Mutex::new(FramedWrite::new(
            Box::new(sink),
            MqttEncoder,
        ))))
    }

    async fn send_packet(&self, packet: impl Encodable) -> Result<(), io::Error> {
        self.0.lock().await.send(packet).await
    }
}

//#[tokio::test]
//async fn test_mqtt() {
//    let url = Url::parse("mqtt://localhost").unwrap();
//    let (mut cxn, _) = Connection::connect(&url, "fish", 16).await.unwrap();
//    cxn.publish_json(
//        mqtt::TopicName::new("test/fish").unwrap(),
//        &vec![1_u8, 2, 3],
//    )
//    .await
//    .unwrap();
//
//    tokio::time::sleep(Duration::from_secs(60)).await;
//}
