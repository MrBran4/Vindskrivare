use heapless::LinearMap;
use serde::Serialize;
use serde_json_core as _;

use crate::{
    config::{self, CMP_TEMPERATURE},
    sen55,
};

#[derive(Debug, Serialize)]
pub struct DiscoveryMessage<'a> {
    #[serde(rename = "dev")]
    pub device: DiscoveryDevice<'a>,

    #[serde(rename = "o")]
    pub origin: DiscoveryOrigin<'a>,

    #[serde(rename = "state_topic")]
    pub state_topic: &'a str,

    #[serde(rename = "cmps")]
    pub components: LinearMap<&'a str, DiscoveryComponent<'a>, 8>,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryDevice<'a> {
    #[serde(rename = "ids")]
    pub identifier: &'a str,
    #[serde(rename = "name")]
    pub name: &'a str,
    #[serde(rename = "mf")]
    pub manufacturer: &'a str,
    #[serde(rename = "mdl")]
    pub model: &'a str,
    #[serde(rename = "sw")]
    pub sw_version: &'a str,
    #[serde(rename = "hw")]
    pub hw_version: &'a str,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryOrigin<'a> {
    #[serde(rename = "name")]
    pub name: &'a str,
    #[serde(rename = "sw")]
    pub sw_version: &'a str,
    #[serde(rename = "url")]
    pub url: &'a str,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryComponent<'a> {
    #[serde(rename = "p")]
    pub platform: &'a str,
    #[serde(rename = "device_class")]
    pub device_class: &'a str,
    #[serde(rename = "unit_of_measurement")]
    pub unit_of_measurement: &'a str,
    #[serde(rename = "name")]
    pub name: &'a str,
    #[serde(rename = "value_template")]
    pub value_template: &'a str,
    #[serde(rename = "unique_id")]
    pub unique_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct StateMessage {
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
    pub pm1: Option<f32>,
    pub pm2_5: Option<f32>,
    pub pm4: Option<f32>,
    pub pm10: Option<f32>,
    pub voc: Option<f32>,
    pub nox: Option<f32>,
}

impl From<sen55::Readings> for StateMessage {
    fn from(readings: sen55::Readings) -> Self {
        Self {
            temperature: readings.temperature,
            humidity: readings.humidity,
            pm1: readings.pm1_0,
            pm2_5: readings.pm2_5,
            pm4: readings.pm4_0,
            pm10: readings.pm10_0,
            voc: readings.voc_index,
            nox: readings.nox_index,
        }
    }
}

pub fn get_discovery_payload() -> DiscoveryMessage<'static> {
    let mut out = DiscoveryMessage {
        device: DiscoveryDevice {
            identifier: config::HASS_DEVICE_IDENTIFIER,
            name: config::HASS_DEVICE_NAME,
            manufacturer: config::HASS_DEVICE_MANUFACTURER,
            model: config::HASS_DEVICE_MODEL,
            sw_version: config::HASS_DEVICE_SW,
            hw_version: config::HASS_DEVICE_HW,
        },
        origin: DiscoveryOrigin {
            name: config::HASS_DEVICE_NAME,
            sw_version: config::HASS_DEVICE_SW,
            url: config::HASS_DEVICE_URL,
        },
        state_topic: config::MQTT_TOPIC_STATE,
        components: LinearMap::new(),
    };

    _ = out.components.insert(
        config::CMP_TEMPERATURE,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "temperature",
            unit_of_measurement: "°C",
            name: "Temperature",
            value_template: "{{ value_json.temperature }}",
            unique_id: CMP_TEMPERATURE,
        },
    );

    _ = out.components.insert(
        config::CMP_HUMIDITY,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "humidity",
            unit_of_measurement: "%",
            name: "Humidity",
            value_template: "{{ value_json.humidity }}",
            unique_id: config::CMP_HUMIDITY,
        },
    );

    _ = out.components.insert(
        config::CMP_PM1,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "pm1",
            unit_of_measurement: "µg/m³",
            name: "PM1.0",
            value_template: "{{ value_json.pm1 }}",
            unique_id: config::CMP_PM1,
        },
    );

    _ = out.components.insert(
        config::CMP_PM2_5,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "pm25",
            unit_of_measurement: "µg/m³",
            name: "PM2.5",
            value_template: "{{ value_json.pm2_5 }}",
            unique_id: config::CMP_PM2_5,
        },
    );

    _ = out.components.insert(
        config::CMP_PM4,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "pm25",
            unit_of_measurement: "µg/m³",
            name: "PM4.0",
            value_template: "{{ value_json.pm4 }}",
            unique_id: config::CMP_PM4,
        },
    );

    _ = out.components.insert(
        config::CMP_PM10,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "pm10",
            unit_of_measurement: "µg/m³",
            name: "PM10.0",
            value_template: "{{ value_json.pm10 }}",
            unique_id: config::CMP_PM10,
        },
    );

    _ = out.components.insert(
        config::CMP_VOC,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "volatile_organic_compounds",
            unit_of_measurement: "µg/m³",
            name: "tVOC",
            value_template: "{{ value_json.voc }}",
            unique_id: config::CMP_VOC,
        },
    );

    _ = out.components.insert(
        config::CMP_NOX,
        DiscoveryComponent {
            platform: "sensor",
            device_class: "nitrous_oxide",
            unit_of_measurement: "ppb",
            name: "tNOx",
            value_template: "{{ value_json.nox }}",
            unique_id: config::CMP_NOX,
        },
    );

    out
}
