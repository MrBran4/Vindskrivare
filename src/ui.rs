use core::fmt::Write;
use core::mem::discriminant;

use embassy_time::Timer;
use embedded_graphics::prelude::{Dimensions, IntoStorage, Point, RgbColor};
use embedded_graphics::{image::Image, image::ImageRawLE, pixelcolor::Rgb565, Drawable};

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Blocking, Spi};
use heapless::String;
use u8g2_fonts::types::HorizontalAlignment;
use u8g2_fonts::FontRenderer;

use crate::sen55::{Health, Readings};
use crate::DelayWrapper;

use {defmt_rtt as _, panic_probe as _};

use st7789v2_driver::ST7789V2;

pub type Display =
    ST7789V2<Spi<'static, SPI0, Blocking>, Output<'static>, Output<'static>, Output<'static>>;

const IMG_WIDTH: u32 = 240;
const CLEAR_STRIP_WIDTH: u32 = 71;

const CLEAR_STRIP_L_POS: Point = Point::new(58, 0);
const CLEAR_STRIP_R_POS: Point = Point::new(163, 0);

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

const RAW_BG_STARTUP: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/startup.rgb565"), IMG_WIDTH);

const RAW_CONNECTING_WIFI: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-wifi.rgb565"),
    IMG_WIDTH,
);

const RAW_CONNECTING_DHCP: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-dhcp.rgb565"),
    IMG_WIDTH,
);

const RAW_CONNECTING_MQTT: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-mqtt.rgb565"),
    IMG_WIDTH,
);

const RAW_CONNECTING_SEN55: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-sen55.rgb565"),
    IMG_WIDTH,
);

const RAW_CONNECTING_READY: ImageRawLE<'static, Rgb565> = ImageRawLE::new(
    include_bytes!("../ui/raw/connecting-ready.rgb565"),
    IMG_WIDTH,
);

const RAW_BG_READINGS_OK: Backgrounds = Backgrounds {
    bg: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-default.rgb565"),
        IMG_WIDTH,
    ),
    clear_l: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-default-blank-l.rgb565"),
        CLEAR_STRIP_WIDTH,
    ),
    clear_r: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-default-blank-r.rgb565"),
        CLEAR_STRIP_WIDTH,
    ),
};

const RAW_BG_READINGS_UNHAPPY: Backgrounds = Backgrounds {
    bg: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-unhappy.rgb565"),
        IMG_WIDTH,
    ),
    clear_l: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-unhappy-blank-l.rgb565"),
        CLEAR_STRIP_WIDTH,
    ),
    clear_r: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-unhappy-blank-r.rgb565"),
        CLEAR_STRIP_WIDTH,
    ),
};

const RAW_BG_READINGS_DANGEROUS: Backgrounds = Backgrounds {
    bg: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-dangerous.rgb565"),
        IMG_WIDTH,
    ),
    clear_l: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-dangerous-blank-l.rgb565"),
        CLEAR_STRIP_WIDTH,
    ),
    clear_r: ImageRawLE::new(
        include_bytes!("../ui/raw/readings-dangerous-blank-r.rgb565"),
        CLEAR_STRIP_WIDTH,
    ),
};

pub struct UiController<'a> {
    display: Display,

    /// Provides the ability to delay for a certain amount of time.
    delay: DelayWrapper<'a>,

    /// The health of the previous reading.
    /// Used to determine whether we need to redraw the background or just the clear strips.
    /// (if the health hasn't changed, the background colour will be the same)
    last_health: Option<Health>,
}

pub enum ConnectionStage {
    Wifi,
    Dhcp,
    Mqtt,
    Sen55,
    Ready,
}

struct Backgrounds {
    bg: ImageRawLE<'static, Rgb565>,
    clear_l: ImageRawLE<'static, Rgb565>,
    clear_r: ImageRawLE<'static, Rgb565>,
}

impl<'a> UiController<'a> {
    pub fn new(display: Display, delay: DelayWrapper<'a>) -> Self {
        Self {
            display,
            delay,
            last_health: None,
        }
    }

    pub async fn init(&mut self) {
        self.display.hard_reset(&mut self.delay).unwrap();
        Timer::after_millis(500).await;
        self.display.init(&mut self.delay).unwrap();
        self.display
            .clear_screen(Rgb565::BLACK.into_storage())
            .unwrap();
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

        // If the health has changed, redraw the background
        // Otherwise, just redraw the clear strips
        match &self.last_health {
            Some(last_health) if discriminant(last_health) == discriminant(&new_health) => {
                // Last health is same as new health, just redraw the clear strips
                let img = Image::new(&bg.clear_l, CLEAR_STRIP_L_POS);
                img.draw(&mut self.display).unwrap();

                let img = Image::new(&bg.clear_r, CLEAR_STRIP_R_POS);
                img.draw(&mut self.display).unwrap();
            }
            _ => {
                // Last health is different (or unset), redraw the background
                let img = Image::new(&bg.bg, Point::zero());
                img.draw(&mut self.display).unwrap();
            }
        }

        // Update the last health so we can compare it next time
        self.last_health = Some(new_health);

        // Draw the readings
        draw_reading(&mut self.display, PM1_POS, &readings.pm1_0);
        draw_reading(&mut self.display, PM25_POS, &readings.pm2_5);
        draw_reading(&mut self.display, PM4_POS, &readings.pm4_0);
        draw_reading(&mut self.display, PM10_POS, &readings.pm10_0);
        draw_reading(&mut self.display, TVOC_POS, &readings.voc_index);
        draw_reading(&mut self.display, TNOX_POS, &readings.nox_index);
        draw_reading(&mut self.display, TEMP_POS, &readings.temperature);
        draw_reading(&mut self.display, HMTY_POS, &readings.humidity);
    }
}

fn draw_reading(display: &mut Display, pos: Point, value: &Option<f32>) {
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
        None => "N/A",
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
