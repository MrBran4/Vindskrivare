use embassy_rp::i2c::{Blocking, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_time::{Delay, Timer};
use log::{error, info, warn};

use crate::avg::Hysterysiser;
use crate::SHARED_CHANNEL;

pub struct Readings {
    pub pm1_0: Option<f32>,
    pub pm2_5: Option<f32>,
    pub pm4_0: Option<f32>,
    pub pm10_0: Option<f32>,
    pub voc_index: Option<f32>,
    pub nox_index: Option<f32>,
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
}

/// Polls the SEN55 sensor and sends the readings to the shared channel.
///
/// If the sensor fails to read too many times in a row, it will attempt to reinit the sensor, and
/// if that fails the board will be put into reset.
///
/// The sensor updates every 1s, is polled every 750ms, is hysterised over 30, 60, and 90 readings.
#[embassy_executor::task]
pub async fn worker(i2c: I2c<'static, I2C1, Blocking>) {
    info!("started sen55 worker");

    info!("Give sensor 5s to power up");
    Timer::after_secs(5).await;

    let mut sensor = sen5x_rs::Sen5x::new(i2c, Delay);
    if init_and_start_readings(&mut sensor).await.is_err() {
        error!("couldn't init sensor, board will reset");
        panic!("couldn't init sensor");
    }

    // Track the rolling averages of the last few readings to smooth out noise.
    // pm1.0, pm2.5, pm4.0, pm10.0 can change rapidly so we average over fewer readings.
    let mut avg_pm1 = Hysterysiser::<30>::new();
    let mut avg_pm2_5 = Hysterysiser::<30>::new();
    let mut avg_pm4 = Hysterysiser::<30>::new();
    let mut avg_pm10 = Hysterysiser::<30>::new();

    // tVOC and tNOx are slower to change so we average over more readings.
    let mut avg_voc = Hysterysiser::<60>::new();
    let mut avg_nox = Hysterysiser::<60>::new();

    // Temperature and humidity are also slow to change.
    let mut avg_temp = Hysterysiser::<90>::new();
    let mut avg_humidity = Hysterysiser::<90>::new();

    let mut recent_read_failures = 0;

    loop {
        Timer::after_millis(750).await;

        // If we've had too many read failures in a row, try to reinit the sensor.
        if recent_read_failures > 10 {
            warn!("Too many consecutive failures; reinitialising sensor");

            if init_and_start_readings(&mut sensor).await.is_err() {
                error!("couldn't init sensor, board will reset");
                panic!("couldn't init sensor");
            }

            // Reset the failure counter so we don't immediately reinit again.
            recent_read_failures = 0;
        };

        match sensor.data_ready_status() {
            Ok(false) => {
                // Data not ready yet, try again later.
                recent_read_failures += 1;
                continue;
            }
            Err(err) => {
                // Error reading data ready status, incremenent the failure counter.
                error!("Error reading data ready status: {:?}", err);
                recent_read_failures += 1;
                continue;
            }
            _ => {
                // Data is ready, reset the failure counter.
                recent_read_failures = 0;
            }
        }

        let measurement = match sensor.measurement() {
            Ok(measurement) => measurement,
            Err(err) => {
                error!("Error reading measurement: {:?}", err);

                recent_read_failures += 1;
                continue;
            }
        };

        // Push the new readings into the rolling averages.
        avg_pm1.push(measurement.pm1_0);
        avg_pm2_5.push(measurement.pm2_5);
        avg_pm4.push(measurement.pm4_0);
        avg_pm10.push(measurement.pm10_0);
        avg_voc.push(measurement.voc_index);
        avg_nox.push(measurement.nox_index);
        avg_temp.push(measurement.temperature);
        avg_humidity.push(measurement.humidity);

        // Publish the rolling averages.
        SHARED_CHANNEL
            .send(Readings {
                pm1_0: avg_pm1.average(),
                pm2_5: avg_pm2_5.average(),
                pm4_0: avg_pm4.average(),
                pm10_0: avg_pm10.average(),
                voc_index: avg_voc.average(),
                nox_index: avg_nox.average(),
                temperature: avg_temp.average(),
                humidity: avg_humidity.average(),
            })
            .await;
    }
}

async fn init_and_start_readings(
    sensor: &mut sen5x_rs::Sen5x<I2c<'static, I2C1, Blocking>, Delay>,
) -> Result<(), ()> {
    if let Err(e) = sensor.reinit() {
        error!("couldn't reinitialise sensor: {e:?}");
        return Err(());
    };

    match sensor.serial_number() {
        Ok(serial) => info!("Sensor serial: {}", serial),
        Err(e) => {
            error!("couldn't read sensor serial: {e:?}");
            return Err(());
        }
    }

    if let Err(e) = sensor.start_measurement() {
        error!("couldn't start readings: {e:?}");
        return Err(());
    }

    info!("Waiting for sensor to settle");
    Timer::after_secs(10).await;

    Ok(())
}
