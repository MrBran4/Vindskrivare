use core::fmt::Write;
use core::mem::discriminant;

use defmt::info;
use embassy_time::Timer;
use embedded_graphics::image::{ImageDrawable, ImageDrawableExt, ImageRaw};
use embedded_graphics::pixelcolor::raw::LittleEndian;
use embedded_graphics::prelude::{DrawTarget, IntoStorage, Point, RgbColor, Size};
use embedded_graphics::primitives::Rectangle;
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
const READING_WIDTH: u32 = 70;
const READING_HEIGHT: u32 = 24;

const READING_SEP: i32 = 66;
const FIRST_READING_Y: i32 = 28;

const PM1_POS: Point = reading_pos(60, 0);
const PM25_POS: Point = reading_pos(60, 1);
const PM4_POS: Point = reading_pos(60, 2);
const PM10_POS: Point = reading_pos(60, 3);

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
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
    Region {
        x: PM25_POS.x as u16,
        y: PM25_POS.y as u16,
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
    Region {
        x: PM4_POS.x as u16,
        y: PM4_POS.y as u16,
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
    Region {
        x: PM10_POS.x as u16,
        y: PM10_POS.y as u16,
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
    Region {
        x: TVOC_POS.x as u16,
        y: TVOC_POS.y as u16,
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
    Region {
        x: TNOX_POS.x as u16,
        y: TNOX_POS.y as u16,
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
    Region {
        x: TEMP_POS.x as u16,
        y: TEMP_POS.y as u16,
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
    Region {
        x: HMTY_POS.x as u16,
        y: HMTY_POS.y as u16,
        width: READING_WIDTH,
        height: READING_HEIGHT,
    },
];

const RAW_BG_STARTUP: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/bg-startup.bin"), DISPLAY_W);

const RAW_CONNECTING_WIFI: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/connect-wifi.bin"), DISPLAY_W);

const RAW_CONNECTING_DHCP: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/connect-dhcp.bin"), DISPLAY_W);

const RAW_CONNECTING_MQTT: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/connect-mqtt.bin"), DISPLAY_W);

const RAW_CONNECTING_SEN55: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/connect-sen55.bin"), DISPLAY_W);

const RAW_CONNECTING_READY: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/connect-ready.bin"), DISPLAY_W);

const RAW_BG_READINGS_OK: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/readings-default.bin"), DISPLAY_W);

const RAW_BG_READINGS_UNHAPPY: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/readings-unhappy.bin"), DISPLAY_W);

const RAW_BG_READINGS_DANGEROUS: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/readings-dangerous.bin"),
    DISPLAY_W,
);

pub struct UiController {
    display: Display,

    /// Provides the ability to delay for a certain amount of time.
    delay: DelayWrapper,

    // The previous health of the readings, used to determine if the background needs to be redrawn
    last_health: Option<Health>,

    // We only show every 5th reading to reduce flicker.
    // This counter is used to keep track.
    reading_skip: u8,
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
        Self {
            display,
            delay,
            last_health: None,
            reading_skip: 0,
        }
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
        // Skip some readings to reduce flicker
        if self.reading_skip < 5 {
            self.reading_skip += 1;
            return;
        } else {
            self.reading_skip = 0;
        }

        // Work out the health of the readings
        let new_health = readings.health();
        let bg = match new_health {
            Health::Ok => &RAW_BG_READINGS_OK,
            Health::Warning => &RAW_BG_READINGS_UNHAPPY,
            Health::Dangerous => &RAW_BG_READINGS_DANGEROUS,
        };

        let mut this_frame_raw = [0; 240 * 280 * 2];
        let mut this_frame_buffer = FrameBuffer::new(&mut this_frame_raw, DISPLAY_W, DISPLAY_H);

        // Last health is different (or unset), redraw the background
        let img = Image::new(bg, Point::zero());
        bg.draw(&mut this_frame_buffer).unwrap();

        if self.last_health.is_none() {
            // First time rendering, draw background directly to display
            img.draw(&mut self.display).unwrap();
        }

        if let Some(last_health) = &self.last_health {
            if discriminant(&new_health) != discriminant(last_health) {
                // Health hasn changed, draw background directly to display
                img.draw(&mut self.display).unwrap();
            }
        }

        // Draw the readings
        draw_reading(&mut self.display, bg, PM1_POS, &readings.pm1_0);
        draw_reading(&mut self.display, bg, TVOC_POS, &readings.voc_index);
        draw_reading(&mut self.display, bg, PM10_POS, &readings.pm10_0);
        draw_reading(&mut self.display, bg, HMTY_POS, &readings.humidity);
        draw_reading(&mut self.display, bg, PM25_POS, &readings.pm2_5);
        draw_reading(&mut self.display, bg, TNOX_POS, &readings.nox_index);
        draw_reading(&mut self.display, bg, PM4_POS, &readings.pm4_0);
        draw_reading(&mut self.display, bg, TEMP_POS, &readings.temperature);

        self.last_health = Some(new_health);
    }
}

fn draw_reading<D>(
    display: &mut D,
    bg: &ImageRaw<'static, Rgb565, LittleEndian>,
    pos: Point,
    value: &Option<f32>,
) where
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

    // Render background plate
    Image::new(
        &bg.sub_image(&Rectangle {
            top_left: pos,
            size: Size {
                width: 71,
                height: 26,
            },
        }),
        pos,
    )
    .draw(display)
    .unwrap();

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
