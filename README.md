# Vindskrivare

Make your own air quality sensor similar to IKEA's Vindstyrka using a Pi Pico W and Sensirion SEN5x sensor.

#### Why not just by a Vindstyrka?

The Vindstyrka is a pretty sound device and actually has a Sensirion SEN54 inside of it, so it's not that different.

- It's fun to build your own stuff
- It's cheaper
- IKEA chose not to read the pm1.0, 4, 10 or NOx values for some reason, even though the sensor provides them
- IKEA reports temperature as an integer, even though the sensor provides a float

#### What do I need?

- Raspberry Pi Pico W
- Sensirion SEN5x sensor (I used [SEN55](https://www.mouser.co.uk/ProductDetail/403-SEN55-SDN-T) but SEN54 should work too)
- To connect the two, use [this cable](https://www.mouser.co.uk/ProductDetail/403-SEN5XJUMPERCABLE)
- Optional 3D printed enclosure (link soon, doesn't fit in case lol)

#### How do I build it?

You'll need these environment variables set:

- `WF_SSID` Your WiFi network name
- `WF_PASS` Your WiFi password
- `MQTT_CLIENT_ID` Client ID to connect to MQTT as. Pick something unique.
- `MQTT_HOST` Hostname of your MQTT broker (without `mqtt://` or port)
- `MQTT_HASS_DISCOVERY_BASE` The base topic for Home Assistant discovery, almost definitely `homeassistant`
- `HASS_DEVICE_NAME` Friendly name of the device, e.g. `Hallway Vindskrivare`
- `HASS_DEVICE_IDENTIFIER` Unique (preferably short) identifier for the device in Home Assistant. e.g. `hwvindskr`
- `HASS_DEVICE_SN` Invent a unique serial number for your device
