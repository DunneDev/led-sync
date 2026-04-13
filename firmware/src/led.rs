use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_hal_smartled::SmartLedsAdapter;
use smart_leds::{RGB8, SmartLedsWrite, brightness};

pub enum LedCommand {
    ChangeColor(RGB8),
}

pub static LED_COMMAND_CHANNEL: Channel<CriticalSectionRawMutex, LedCommand, 5> = Channel::new();

#[embassy_executor::task]
pub async fn led_task(mut led_adapter: SmartLedsAdapter<'static, 25>) {
    loop {
        match LED_COMMAND_CHANNEL.receive().await {
            LedCommand::ChangeColor(color) => led_adapter
                .write(brightness([color].into_iter(), 20))
                .unwrap(),
        }
    }
}
