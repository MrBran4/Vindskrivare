use defmt::{error, info, warn};
use embassy_rp::i2c::{Blocking, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_time::{Delay, Timer};
use sen5x_rs::Error;

use crate::{MQTT_READING_CHANNEL, UI_READING_CHANNEL};

pub struct Readings {
    pub pm1_0: f32,
    pub pm2_5: f32,
    pub pm4_0: f32,
    pub pm10_0: f32,
    pub voc_index: f32,
    pub nox_index: f32,
    pub temperature: f32,
    pub humidity: f32,
}

/// A vague health indicator for the overall readings.
pub enum Health {
    Ok,
    Warning,
    Dangerous,
}

impl Readings {
    pub fn health(&self) -> Health {
        // If any of the readings are above the threshold, we're in the danger zone.
        // unwrapping is safe because we've already checked that all the readings are Some.
        if self.pm1_0 > 100.0
            || self.pm2_5 > 100.0
            || self.pm4_0 > 100.0
            || self.pm10_0 > 100.0
            || self.voc_index > 400.0
            || self.nox_index > 5.0
        {
            return Health::Dangerous;
        }

        // Same thing but with warning thresholds.
        if self.pm1_0 > 25.0
            || self.pm2_5 > 25.0
            || self.pm4_0 > 25.0
            || self.pm10_0 > 25.0
            || self.voc_index > 225.0
            || self.nox_index > 2.5
        {
            return Health::Warning;
        }

        Health::Ok
    }
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

    let mut recent_read_failures = 0;

    loop {
        Timer::after_millis(1000).await;

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
                match err {
                    Error::Crc => warn!("Couldn't read sen5x readiness: CRC mismatch"),
                    Error::I2c(_) => error!("Couldn't read sen5x readiness: i2c mismatch"),
                    Error::Internal => error!("Couldn't read sen5x readiness: sensirion internal"),
                    Error::SelfTest => error!("Couldn't read sen5x readiness: self-test failure"),
                    Error::NotAllowed => error!("Couldn't read sen5x readiness: not allowed"),
                }
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
                match err {
                    Error::Crc => error!("Couldn't read sensor: CRC mismatch"),
                    Error::I2c(_) => error!("Couldn't read sensor: i2c mismatch"),
                    Error::Internal => error!("Couldn't read sensor: sensirion internal"),
                    Error::SelfTest => error!("Couldn't read sensor: self-test failure"),
                    Error::NotAllowed => error!("Couldn't read sensor: not allowed"),
                }

                recent_read_failures += 1;
                continue;
            }
        };

        // Publish the rolling averages.
        MQTT_READING_CHANNEL
            .send(Readings {
                pm1_0: measurement.pm1_0 * 10_f32,
                pm2_5: measurement.pm2_5 * 10_f32,
                pm4_0: measurement.pm4_0 * 10_f32,
                pm10_0: measurement.pm10_0 * 10_f32,
                voc_index: measurement.voc_index,
                nox_index: measurement.nox_index,
                temperature: measurement.temperature,
                humidity: measurement.humidity,
            })
            .await;

        if UI_READING_CHANNEL
            .try_send(Readings {
                pm1_0: measurement.pm1_0 * 10_f32,
                pm2_5: measurement.pm2_5 * 10_f32,
                pm4_0: measurement.pm4_0 * 10_f32,
                pm10_0: measurement.pm10_0 * 10_f32,
                voc_index: measurement.voc_index,
                nox_index: measurement.nox_index,
                temperature: measurement.temperature,
                humidity: measurement.humidity,
            })
            .is_err()
        {
            warn!("UI's readings channel is full, it might be struggling to keep up");
        };
    }
}

async fn init_and_start_readings(
    sensor: &mut sen5x_rs::Sen5x<I2c<'static, I2C1, Blocking>, Delay>,
) -> Result<(), ()> {
    if let Err(e) = sensor.reinit() {
        match e {
            Error::Crc => warn!("Couldn't init sensor: CRC mismatch"),
            Error::I2c(_) => error!("Couldn't init sensor: i2c mismatch"),
            Error::Internal => error!("Couldn't init sensor: sensirion internal"),
            Error::SelfTest => error!("Couldn't init sensor: self-test failure"),
            Error::NotAllowed => error!("Couldn't init sensor: not allowed"),
        }
        return Err(());
    };

    match sensor.serial_number() {
        Ok(serial) => info!("Sensor serial: {}", serial),
        Err(e) => {
            match e {
                Error::Crc => warn!("Couldn't read sen5x serial: CRC mismatch"),
                Error::I2c(_) => error!("Couldn't read sen5x serial: i2c mismatch"),
                Error::Internal => error!("Couldn't read sen5x serial: sensirion internal"),
                Error::SelfTest => error!("Couldn't read sen5x serial: self-test failure"),
                Error::NotAllowed => error!("Couldn't read sen5x serial: not allowed"),
            }
            return Err(());
        }
    }

    if let Err(e) = sensor.start_measurement() {
        match e {
            Error::Crc => warn!("Couldn't start readings: CRC mismatch"),
            Error::I2c(_) => error!("Couldn't start readings: i2c mismatch"),
            Error::Internal => error!("Couldn't start readings: sensirion internal"),
            Error::SelfTest => error!("Couldn't start readings: self-test failure"),
            Error::NotAllowed => error!("Couldn't start readings: not allowed"),
        }
        return Err(());
    }

    info!("Waiting for sensor to settle");
    Timer::after_secs(5).await;

    Ok(())
}
