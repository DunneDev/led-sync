use embassy_time::Timer;
use esp_hal::gpio::Input;

use crate::mqtt::{ButtonState, Command, MQTT_COMMAND_CHANNEL};

#[embassy_executor::task]
pub async fn button_task(mut button: Input<'static>) {
    loop {
        button.wait_for_falling_edge().await;
        MQTT_COMMAND_CHANNEL
            .send(Command::PublishButtonEvent(ButtonState::Pressed))
            .await;

        Timer::after_millis(50).await;

        button.wait_for_rising_edge().await;
        MQTT_COMMAND_CHANNEL
            .send(Command::PublishButtonEvent(ButtonState::Released))
            .await;

        Timer::after_millis(50).await;
    }
}
