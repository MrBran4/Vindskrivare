use core::fmt::Write;
use core::mem::discriminant;

use defmt::{error, info};
use embassy_time::Timer;
use embedded_graphics::prelude::{DrawTarget, IntoStorage, Point, RgbColor};
use embedded_graphics::{image::Image, image::ImageRawLE, pixelcolor::Rgb565, Drawable};

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Blocking, Spi};
use heapless::String;
use u8g2_fonts::types::HorizontalAlignment;
use u8g2_fonts::FontRenderer;

use crate::sen55::{Health, Readings};
use crate::{DelayWrapper, UI_READING_CHANNEL};

use {defmt_rtt as _, panic_probe as _};

use st7789v2_driver::{FrameBuffer, Region, ST7789V2};

pub type Display =
    ST7789V2<Spi<'static, SPI0, Blocking>, Output<'static>, Output<'static>, Output<'static>>;

const DISPLAY_W: u32 = 240;
const DISPLAY_H: u32 = 280;
const CLEAR_STRIP_W: u32 = 71;

const READING_SEP: i32 = 60;
const FIRST_READING_Y: i32 = 37;

const PM1_POS: Point = reading_pos(63, 0);
const PM25_POS: Point = reading_pos(63, 1);
const PM4_POS: Point = reading_pos(63, 2);
const PM10_POS: Point = reading_pos(63, 3);

const TVOC_POS: Point = reading_pos(168, 0);
const TNOX_POS: Point = reading_pos(168, 1);
const TEMP_POS: Point = reading_pos(168, 2);
const HMTY_POS: Point = reading_pos(168, 3);

const fn reading_pos(x: i32, index: u32) -> Point {
    Point::new(x, FIRST_READING_Y + (READING_SEP * index as i32))
}

const READING_REGIONS: [Region; 8] = [
    Region {
        x: PM1_POS.x as u16,
        y: PM1_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
    Region {
        x: PM25_POS.x as u16,
        y: PM25_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
    Region {
        x: PM4_POS.x as u16,
        y: PM4_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
    Region {
        x: PM10_POS.x as u16,
        y: PM10_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
    Region {
        x: TVOC_POS.x as u16,
        y: TVOC_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
    Region {
        x: TNOX_POS.x as u16,
        y: TNOX_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
    Region {
        x: TEMP_POS.x as u16,
        y: TEMP_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
    Region {
        x: HMTY_POS.x as u16,
        y: HMTY_POS.y as u16,
        width: CLEAR_STRIP_W,
        height: 24,
    },
];

const RAW_BG_STARTUP: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/startup.rgb565"), DISPLAY_W);

const RAW_CONNECTING_WIFI: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-wifi.rgb565"),
    DISPLAY_W,
);

const RAW_CONNECTING_DHCP: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-dhcp.rgb565"),
    DISPLAY_W,
);

const RAW_CONNECTING_MQTT: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-mqtt.rgb565"),
    DISPLAY_W,
);

const RAW_CONNECTING_SEN55: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-sen55.rgb565"),
    DISPLAY_W,
);

const RAW_CONNECTING_READY: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-ready.rgb565"),
    DISPLAY_W,
);

const RAW_BG_READINGS_OK: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/readings-default.rgb565"),
    DISPLAY_W,
);

const RAW_BG_READINGS_UNHAPPY: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/readings-unhappy.rgb565"),
    DISPLAY_W,
);

const RAW_BG_READINGS_DANGEROUS: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/readings-dangerous.rgb565"),
    DISPLAY_W,
);

pub struct UiController {
    display: Display,

    /// Provides the ability to delay for a certain amount of time.
    delay: DelayWrapper,
}

#[allow(unused)]
pub enum ConnectionStage {
    Wifi,
    Dhcp,
    Mqtt,
    Sen55,
    Ready,
}

impl UiController {
    pub fn new(display: Display, delay: DelayWrapper) -> Self {
        Self { display, delay }
    }

    pub async fn init(&mut self) {
        self.display.hard_reset(&mut self.delay).unwrap();
        Timer::after_millis(500).await;
        self.display.init(&mut self.delay).unwrap();
        self.display
            .clear_screen(Rgb565::BLACK.into_storage())
            .unwrap();

        // Set up the regions for the readings
        for region in READING_REGIONS.iter() {
            self.display.store_region(*region).unwrap();
        }
    }

    pub fn render_startup(&mut self) {
        let img = Image::new(&RAW_BG_STARTUP, Point::zero());
        img.draw(&mut self.display).unwrap();
    }

    pub fn render_connecting(&mut self, connection_stage: ConnectionStage) {
        let img = match connection_stage {
            ConnectionStage::Wifi => Image::new(&RAW_CONNECTING_WIFI, Point::zero()),
            ConnectionStage::Dhcp => Image::new(&RAW_CONNECTING_DHCP, Point::zero()),
            ConnectionStage::Mqtt => Image::new(&RAW_CONNECTING_MQTT, Point::zero()),
            ConnectionStage::Sen55 => Image::new(&RAW_CONNECTING_SEN55, Point::zero()),
            ConnectionStage::Ready => Image::new(&RAW_CONNECTING_READY, Point::zero()),
        };

        img.draw(&mut self.display).unwrap();
    }

    pub fn render_readings(&mut self, readings: Readings) {
        // Work out the health of the readings
        let new_health = readings.health();
        let bg = match new_health {
            Health::Ok => &RAW_BG_READINGS_OK,
            Health::Warning => &RAW_BG_READINGS_UNHAPPY,
            Health::Dangerous => &RAW_BG_READINGS_DANGEROUS,
        };

        let mut this_frame_raw = [0; 240 * 280];
        let mut this_frame_buffer = FrameBuffer::new(&mut this_frame_raw, DISPLAY_W, DISPLAY_H);

        // Last health is different (or unset), redraw the background
        let img = Image::new(bg, Point::zero());
        img.draw(&mut this_frame_buffer).unwrap();

        // Draw the readings
        draw_reading(&mut this_frame_buffer, PM1_POS, &readings.pm1_0);
        draw_reading(&mut this_frame_buffer, PM25_POS, &readings.pm2_5);
        draw_reading(&mut this_frame_buffer, PM4_POS, &readings.pm4_0);
        draw_reading(&mut this_frame_buffer, PM10_POS, &readings.pm10_0);
        draw_reading(&mut this_frame_buffer, TVOC_POS, &readings.voc_index);
        draw_reading(&mut this_frame_buffer, TNOX_POS, &readings.nox_index);
        draw_reading(&mut this_frame_buffer, TEMP_POS, &readings.temperature);
        draw_reading(&mut this_frame_buffer, HMTY_POS, &readings.humidity);

        // render all the updated regions
        if self
            .display
            .show_regions(this_frame_buffer.get_buffer())
            .is_err()
        {
            error!("Failed to render regions");
        };
    }
}

fn draw_reading<D>(display: &mut D, pos: Point, value: &Option<f32>)
where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: core::fmt::Debug,
{
    let font = FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_logisoso24_tn>();

    let mut buf = String::<8>::new();

    // Differente decimal places based on the value, so its always fewer than 4 characters
    let content = match value {
        Some(v) if *v > 100.0 => {
            write!(&mut buf, "{:.0}", v).unwrap();
            buf.as_str()
        }
        Some(v) if *v > 10.0 => {
            write!(&mut buf, "{:.1}", v).unwrap();
            buf.as_str()
        }
        Some(v) => {
            write!(&mut buf, "{:.2}", v).unwrap();
            buf.as_str()
        }
        None => "...",
    };

    font.render_aligned(
        content,
        pos,
        u8g2_fonts::types::VerticalPosition::Top,
        HorizontalAlignment::Left,
        u8g2_fonts::types::FontColor::Transparent(Rgb565::WHITE),
        display,
    )
    .expect("couldn't render time");
}

/// Consumes a UiController and draws readings to it whenever
/// new ones are recieved on the UI channel.
#[embassy_executor::task]
pub async fn worker(mut ui: UiController) {
    info!("started ui worker");

    loop {
        let readings = UI_READING_CHANNEL.receive().await;
        ui.render_readings(readings);
    }
}
