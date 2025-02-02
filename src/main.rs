//! This example uses the RP Pico W board Wifi chip (cyw43).
//! Connects to Wifi network and makes a web request to get the current time.

#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use core::panic::PanicInfo;
use core::sync::atomic::{self, Ordering};

use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use log::{error, info, warn};
use rand::RngCore;

use embassy_net::{Config, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::InterruptHandler as I2cInterruptHandler;
use embassy_rp::peripherals::I2C1;
use embassy_rp::peripherals::{DMA_CH0, I2C0, PIO0, USB};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::usb::{Driver, InterruptHandler as UsbInterruptHandler};

use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    USBCTRL_IRQ => UsbInterruptHandler<USB>;
    I2C1_IRQ => I2cInterruptHandler<I2C1>;
    I2C0_IRQ => I2cInterruptHandler<I2C0>;
});

const WIFI_NETWORK: &str = env!("WF_SSID");
const WIFI_PASSWORD: &str = env!("WF_PASS");

mod net;
mod sen55;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut rng = RoscRng;

    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver)).unwrap();

    // Wait for USB serial to be ready (or for the other end to start listening, not sure)
    Timer::after_secs(5).await;
    info!("USB serial logging up");

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

    info!("set up i2c ");
    /*
    Pico VBUS  ->  Sensor VDD (Pin 1) red
    Pico GND   ->  Sensor GND (Pin 2) black
    Pico GP4   ->  Sensor SDA (Pin 3) green
    Pico GP5   ->  Sensor SCL (Pin 4) yellow
    Pico GND   ->  Sensor SEL (Pin 5) blue
               ->  Sensor NC  (Pin 6) purple (do not connect)
    */
    let sda = p.PIN_4; // Marked GP4 on the board
    let scl = p.PIN_5; // Marked GP5 on the board
    let i2c =
        embassy_rp::i2c::I2c::new_blocking(p.I2C0, scl, sda, embassy_rp::i2c::Config::default());
    info!("i2c configured");

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

    // Generate random seed
    let seed = rng.next_u64();

    // Init network stack
    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner
        .spawn(net_task(runner))
        .expect("couldn't spawn net task");

    loop {
        match control
            .join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
            .await
        {
            Ok(_) => break,
            Err(err) => {
                warn!("wifi join failed with status: {}", err.status);
            }
        }
    }

    // Wait for DHCP, not necessary when using static IP
    info!("Waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
        warn!("DHCP not up yet")
    }

    info!("Waiting for link up...");
    while !stack.is_link_up() {
        Timer::after_millis(500).await;
        warn!("Link not up yet")
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

    spawner
        .spawn(net::worker(stack))
        .expect("Couldn't spawn net task");

    info!("Spawning sen55 task");
    Timer::after_secs(5).await;
    spawner
        .spawn(sen55::worker(i2c))
        .expect("Couldn't spawn sen55 task");

    loop {
        info!("Main loop");

        Timer::after(Duration::from_secs(5)).await;
    }
}

/// Pipes log messages over USB serial back to whatever just flashed us.
#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[panic_handler]
fn handle_panic(info: &PanicInfo) -> ! {
    error!("Panicking: {}", info);

    cortex_m::peripheral::SCB::sys_reset();
}
