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
use std::{convert::TryFrom, io, num::NonZeroU16, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    stream::{Stream, StreamExt},
    sync::{mpsc, Mutex},
    task,
};
use tokio_rustls::webpki::{DNSName, DNSNameRef};
use tokio_util::codec::{FramedRead, FramedWrite};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
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
}

pub(crate) struct Connection {
    sink: PacketSink,
}

enum Scheme {
    Mqtt,
    MqttS { ca_pem: Vec<u8>, domain: DNSName },
}

pub(crate) struct ConnectOptions {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    scheme: Scheme,
}

type MqttStream = FramedRead<Box<dyn AsyncRead + Unpin + Send + Sync>, MqttDecoder>;
type MqttSink = FramedWrite<Box<dyn AsyncWrite + Unpin + Send + Sync>, MqttEncoder>;

pub(crate) enum Ssl {
    None,
    WithCert(Vec<u8>),
}

impl ConnectOptions {
    async fn connect(&self) -> Result<(MqttStream, MqttSink), eyre::Error> {
        let stream = TcpStream::connect((&self.host[..], self.port)).await?;
        match &self.scheme {
            Scheme::Mqtt => {
                let (r, w) = stream.into_split();
                // dedup
                Ok((
                    FramedRead::new(Box::new(r), MqttDecoder::default()),
                    FramedWrite::new(Box::new(w), MqttEncoder),
                ))
            }
            Scheme::MqttS { ca_pem, domain } => {
                let mut config = tokio_rustls::rustls::ClientConfig::new();
                config
                    .root_store
                    .add_pem_file(&mut io::Cursor::new(ca_pem))
                    .unwrap();
                let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
                let stream = connector.connect(domain.as_ref(), stream).await?;
                let (r, w) = tokio::io::split(stream);
                // this
                Ok((
                    FramedRead::new(Box::new(r), MqttDecoder::default()),
                    FramedWrite::new(Box::new(w), MqttEncoder),
                ))
            }
        }
    }

    pub fn new(url: &Url, ssl: Ssl) -> Result<Self, Error> {
        let invalid_url = || Error::InvalidUrl { url: url.clone() };
        let host = url
            .host_str()
            .map(ToOwned::to_owned)
            .ok_or_else(invalid_url)?;

        let (port, scheme) = match url.scheme() {
            "mqtt" => {
                tracing::warn!("Using non ssl mqtt");
                (1883, Scheme::Mqtt)
            }
            "mqtts" => (
                8883,
                Scheme::MqttS {
                    ca_pem: match ssl {
                        Ssl::None => panic!(),
                        Ssl::WithCert(cert) => cert,
                    },
                    domain: DNSNameRef::try_from_ascii_str(&host).unwrap().into(),
                },
            ),
            _ => return Err(invalid_url()),
        };

        let port = url.port().unwrap_or(port);

        Ok(ConnectOptions {
            port,
            host,
            username: if url.username().is_empty() {
                None
            } else {
                Some(url.username().to_string())
            },
            password: url.password().map(ToOwned::to_owned),
            scheme,
        })
    }
}

impl Connection {
    pub async fn connect(
        // see: https://github.com/mqtt/mqtt.org/wiki/URI-Scheme
        connect_options: &ConnectOptions,
        client_id: &str,
        keep_alive: u16,
    ) -> Result<
        (
            Self,
            impl Stream<Item = (String, Vec<u8>)> + Send + Unpin + Sync,
        ),
        Error,
    > {
        let (mut r, w) = connect_options.connect().await.unwrap();
        let sink = PacketSink::new(w);

        let mut packet = ConnectPacket::new("MQTT", client_id);
        packet.set_user_name(connect_options.username.clone());
        packet.set_password(connect_options.password.clone());
        packet.set_clean_session(true);
        packet.set_keep_alive(keep_alive);
        sink.send_packet(packet).await?;

        match r.next().await.unwrap() {
            Ok(VariablePacket::ConnackPacket(packet)) => match packet.connect_return_code() {
                ConnectReturnCode::ConnectionAccepted => {}
                return_code => return Err(Error::ConnectionRefused { return_code }),
            },
            e => {
                tracing::error!("{:#?}", e);
                return Err(Error::UnexpectedPacket);
            }
        }

        let (pub_tx, pub_rx) = mpsc::channel(1);

        task::spawn(driver_task(sink.clone(), r, pub_tx));

        if let Ok(keep_alive) = NonZeroU16::try_from(keep_alive) {
            task::spawn(ping_task(sink.clone(), keep_alive));
        }

        Ok((Self { sink }, pub_rx))
    }

    pub async fn publish_json(
        &mut self,
        topic_name: mqtt::TopicName,
        msg: &impl serde::Serialize,
    ) -> Result<(), Error> {
        let packet = PublishPacket::new(
            topic_name,
            QoSWithPacketIdentifier::Level0,
            serde_json::to_string(msg)?,
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
        tracing::trace!("Sending ping");
        if let Err(e) = sink.send_packet(PingreqPacket::new()).await {
            tracing::error!("Failed sending ping packet: {}", e)
        }
    }
}

async fn driver_task(sink: PacketSink, mut r: MqttStream, pub_tx: mpsc::Sender<(String, Vec<u8>)>) {
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
struct PacketSink(Arc<Mutex<MqttSink>>);

impl PacketSink {
    fn new(sink: MqttSink) -> Self {
        Self(Arc::new(Mutex::new(sink)))
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
