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
- Optional 3D printed enclosure here
