pub mod tcp;

use defmt::{error, info};
use embassy_executor::{SpawnError, Spawner};
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::Timer;
use esp_hal::{
    peripherals::WIFI,
    rng::{Trng, TrngError},
};
use esp_radio::{
    Controller, InitializationError,
    wifi::{ClientConfig, ModeConfig, WifiController, WifiDevice, WifiError, WifiEvent},
};
use static_cell::StaticCell;

const SOCKET_COUNT: usize = 3;

pub static ESP_RADIO_CTRL: StaticCell<Controller<'static>> = StaticCell::new();
pub static STACK_RESOURCES: StaticCell<StackResources<SOCKET_COUNT>> = StaticCell::new();

#[derive(defmt::Format)]
pub enum NetworkingInitError {
    RadioInitialization(InitializationError),
    Wifi(WifiError),
    Trng(TrngError),
    TaskSpawn(SpawnError),
}

impl From<InitializationError> for NetworkingInitError {
    fn from(err: InitializationError) -> Self {
        NetworkingInitError::RadioInitialization(err)
    }
}

impl From<WifiError> for NetworkingInitError {
    fn from(err: WifiError) -> Self {
        NetworkingInitError::Wifi(err)
    }
}

impl From<TrngError> for NetworkingInitError {
    fn from(err: TrngError) -> Self {
        NetworkingInitError::Trng(err)
    }
}

impl From<SpawnError> for NetworkingInitError {
    fn from(err: SpawnError) -> Self {
        NetworkingInitError::TaskSpawn(err)
    }
}

pub struct Networking<'d> {
    stack: Stack<'d>,
}

impl<'d> Networking<'d> {
    pub async fn new(wifi: WIFI<'static>, spawner: Spawner) -> Result<Self, NetworkingInitError> {
        let esp_radio_ctrl = ESP_RADIO_CTRL.init(esp_radio::init()?);

        let (controller, interfaces) =
            esp_radio::wifi::new(esp_radio_ctrl, wifi, Default::default())?;

        let wifi_interface = interfaces.sta;
        let config = embassy_net::Config::dhcpv4(Default::default());

        let rng = Trng::try_new()?;
        let net_seed = (rng.random() as u64) << 32 | rng.random() as u64;

        let (stack, runner) = embassy_net::new(
            wifi_interface,
            config,
            STACK_RESOURCES.init(StackResources::<SOCKET_COUNT>::new()),
            net_seed,
        );

        spawner.spawn(ap_connection(controller))?;
        spawner.spawn(net_task(runner))?;

        stack.wait_config_up().await;

        Ok(Self { stack })
    }
}

#[embassy_executor::task]
async fn ap_connection(mut controller: WifiController<'static>) {
    info!("Start connection task");

    const RETRY_INTERVAL_S: u64 = 5;
    loop {
        if let Err(err) = start_controller(&mut controller).await {
            error!("{}", err);
            Timer::after_secs(RETRY_INTERVAL_S).await;
            continue;
        }

        match controller.connect_async().await {
            Ok(_) => info!("Wifi connected!"),
            Err(e) => {
                error!("Failed to connect to wifi: {}", e);
                Timer::after_secs(5).await;
            }
        }

        controller.wait_for_event(WifiEvent::StaDisconnected).await;
        error!("Disconnected from wifi");
        Timer::after_secs(RETRY_INTERVAL_S).await;
    }
}

async fn start_controller(controller: &mut WifiController<'static>) -> Result<(), WifiError> {
    if controller.is_started()? {
        return Ok(());
    }

    let client_config = ModeConfig::Client(
        ClientConfig::default()
            .with_ssid(env!("SSID").into())
            .with_password(env!("PASSWORD").into()),
    );
    controller.set_config(&client_config)?;
    info!("Starting wifi controller");
    controller.start_async().await?;
    info!("Wifi controller started");

    Ok(())
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
