#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use cortex_m::delay::Delay;
use cyw43::JoinOptions;
use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_time::{Duration, Timer, WithTimeout};
use embedded_hal_1::delay::DelayNs;
use rand::RngCore;

use embassy_net::{Config, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::{RoscRng, clk_sys_freq};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::InterruptHandler as I2cInterruptHandler;
use embassy_rp::peripherals::{DMA_CH0, I2C0, I2C1, PIO0, PIO1};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::spi::{self, Spi};
use ui::{ConnectionStage, UiController};

use {defmt_rtt as _, panic_probe as _};

use sen55::Readings;
use st7789v2_driver::ST7789V2;
use static_cell::StaticCell;

mod avg;
mod config;
mod hass;
mod mqtt;
mod sen55;
mod ui;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    PIO1_IRQ_0 => InterruptHandler<PIO1>;
    I2C1_IRQ => I2cInterruptHandler<I2C1>;
    I2C0_IRQ => I2cInterruptHandler<I2C0>;
});

static MQTT_RX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
static MQTT_TX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
static MQTT_WORKING_BUFFER: StaticCell<[u8; 8192]> = StaticCell::new();

// Create channel for the sensor readings to be sent to the MQTT worker
static MQTT_READING_CHANNEL: embassy_sync::channel::Channel<ThreadModeRawMutex, Readings, 10> =
    embassy_sync::channel::Channel::new();

// Create channel for the sensor readings to be sent to the UI
static UI_READING_CHANNEL: embassy_sync::channel::Channel<ThreadModeRawMutex, Readings, 10> =
    embassy_sync::channel::Channel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // General setup
    let p = embassy_rp::init(Default::default());
    let core = cortex_m::Peripherals::take().unwrap();

    let mut rng = RoscRng;

    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

    info!("Hello world!");

    // Grab pins for the i2c to the SEN55 sensor.
    // Note Pin 6 on the sensor is not connected (even to ground).
    //
    // Pico VBUS -> Sensor VDD (Pin 1) red
    // Pico GND  -> Sensor GND (Pin 2) black
    // Pico GP26 -> Sensor SDA (Pin 3) green
    // Pico GP27 -> Sensor SCL (Pin 4) yellow
    // Pico GND  -> Sensor SEL (Pin 5) blue
    let i2c = embassy_rp::i2c::I2c::new_blocking(
        p.I2C1,
        p.PIN_27, // Laballed GP5 on the Pico, NOT the one labelled 'Pin 5' on the pinout!
        p.PIN_26, // Laballed GP4 on the Pico, NOT the one labelled 'Pin 4' on the pinout!
        embassy_rp::i2c::Config::default(),
    );

    let mut display_spi_cfg = spi::Config::default();
    display_spi_cfg.frequency = 64_000_000_u32; // 64 MHz
    display_spi_cfg.phase = spi::Phase::CaptureOnSecondTransition;
    display_spi_cfg.polarity = spi::Polarity::IdleHigh;

    // Display pins
    let display_clk = p.PIN_18; // GP18 -> CLK
    let display_mosi = p.PIN_19; // GP19 -> DIN
    let display_dc = Output::new(p.PIN_16, Level::Low); // GP16 -> DC
    let display_rst = Output::new(p.PIN_21, Level::Low); // GP21 -> RST
    let display_cs = Output::new(p.PIN_17, Level::High); // GP17 -> CS (assuming we only have one thing on the bus)
    let _display_bl = Output::new(p.PIN_22, Level::High); // GP22 -> BL (backlight, always on for now)

    let display_spi = Spi::new_blocking_txonly(p.SPI0, display_clk, display_mosi, display_spi_cfg);

    let screen_direction = st7789v2_driver::VERTICAL;
    let lcd_width = 240_u32;
    let lcd_height = 280_u32;
    // Initialize the display
    let display: ui::Display = ST7789V2::new(
        display_spi,
        display_dc,
        display_cs,
        display_rst,
        true, // 'is it rgb?' (yes)
        screen_direction,
        lcd_width,
        lcd_height,
    );

    // Set up the delay for the first core
    let delay_wrapper = DelayWrapper::new(Delay::new(core.SYST, clk_sys_freq()));

    // Hand off display to the UI module
    let mut display = ui::UiController::new(display, delay_wrapper);

    display.init().await;

    display.render_startup();

    Timer::after_secs(3).await;

    // Grab pins for the CYW43 (wifi chip); set up SPI to it.
    // Wifi chip is integrated into the pico and we use PIO to drive SPI to it.
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    // Start the CYW43 driver
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    spawner
        .spawn(cyw43_task(runner))
        .expect("couldn't spawn cyw43 worker task");

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let config = Config::dhcpv4(Default::default());

    // Generate random seed super securely
    let seed = rng.next_u64();

    // Init network stack
    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::new()),
        seed,
    );

    // Start embassy's network stack and wait for it to be ready
    spawner
        .spawn(net_task(runner))
        .expect("couldn't spawn net task");

    // Wait for the network to be connected
    wait_for_network(&mut control, &stack, &mut display).await;

    let mqtt_rx_buffer = MQTT_RX_BUFFER.init([0u8; 4096]);
    let mqtt_tx_buffer = MQTT_TX_BUFFER.init([0u8; 4096]);
    let mqtt_working_buffer = MQTT_WORKING_BUFFER.init([0u8; 8192]);
    spawner
        .spawn(mqtt::worker(
            stack,
            mqtt_rx_buffer,
            mqtt_tx_buffer,
            mqtt_working_buffer,
        ))
        .expect("Couldn't spawn mqtt task");

    display.render_connecting(ConnectionStage::Mqtt);

    spawner
        .spawn(sen55::worker(i2c))
        .expect("Couldn't spawn sen55 task");

    display.render_connecting(ConnectionStage::Ready);

    spawner
        .spawn(ui::worker(display))
        .expect("Couldn't spawn ui task");

    loop {
        info!("Main loop");

        Timer::after(Duration::from_secs(60)).await;
    }
}

/// Pokes the CYW43 driver to do hardware network stuff.
#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

/// Pokes embassy's network stack to do software network stuff.
#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

/// Wait (possibly forever) for the network to be connected.
async fn wait_for_network(
    control: &mut cyw43::Control<'_>,
    stack: &embassy_net::Stack<'_>,
    display: &mut UiController,
) {
    info!("Waiting for link up...");
    display.render_connecting(ConnectionStage::Wifi);

    loop {
        match control
            .join(
                config::WIFI_NETWORK,
                JoinOptions::new(config::WIFI_PASSWORD.as_bytes()),
            )
            .with_timeout(Duration::from_secs(30))
            .await
        {
            Ok(_) => break,
            Err(err) => {
                warn!("wifi join failed with status: {}", err);
            }
        }
    }

    display.render_connecting(ConnectionStage::Dhcp);

    // Wait for DHCP, not necessary when using static IP
    info!("Waiting for DHCP...");
    let mut retries = 60;
    while !stack.is_config_up() {
        Timer::after_millis(500).await;
        warn!("DHCP not up yet");

        retries -= 1;

        if retries == 0 {
            panic!("DHCP failed to come up within 30 seconds, giving up and resetting");
        }
    }

    info!("Waiting for link up...");
    let mut retries = 120;
    while !stack.is_link_up() {
        Timer::after_millis(500).await;
        warn!("Link not up yet");

        retries -= 1;

        if retries == 0 {
            panic!("Link layer failed to come up within 30 seconds, giving up and resetting");
        }
    }
    info!("Link up!");

    if let Some(ip) = stack.config_v4() {
        info!("IP address (v4): {}", ip.address);
    }
    if let Some(ip) = stack.config_v6() {
        info!("IP address (v6): {}", ip.address);
    }

    info!("Waiting network stack...");
    stack.wait_config_up().await;

    info!("Stack up!");
}

pub struct DelayWrapper {
    delay: Delay,
}

impl DelayWrapper {
    pub fn new(delay: Delay) -> Self {
        DelayWrapper { delay }
    }
}

impl DelayNs for DelayWrapper {
    fn delay_ns(&mut self, ns: u32) {
        let us = (ns + 999) / 1000; // Convert nanoseconds to microseconds
        self.delay.delay_us(us); // Use microsecond delay
    }
}
