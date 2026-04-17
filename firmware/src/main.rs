#![no_std]
#![no_main]

mod button;
mod led;
mod mqtt;
mod networking;

use crate::networking::Networking;
use defmt::{error, info};
use embassy_executor::Spawner;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Pull},
    rmt::{PulseCode, Rmt},
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use panic_rtt_target as _;
use static_cell::StaticCell;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

static LED_BUFFER: StaticCell<[PulseCode; 25]> = StaticCell::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    rtt_target::rtt_init_defmt!();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 56 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    let networking = match Networking::new(peripherals.WIFI, spawner).await {
        Ok(networking) => networking,
        Err(err) => {
            error!("Failed to initialize networking stack: {}", err);
            panic!("Fatal error: unable to proceed without networking stack")
        }
    };

    spawner.spawn(mqtt::mqtt_task(networking)).unwrap();

    let button = Input::new(
        peripherals.GPIO9,
        InputConfig::default().with_pull(Pull::Up),
    );

    spawner.spawn(button::button_task(button)).unwrap();

    let led_buffer = LED_BUFFER.init(smart_led_buffer!(1));
    let led_adapter = {
        let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).unwrap();
        SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO8, led_buffer)
    };

    spawner.spawn(led::led_task(led_adapter)).unwrap();
    core::future::pending::<()>().await;
}
