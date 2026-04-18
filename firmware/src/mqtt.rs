use core::num::NonZeroU16;

use alloc::{
    format,
    string::{FromUtf8Error, String},
};
use defmt::{Debug2Format, error, info};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::Timer;
use rust_mqtt::{
    buffer::AllocBuffer,
    client::{
        Client as MqttClient, MqttError,
        event::Event,
        options::{ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference},
    },
    config::{KeepAlive, SessionExpiryInterval},
    types::{MqttBinary, MqttString, MqttStringError, TooLargeToEncode, TopicFilter, TopicName},
};
use serde::Deserialize;
use smart_leds::RGB8;

use crate::{
    led::{LED_COMMAND_CHANNEL, LedCommand},
    networking::{
        Networking,
        tcp::{TcpConnection, TcpConnectionError, TlsConnectionError},
    },
};

pub static MQTT_COMMAND_CHANNEL: Channel<CriticalSectionRawMutex, Command, 5> = Channel::new();

pub enum ButtonState {
    Pressed,
    Released,
}

pub enum Command {
    PublishButtonEvent(ButtonState),
}

#[derive(Deserialize)]
struct LedMessage {
    r: u8,
    g: u8,
    b: u8,
}

#[embassy_executor::task]
pub async fn mqtt_service(networking: Networking<'static>) {
    loop {
        if let Err(err) = mqtt_session(&networking).await {
            error!("Error running MQTT session: {}", err);
            Timer::after_secs(5).await;
        }
    }
}

#[derive(Debug, defmt::Format)]
enum MqttSessionError {
    ConnectOptions(ConnectOptionsError),
    TcpConnection(TcpConnectionError),
    TlsConnection(TlsConnectionError),
    Mqtt,
    MqttString(MqttStringError),
    TopicFilterCreation,
    MessageNotValidUtf8,
}

impl From<ConnectOptionsError> for MqttSessionError {
    fn from(err: ConnectOptionsError) -> Self {
        MqttSessionError::ConnectOptions(err)
    }
}

impl From<TcpConnectionError> for MqttSessionError {
    fn from(err: TcpConnectionError) -> Self {
        MqttSessionError::TcpConnection(err)
    }
}

impl From<TlsConnectionError> for MqttSessionError {
    fn from(err: TlsConnectionError) -> Self {
        MqttSessionError::TlsConnection(err)
    }
}

impl<'e> From<MqttError<'e>> for MqttSessionError {
    fn from(_: MqttError<'e>) -> Self {
        MqttSessionError::Mqtt
    }
}

impl From<MqttStringError> for MqttSessionError {
    fn from(err: MqttStringError) -> Self {
        MqttSessionError::MqttString(err)
    }
}

impl From<FromUtf8Error> for MqttSessionError {
    fn from(_: FromUtf8Error) -> Self {
        MqttSessionError::MessageNotValidUtf8
    }
}

async fn mqtt_session(networking: &Networking<'static>) -> Result<(), MqttSessionError> {
    const BROKER_ADDRESS: &str = env!("BROKER_ADDRESS");

    let tls_connection = TcpConnection::new(networking, BROKER_ADDRESS)
        .await?
        .with_tls()
        .await?;

    let mut mqtt_buffer = AllocBuffer;

    let mut client: MqttClient<_, _, 10, 10, 10, 4> = MqttClient::new(&mut mqtt_buffer);
    let connect_options = get_connect_options()?;

    client
        .connect(
            tls_connection,
            &connect_options,
            Some(MqttString::from_str(env!("CLIENT_ID"))?),
        )
        .await?;

    info!("connected");

    client
        .subscribe(
            TopicFilter::new(MqttString::from_str("led")?)
                .ok_or(MqttSessionError::TopicFilterCreation)?,
            SubscriptionOptions::new().at_least_once(),
        )
        .await?;

    let button_topic = TopicReference::Name(
        TopicName::new(MqttString::from_str("button")?)
            .ok_or(MqttSessionError::TopicFilterCreation)?,
    );
    let button_options = PublicationOptions::new(button_topic).at_least_once();

    loop {
        match select(MQTT_COMMAND_CHANNEL.receive(), client.poll()).await {
            Either::First(command) => match command {
                Command::PublishButtonEvent(button_state) => {
                    let message = match button_state {
                        ButtonState::Pressed => "pressed",
                        ButtonState::Released => "released",
                    };

                    if let Err(err) = client
                        .publish(&button_options, format_msg(message).as_str().into())
                        .await
                    {
                        if !err.is_recoverable() {
                            return Err(err.into());
                        } else {
                            error!("Error publishing button event: {}", err);
                        }
                    }
                }
            },

            Either::Second(poll_result) => match poll_result {
                Ok(event) => {
                    if let Event::Publish(publish_msg) = event {
                        let msg = match String::from_utf8(publish_msg.message.as_bytes().to_vec()) {
                            Ok(msg) => msg,
                            Err(err) => {
                                error!("Error parsing MQTT message: {}", Debug2Format(&err));
                                continue;
                            }
                        };

                        info!("Received message on led topic: {}", msg.as_str());

                        let (red, green, blue) =
                            match serde_json_core::from_str::<LedMessage>(msg.as_str()) {
                                Ok((LedMessage { r, g, b }, _)) => (r, g, b),
                                Err(err) => {
                                    error!("Error deserializing MQTT message: {}", err);
                                    continue;
                                }
                            };

                        LED_COMMAND_CHANNEL
                            .send(LedCommand::ChangeColor(RGB8::new(red, green, blue)))
                            .await
                    }
                }
                Err(err) => {
                    if !err.is_recoverable() {
                        return Err(err.into());
                    }
                    error!("MQTT Poll Error: {}", err);
                }
            },
        }
    }
}

#[derive(Debug, defmt::Format)]
enum ConnectOptionsError {
    Username(MqttStringError),
    MqttPasswordTooLong,
}

impl From<MqttStringError> for ConnectOptionsError {
    fn from(err: MqttStringError) -> Self {
        ConnectOptionsError::Username(err)
    }
}

impl From<TooLargeToEncode> for ConnectOptionsError {
    fn from(_: TooLargeToEncode) -> Self {
        ConnectOptionsError::MqttPasswordTooLong
    }
}

fn get_connect_options<'c>() -> Result<ConnectOptions<'c>, ConnectOptionsError> {
    let mut connect_options = ConnectOptions::new()
        .clean_start()
        .session_expiry_interval(SessionExpiryInterval::NeverEnd)
        .user_name(MqttString::from_str(env!("MQTT_USERNAME"))?)
        .password(MqttBinary::from_slice(env!("MQTT_PASSWORD").as_bytes())?);

    const KEEP_ALIVE_SECONDS: u16 = 120;
    if let Some(non_zero_seconds) = NonZeroU16::new(KEEP_ALIVE_SECONDS) {
        let keep_alive = KeepAlive::Seconds(non_zero_seconds);
        connect_options = connect_options.keep_alive(keep_alive)
    }

    Ok(connect_options)
}

fn format_msg(msg: &str) -> String {
    format!(r#"{{"msg":"{}"}}"#, msg)
}
