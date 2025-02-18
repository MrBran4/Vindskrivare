use embassy_net::{dns::DnsQueryType, tcp::TcpSocket, Stack};
use embassy_time::Timer;
use log::{error, info};
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::reason_codes::ReasonCode,
    utils::rng_generator::CountingRng,
};

use crate::{config, hass, MQTT_READING_CHANNEL};

/// Publishes updated readings to the MQTT broker, including the initial hass discovery message.
#[embassy_executor::task]
pub async fn worker(
    stack: Stack<'static>,
    rx_buffer: &'static mut [u8],
    tx_buffer: &'static mut [u8],
    work_buffer: &'static mut [u8],
) {
    info!("started mqtt worker");

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

            let state_payload_len =
                match serde_json_core::to_slice(&hass::StateMessage::from(readings), work_buffer) {
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
