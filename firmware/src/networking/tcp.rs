use defmt::info;
use embassy_net::{
    IpEndpoint, dns,
    tcp::{ConnectError, TcpSocket},
};
use embedded_io::ErrorType;
use embedded_tls::{
    Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext, TlsError, UnsecureProvider,
};
use esp_hal::rng::{Trng, TrngError};
use smoltcp::wire::DnsQueryType;
use static_cell::StaticCell;

use crate::networking::Networking;

const TCP_BUFF_SIZE: usize = 4096;
static TCP_TX_BUFF: StaticCell<[u8; TCP_BUFF_SIZE]> = StaticCell::new();
static TCP_RX_BUFF: StaticCell<[u8; TCP_BUFF_SIZE]> = StaticCell::new();

const TLS_BUFF_SIZE: usize = 16640;
static TLS_TX_BUFF: StaticCell<[u8; TLS_BUFF_SIZE]> = StaticCell::new();
static TLS_RX_BUFF: StaticCell<[u8; TLS_BUFF_SIZE]> = StaticCell::new();

#[derive(Debug, defmt::Format)]
pub enum TcpConnectionError {
    Dns(dns::Error),
    AddressNotFound,
    Connection(ConnectError),
}

impl From<dns::Error> for TcpConnectionError {
    fn from(err: dns::Error) -> Self {
        TcpConnectionError::Dns(err)
    }
}

impl From<ConnectError> for TcpConnectionError {
    fn from(err: ConnectError) -> Self {
        TcpConnectionError::Connection(err)
    }
}

pub struct TcpConnection<'d> {
    socket: TcpSocket<'d>,
    address_str: &'static str,
}

impl<'d> ErrorType for TcpConnection<'d> {
    type Error = embedded_io::ErrorKind;
}

impl<'d> embedded_io_async::Read for TcpConnection<'d> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.socket
            .read(buf)
            .await
            .map_err(|_| embedded_io::ErrorKind::ConnectionReset)
    }
}

impl<'d> embedded_io_async::Write for TcpConnection<'d> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.socket
            .write(buf)
            .await
            .map_err(|_| embedded_io::ErrorKind::ConnectionReset)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.socket
            .flush()
            .await
            .map_err(|_| embedded_io::ErrorKind::ConnectionReset)
    }
}

#[derive(Debug, defmt::Format)]
pub enum TlsConnectionError {
    Tls(TlsError),
    Trng(TrngError),
}

impl From<TlsError> for TlsConnectionError {
    fn from(err: TlsError) -> Self {
        TlsConnectionError::Tls(err)
    }
}

impl From<TrngError> for TlsConnectionError {
    fn from(err: TrngError) -> Self {
        TlsConnectionError::Trng(err)
    }
}

impl<'d> TcpConnection<'d> {
    pub async fn new(
        networking: &Networking<'static>,
        address_str: &'static str,
    ) -> Result<Self, TcpConnectionError> {
        let addresses = networking
            .stack
            .dns_query(address_str, DnsQueryType::A)
            .await?;
        let address = match addresses.first() {
            Some(address) => address,
            None => return Err(TcpConnectionError::AddressNotFound),
        };

        const MQTT_PORT: u16 = 8883;
        let endpoint = IpEndpoint::new(*address, MQTT_PORT);

        info!("Connecting to {}", address);

        let rx_buff = TCP_RX_BUFF.init([0; TCP_BUFF_SIZE]);
        let tx_buff = TCP_TX_BUFF.init([0; TCP_BUFF_SIZE]);

        let mut socket = TcpSocket::new(networking.stack, rx_buff, tx_buff);
        socket.connect(endpoint).await?;

        Ok(Self {
            socket,
            address_str,
        })
    }

    pub async fn with_tls(
        self,
    ) -> Result<TlsConnection<'d, TcpConnection<'d>, Aes128GcmSha256>, TlsConnectionError> {
        let tx_buff = TLS_TX_BUFF.init([0; TLS_BUFF_SIZE]);
        let rx_buff = TLS_RX_BUFF.init([0; TLS_BUFF_SIZE]);

        let address = self.address_str;
        let rng = Trng::try_new().map_err(TlsConnectionError::from)?;
        let mut tls: TlsConnection<_, Aes128GcmSha256> = TlsConnection::new(self, rx_buff, tx_buff);

        info!("Starting handshake");

        let config = TlsConfig::new()
            .with_server_name(address)
            .enable_rsa_signatures();

        tls.open(TlsContext::new(
            &config,
            UnsecureProvider::new::<Aes128GcmSha256>(rng),
        ))
        .await?;

        info!("Handshake complete");

        Ok(tls)
    }
}
