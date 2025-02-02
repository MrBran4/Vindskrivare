use embassy_rp::i2c::{Blocking, I2c};
use embassy_rp::peripherals::I2C0;
use embassy_time::{Delay, Timer};
use log::{error, info};

/// Polls the SEN55 sensor and for now just prints the it to serial.
#[embassy_executor::task]
pub async fn worker(i2c: I2c<'static, I2C0, Blocking>) {
    info!("started sen55 worker");

    info!("Give sensor 5s to power up");
    Timer::after_secs(5).await;

    let mut sensor = sen5x_rs::Sen5x::new(i2c, Delay);
    if let Err(e) = sensor.reinit() {
        error!("couldn't reinitialise sensor: {e:?}");
        Timer::after_secs(2).await;
        panic!("couldn't reinitialise sensor");
    };

    match sensor.serial_number() {
        Ok(serial) => info!("Sensor serial: {}", serial),
        Err(e) => {
            error!("couldn't read sensor serial: {e:?}");
            Timer::after_secs(2).await;
            panic!("couldn't read sensor serial");
        }
    }

    if let Err(e) = sensor.start_measurement() {
        error!("couldn't start readings: {e:?}");
        Timer::after_secs(2).await;
        panic!("couldn't start readings");
    }

    info!("Waiting for sensor to settle");
    Timer::after_secs(10).await;

    loop {
        Timer::after_millis(250).await;

        match sensor.data_ready_status() {
            Ok(false) => {
                continue;
            }
            Err(err) => {
                error!("Error reading data ready status: {:?}", err);
                continue;
            }
            _ => {}
        }

        let measurement = match sensor.measurement() {
            Ok(measurement) => measurement,
            Err(err) => {
                error!("Error reading measurement: {:?}", err);
                continue;
            }
        };

        info!("PM1.0: {}", measurement.pm1_0);
        info!("PM2.5: {}", measurement.pm2_5);
        info!("PM4.0: {}", measurement.pm4_0);
        info!("PM10:  {}", measurement.pm10_0);
        info!("tVOC:  {}", measurement.voc_index);
        info!("tNOx:  {}", measurement.nox_index);
        info!("Temp:  {}", measurement.temperature);
        info!("Humid: {}", measurement.humidity);
    }
}
