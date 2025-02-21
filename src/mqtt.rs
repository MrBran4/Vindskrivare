use embassy_net::{dns::DnsQueryType, tcp::TcpSocket, Stack};
use embassy_time::Timer;
use log::{error, info};
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::reason_codes::ReasonCode,
    utils::rng_generator::CountingRng,
};

use crate::{avg::Hysterysiser, config, hass, sen55::Readings, MQTT_READING_CHANNEL};

/// Publishes updated readings to the MQTT broker, including the initial hass discovery message.
#[embassy_executor::task]
pub async fn worker(
    stack: Stack<'static>,
    rx_buffer: &'static mut [u8],
    tx_buffer: &'static mut [u8],
    work_buffer: &'static mut [u8],
) {
    info!("started mqtt worker");

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

    loop {
        Timer::after_millis(500).await;

        let mut socket = TcpSocket::new(stack, rx_buffer, tx_buffer);

        socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        let address = match stack
            .dns_query(config::MQTT_HOST, DnsQueryType::A)
            .await
            .map(|a| a[0])
        {
            Ok(address) => address,
            Err(e) => {
                error!("DNS lookup error: {e:?}");
                continue;
            }
        };

        let remote_endpoint = (address, 1883);
        info!("connecting...");
        let connection = socket.connect(remote_endpoint).await;
        if let Err(e) = connection {
            error!("connect error: {:?}", e);
            continue;
        }
        info!("connected!");

        let mut config = ClientConfig::new(
            rust_mqtt::client::client_config::MqttVersion::MQTTv5,
            CountingRng(20000),
        );
        config.add_max_subscribe_qos(rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS1);
        config.add_client_id(config::MQTT_CLIENT_ID);
        config.max_packet_size = 100;
        let mut recv_buffer = [0; 8192];
        let mut write_buffer = [0; 8192];

        let mut client = MqttClient::<_, 5, _>::new(
            socket,
            &mut write_buffer,
            8192,
            &mut recv_buffer,
            80,
            config,
        );

        match client.connect_to_broker().await {
            Ok(()) => {}
            Err(mqtt_error) => match mqtt_error {
                ReasonCode::NetworkError => {
                    error!("MQTT Network Error");
                    continue;
                }
                _ => {
                    error!("Other MQTT Error: {:?}", mqtt_error);
                    continue;
                }
            },
        }

        info!("Connected to MQTT Broker");

        // Always start by publishing a discovery message to Home Assistant.
        let discovery_payload = hass::get_discovery_payload();
        let serialized_len = match serde_json_core::to_slice(&discovery_payload, work_buffer) {
            Ok(serialized_len) => serialized_len,
            Err(e) => {
                error!("Error serializing discovery payload: {:?}", e);
                0
            }
        };

        match client
            .send_message(
                config::MQTT_TOPIC_DICSOVERY,
                &work_buffer[..serialized_len],
                rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                true,
            )
            .await
        {
            Ok(()) => {
                info!("Sent discovery message");
            }
            Err(mqtt_error) => match mqtt_error {
                ReasonCode::NetworkError => {
                    error!("Discovery message failed: MQTT Network Error");
                    continue;
                }
                _ => {
                    error!(
                        "Discovery message failed due to other MQTT Error: {:?}",
                        mqtt_error
                    );
                    continue;
                }
            },
        }

        loop {
            // Would be reading from the sensor channel here, for now just send a dummy message.
            let readings = MQTT_READING_CHANNEL.receive().await;

            // Push the new readings into the rolling averages.
            avg_pm1.push(readings.pm1_0 * 10_f32);
            avg_pm2_5.push(readings.pm2_5 * 10_f32);
            avg_pm4.push(readings.pm4_0 * 10_f32);
            avg_pm10.push(readings.pm10_0 * 10_f32);
            avg_voc.push(readings.voc_index);
            avg_nox.push(readings.nox_index);
            avg_temp.push(readings.temperature);
            avg_humidity.push(readings.humidity);

            let state_payload_len = match serde_json_core::to_slice(
                &hass::StateMessage::from(Readings {
                    pm1_0: avg_pm1.average(),
                    pm2_5: avg_pm2_5.average(),
                    pm4_0: avg_pm4.average(),
                    pm10_0: avg_pm10.average(),
                    voc_index: avg_voc.average(),
                    nox_index: avg_nox.average(),
                    temperature: avg_temp.average(),
                    humidity: avg_humidity.average(),
                }),
                work_buffer,
            ) {
                Ok(serialized_len) => serialized_len,
                Err(e) => {
                    error!("Error serializing state payload: {:?}", e);
                    continue;
                }
            };

            match client
                .send_message(
                    config::MQTT_TOPIC_STATE,
                    &work_buffer[..state_payload_len],
                    rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                    true,
                )
                .await
            {
                Ok(()) => {
                    info!("State message sent");
                }
                Err(mqtt_error) => match mqtt_error {
                    ReasonCode::NetworkError => {
                        error!("State publish failed: MQTT Network Error");
                        break;
                    }
                    _ => {
                        error!(
                            "State publish failed due to some other MQTT Error: {:?}",
                            mqtt_error
                        );
                        continue;
                    }
                },
            }
        }
    }
}
