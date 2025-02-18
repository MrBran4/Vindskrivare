pub const WIFI_NETWORK: &str = env!("WF_SSID");
pub const WIFI_PASSWORD: &str = env!("WF_PASS");

pub const MQTT_CLIENT_ID: &str = env!("MQTT_CLIENT_ID");
pub const MQTT_HOST: &str = env!("MQTT_HOST");

// Easier to construct all this stuff at compile time than to do it at runtime
// every time we need to send a message, which is very often.

pub const MQTT_TOPIC_DICSOVERY: &str = concat!(
    env!("MQTT_HASS_DISCOVERY_BASE"),
    "/device/",
    env!("HASS_DEVICE_IDENTIFIER"),
    "/config"
);

pub const MQTT_TOPIC_STATE: &str =
    concat!("/vindskrivare/", env!("HASS_DEVICE_IDENTIFIER"), "/state");

pub const HASS_DEVICE_IDENTIFIER: &str = env!("HASS_DEVICE_IDENTIFIER");
pub const HASS_DEVICE_NAME: &str = env!("HASS_DEVICE_NAME");
pub const HASS_DEVICE_MANUFACTURER: &str = "mrbran4";
pub const HASS_DEVICE_MODEL: &str = "Vindskrivare";
pub const HASS_DEVICE_SW: &str = env!("CARGO_PKG_VERSION");
pub const HASS_DEVICE_HW: &str = "PicoW_SEN55_v1.0";
pub const HASS_DEVICE_URL: &str = "https://github.com/mrbran4/Vindskrivare";

pub const CMP_TEMPERATURE: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_t");
pub const CMP_HUMIDITY: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_h");
pub const CMP_PM1: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_pm1");
pub const CMP_PM2_5: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_pm2_5");
pub const CMP_PM4: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_pm4");
pub const CMP_PM10: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_pm10");
pub const CMP_VOC: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_voc");
pub const CMP_NOX: &str = concat!(env!("HASS_DEVICE_IDENTIFIER"), "_nox");
