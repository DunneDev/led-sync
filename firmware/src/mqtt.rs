use defmt::info;
use embassy_net::{
    IpEndpoint, Stack,
    dns::DnsQueryType,
    tcp::{self, TcpSocket},
};
use embassy_time::Timer;
use embedded_tls::{Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext, UnsecureProvider};
use esp_hal::rng::Trng;
use rust_mqtt::{
    buffer::AllocBuffer,
    client::{
        Client as MqttClient,
        options::{ConnectOptions, PublicationOptions, TopicReference},
    },
    config::SessionExpiryInterval,
    types::{MqttBinary, MqttString, TopicName},
};

const TCP_BUFF_SIZE: usize = 4096;
const TLS_BUFF_SIZE: usize = 16640;

#[derive(Debug)]
struct MqttTcpError(tcp::Error);

impl core::fmt::Display for MqttTcpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::write(f, format_args!("{:?}", self.0))
    }
}

impl core::error::Error for MqttTcpError {}

impl embedded_io::Error for MqttTcpError {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self.0 {
            tcp::Error::ConnectionReset => embedded_io::ErrorKind::ConnectionReset,
        }
    }
}

struct MqttTcpSocket<'a>(TcpSocket<'a>);

impl<'a> MqttTcpSocket<'a> {
    fn new(socket: TcpSocket<'a>) -> Self {
        Self(socket)
    }
}

impl<'a> embedded_io_async::ErrorType for MqttTcpSocket<'a> {
    type Error = MqttTcpError;
}

impl<'a> embedded_io_async::Read for MqttTcpSocket<'a> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf).await.map_err(MqttTcpError)
    }
}

impl<'a> embedded_io_async::Write for MqttTcpSocket<'a> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0.write(buf).await.map_err(MqttTcpError)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.flush().await.map_err(MqttTcpError)
    }
}

#[embassy_executor::task]
pub async fn mqtt_task(stack: Stack<'static>, rng: Trng) {
    const BROKER_ADDRESS: &str = env!("BROKER_ADDRESS");

    let addresses = stack
        .dns_query(BROKER_ADDRESS, DnsQueryType::A)
        .await
        .unwrap();
    let address = addresses.first().unwrap();
    let endpoint = IpEndpoint::new(*address, 8883);

    info!("Connecting to MQTT broker");

    let mut rx_buffer = [0; TCP_BUFF_SIZE];
    let mut tx_buffer = [0; TCP_BUFF_SIZE];
    let mut socket = MqttTcpSocket::new(TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer));
    socket.0.connect(endpoint).await.unwrap();

    let mut read_record_buffer = [0; TLS_BUFF_SIZE];
    let mut write_record_buffer = [0; TLS_BUFF_SIZE];
    let mut tls: TlsConnection<_, Aes128GcmSha256> =
        TlsConnection::new(socket, &mut read_record_buffer, &mut write_record_buffer);

    info!("Starting handshake");

    let config = TlsConfig::new()
        .with_server_name(BROKER_ADDRESS)
        .enable_rsa_signatures();

    tls.open(TlsContext::new(
        &config,
        UnsecureProvider::new::<Aes128GcmSha256>(rng),
    ))
    .await
    .unwrap();

    info!("Handshake complete");

    let mut mqtt_buffer = AllocBuffer;
    let mut client: MqttClient<_, _, 10, 10, 10, 4> = MqttClient::new(&mut mqtt_buffer);

    let connect_options = ConnectOptions::new()
        .clean_start()
        .session_expiry_interval(SessionExpiryInterval::NeverEnd)
        .user_name(MqttString::from_str(env!("MQTT_USERNAME")).unwrap())
        .password(MqttBinary::from_slice(env!("MQTT_PASSWORD").as_bytes()).unwrap());

    client
        .connect(
            tls,
            &connect_options,
            Some(MqttString::from_str(env!("CLIENT_ID")).unwrap()),
        )
        .await
        .unwrap();

    info!("connected");

    let button_topic =
        TopicReference::Name(TopicName::new(MqttString::from_str("button").unwrap()).unwrap());
    let button_options = PublicationOptions::new(button_topic).at_least_once();

    let message = r#"{"msg":"pressed"}"#;
    client
        .publish(&button_options, message.into())
        .await
        .unwrap()
        .unwrap();

    info!("Sent message: {}", message);

    loop {
        Timer::after_secs(10).await;
    }
}
