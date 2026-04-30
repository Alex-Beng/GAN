fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("LED blink demo starting!");

    let peripherals = esp_idf_svc::hal::peripherals::Peripherals::take().unwrap();
    let mut led2 = esp_idf_svc::hal::gpio::PinDriver::output(peripherals.pins.gpio2).unwrap();
    let mut led8 = esp_idf_svc::hal::gpio::PinDriver::output(peripherals.pins.gpio8).unwrap();

    loop {
        led2.set_high().unwrap();
        led8.set_high().unwrap();
        log::info!("LED ON");
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(500);

        led2.set_low().unwrap();
        led8.set_low().unwrap();
        log::info!("LED OFF");
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(500);
    }
}