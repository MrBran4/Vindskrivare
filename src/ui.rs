use core::fmt::Write;
use core::mem::discriminant;

use defmt::info;
use embassy_time::Timer;
use embedded_graphics::image::{ImageDrawable, ImageDrawableExt, ImageRaw};
use embedded_graphics::pixelcolor::raw::LittleEndian;
use embedded_graphics::prelude::{DrawTarget, IntoStorage, Point, RgbColor, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, StyledDrawable};
use embedded_graphics::{image::Image, image::ImageRawLE, pixelcolor::Rgb565, Drawable};

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Blocking, Spi};
use heapless::String;
use u8g2_fonts::types::HorizontalAlignment;
use u8g2_fonts::FontRenderer;

use crate::sen55::{Health, Readings};
use crate::{DelayWrapper, UI_READING_CHANNEL};

use defmt_rtt as _;

use st7789v2_driver::{FrameBuffer, ST7789V2};

pub type Display =
    ST7789V2<Spi<'static, SPI0, Blocking>, Output<'static>, Output<'static>, Output<'static>>;

const DISPLAY_W: u32 = 240;
const DISPLAY_H: u32 = 280;

const READING_WIDTH: u32 = 71;
const READING_HEIGHT: u32 = 26;
const READING_SEP: i32 = 66;
const FIRST_READING_Y: i32 = 28;

const GRAPH_WIDTH: u32 = 150;
const GRAPH_HEIGHT: u32 = 35;
const GRAPH_SEP: i32 = 66;
const FIRST_GRAPH_Y: i32 = 23;

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

const fn graph_pos(x: i32, index: u32) -> Point {
    Point::new(x, FIRST_GRAPH_Y + (GRAPH_SEP * index as i32))
}

const GRAPH_1_POS: Point = graph_pos(70, 0);
const GRAPH_2_POS: Point = graph_pos(70, 1);
const GRAPH_3_POS: Point = graph_pos(70, 2);
const GRAPH_4_POS: Point = graph_pos(70, 3);

const RAW_BG_STARTUP: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/bg-startup.bin"), DISPLAY_W);

const RAW_CONNECTING_WIFI: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/connect-wifi.bin"), DISPLAY_W);

const RAW_CONNECTING_DHCP: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/connect-dhcp.bin"), DISPLAY_W);

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

const RAW_BG_GRAPHS_OK: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/graphs-default.bin"), DISPLAY_W);

const RAW_BG_GRAPHS_UNHAPPY: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/graphs-unhappy.bin"), DISPLAY_W);

const RAW_BG_GRAPHS_DANGEROUS: ImageRawLE<'static, Rgb565> =
    ImageRawLE::new(include_bytes!("../ui/raw/graphs-dangerous.bin"), DISPLAY_W);

pub struct UiController {
    display: Display,

    /// Provides the ability to delay for a certain amount of time.
    delay: DelayWrapper,

    // The previous health of the readings, used to determine if the background needs to be redrawn
    last_health: Option<Health>,

    // History of readings for graphing
    history: HistoricReadings,

    frame_buffer_raw: [u8; 240 * 280 * 2],
}

pub enum ConnectionStage {
    Wifi,
    Dhcp,
    Ready,
}

impl UiController {
    pub fn new(display: Display, delay: DelayWrapper) -> Self {
        Self {
            display,
            delay,
            last_health: None,
            history: HistoricReadings::new(),
            frame_buffer_raw: [0; 240 * 280 * 2],
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
            ConnectionStage::Ready => Image::new(&RAW_CONNECTING_READY, Point::zero()),
        };

        img.draw(&mut self.display).unwrap();
    }

    pub fn render_readings_page(&mut self, readings: &Readings, first_of_cycle: bool) {
        // Work out the health of the readings
        let new_health = readings.health();
        let bg = match new_health {
            Health::Ok => &RAW_BG_READINGS_OK,
            Health::Warning => &RAW_BG_READINGS_UNHAPPY,
            Health::Dangerous => &RAW_BG_READINGS_DANGEROUS,
        };

        let mut this_frame_buffer =
            FrameBuffer::new(&mut self.frame_buffer_raw, DISPLAY_W, DISPLAY_H);

        // Draw the background straight into the temporary framebuffer (so it's behind the readings we're about to redraw)
        let img = Image::new(bg, Point::zero());
        if let Err(e) = bg.draw(&mut this_frame_buffer) {
            info!("Error drawing background: {}", e);
        }

        match (&self.last_health, first_of_cycle) {
            (_, true) => {
                // First time rendering this page, draw background directly to display
                img.draw(&mut self.display).unwrap();
            }
            (None, _) => {
                // First reading outright, draw background directly to display
                img.draw(&mut self.display).unwrap();
            }
            (Some(last_health), _) if discriminant(&new_health) != discriminant(last_health) => {
                // Health hasn changed, draw background directly to display
                img.draw(&mut self.display).unwrap();
            }
            _ => {
                // No change in health, nothing to be done
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

    /// Render the first page of graphs
    pub fn render_graphs_page(&mut self, readings: &Readings, first_of_cycle: bool) {
        // Work out the health of the readings
        let new_health = readings.health();
        let bg = match new_health {
            Health::Ok => &RAW_BG_GRAPHS_OK,
            Health::Warning => &RAW_BG_GRAPHS_UNHAPPY,
            Health::Dangerous => &RAW_BG_GRAPHS_DANGEROUS,
        };

        let mut this_frame_buffer =
            FrameBuffer::new(&mut self.frame_buffer_raw, DISPLAY_W, DISPLAY_H);

        // Draw the background straight into the temporary framebuffer (so it's behind the readings we're about to redraw)
        let img = Image::new(bg, Point::zero());
        bg.draw(&mut this_frame_buffer).unwrap();

        match (&self.last_health, first_of_cycle) {
            (_, true) => {
                // First time rendering this page, draw background directly to display
                img.draw(&mut self.display).unwrap();
            }
            (None, _) => {
                // First reading outright, draw background directly to display
                img.draw(&mut self.display).unwrap();
            }
            (Some(last_health), _) if discriminant(&new_health) != discriminant(last_health) => {
                // Health hasn changed, draw background directly to display
                img.draw(&mut self.display).unwrap();
            }
            _ => {
                // No change in health, nothing to be done
            }
        }

        // Draw the graphs
        draw_graph(&mut self.display, bg, GRAPH_1_POS, &self.history.pm1_0);
        draw_graph(&mut self.display, bg, GRAPH_2_POS, &self.history.pm2_5);
        draw_graph(&mut self.display, bg, GRAPH_3_POS, &self.history.voc);
        draw_graph(&mut self.display, bg, GRAPH_4_POS, &self.history.nox);

        self.last_health = Some(new_health);
    }
}

fn draw_reading<D>(
    display: &mut D,
    bg: &ImageRaw<'static, Rgb565, LittleEndian>,
    pos: Point,
    value: &f32,
) where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: core::fmt::Debug,
{
    let font = FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_logisoso24_tn>();

    let mut buf = String::<8>::new();

    // Differente decimal places based on the value, so its always fewer than 4 characters
    let content = match value {
        v if *v >= 100.0 => {
            write!(&mut buf, "{:.0}", v).unwrap();
            buf.as_str()
        }
        v if *v >= 10.0 => {
            write!(&mut buf, "{:.1}", v).unwrap();
            buf.as_str()
        }
        v if *v == 0.0 => "0",
        v if *v <= 10.0 => {
            write!(&mut buf, "{:.0}", v).unwrap();
            buf.as_str()
        }
        v if *v < 0.0 => {
            write!(&mut buf, "{:.1}", v).unwrap();
            buf.as_str()
        }
        v => {
            write!(&mut buf, "{:.2}", v).unwrap();
            buf.as_str()
        }
    };

    // Render background plate
    Image::new(
        &bg.sub_image(&Rectangle {
            top_left: pos,
            size: Size {
                width: READING_WIDTH,
                height: READING_HEIGHT,
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

const GRAPH_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(Rgb565::WHITE);

fn draw_graph<D>(
    display: &mut D,
    bg: &ImageRaw<'static, Rgb565, LittleEndian>,
    pos: Point,
    readings: &History,
) where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: core::fmt::Debug,
{
    // Render background plate
    Image::new(
        &bg.sub_image(&Rectangle {
            top_left: pos,
            size: Size {
                width: GRAPH_WIDTH,
                height: GRAPH_HEIGHT,
            },
        }),
        pos,
    )
    .draw(display)
    .unwrap();

    let graph_scale_min = readings
        .iter()
        .map(|el| (*el * 1000_f32) as i32)
        .min()
        .unwrap_or(0)
        / 1000;

    let graph_scale_max = readings
        .iter()
        .map(|el| (*el * 1000_f32) as i32)
        .max()
        .unwrap_or(0)
        / 1000;

    // Render the graph, right to left.
    readings.iter().enumerate().for_each(|(idx, reading)| {
        let x = pos.x + GRAPH_WIDTH as i32 - idx as i32 - 1;
        let y = pos.y + GRAPH_HEIGHT as i32
            - 1
            - (((*reading * 1000_f32) as i32 - graph_scale_min) * GRAPH_HEIGHT as i32)
                / (graph_scale_max - graph_scale_min);

        Rectangle::new(Point::new(x, y), Size::new(1, 1))
            .draw_styled(&GRAPH_STYLE, display)
            .unwrap()
    });
}

/// Consumes a UiController and draws readings to it whenever
/// new ones are recieved on the UI channel.
#[embassy_executor::task]
pub async fn worker(mut ui: UiController) {
    info!("started ui worker");

    let mut reading_idx = 0;

    loop {
        let readings = UI_READING_CHANNEL.receive().await;

        // Push the readings to the history
        ui.history.pm1_0.push(readings.pm1_0);
        ui.history.pm2_5.push(readings.pm2_5);
        ui.history.pm4_0.push(readings.pm4_0);
        ui.history.pm10_0.push(readings.pm10_0);
        ui.history.voc.push(readings.voc_index);
        ui.history.nox.push(readings.nox_index);
        ui.history.temp.push(readings.temperature);
        ui.history.humidity.push(readings.humidity);

        info!("Reading idx is {}", reading_idx);

        // Render the right page (at the right rate)
        match reading_idx {
            x @ 0..=25 if x % 5 == 0 => {
                info!("In range 0..=25 and %5");
                ui.render_readings_page(&readings, reading_idx == 0);
            }
            x @ 26..=50 if x % 3 == 0 => {
                info!("In range 26..=50 and %3");
                ui.render_graphs_page(&readings, reading_idx == 27);
            }
            _ => {
                info!("No render");
            }
        }

        // Readings come in once per second, but we don't show every one to reduce flicker
        // We also alternate between pages, 20s each.
        reading_idx = (reading_idx + 1) % 50;
    }
}

struct HistoricReadings {
    pm1_0: History,
    pm2_5: History,
    pm4_0: History,
    pm10_0: History,
    voc: History,
    nox: History,
    temp: History,
    humidity: History,
}

impl HistoricReadings {
    fn new() -> Self {
        Self {
            pm1_0: History::new(),
            pm2_5: History::new(),
            pm4_0: History::new(),
            pm10_0: History::new(),
            voc: History::new(),
            nox: History::new(),
            temp: History::new(),
            humidity: History::new(),
        }
    }
}

struct History {
    idx: usize,
    readings: [f32; GRAPH_WIDTH as usize],
}

impl History {
    fn new() -> Self {
        Self {
            idx: 0,
            readings: [0.0; GRAPH_WIDTH as usize],
        }
    }

    // Add a new reading to the history
    fn push(&mut self, value: f32) {
        self.readings[self.idx] = value;
        self.idx = (self.idx + 1) % GRAPH_WIDTH as usize;
    }

    // Get an iterator over the readings, newest to oldest.
    fn iter(&self) -> impl Iterator<Item = &f32> + '_ {
        self.readings[self.idx..]
            .iter()
            .chain(self.readings[..self.idx].iter())
    }
}
