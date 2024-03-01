use std::{fmt::Display, format, fs, println, thread, time::SystemTime};

use chrono::{DateTime, Local};
use fontdue::Font;
use hidapi::{DeviceInfo, HidApi, HidDevice, HidError};
use itertools::Itertools;
use sysinfo::{CpuExt, System, SystemExt};

pub const PAYLOAD_SIZE: usize = 32;

fn is_my_device(device: &DeviceInfo) -> bool {
    device.vendor_id() == 0x4B42 && device.product_id() == 0x6072 && device.usage_page() == 0xFF60
}

pub trait HidAdapter {
    fn write(&self, data: &[u8]) -> Result<usize, HidError>;

    fn as_any(&self) -> &dyn std::any::Any;
}

impl HidAdapter for HidDevice {
    fn write(&self, data: &[u8]) -> Result<usize, HidError> {
        self.write(data)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(PartialEq, Clone)]
pub struct DataPacket {
    index: u8,
    payload: [u8; PAYLOAD_SIZE - 2],
}

impl DataPacket {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![1, self.index];
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    pub fn send(&self, device: &dyn HidAdapter) -> Result<(), HidError> {
        let bytes = self.to_bytes();

        // println!("{:?}", bytes)/* ; */
        device.write(&bytes)?;

        Ok(())
    }

    pub fn new(starting_index: u8, payload: [u8; PAYLOAD_SIZE - 2]) -> Self {
        Self {
            index: starting_index,
            payload,
        }
    }
}

struct Screen {
    width: usize,
    height: usize,
    data: Vec<u8>,
    _prev_packets: Option<Vec<DataPacket>>,
    device: Box<dyn HidAdapter>,
}

impl Display for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = self
            .data
            .iter()
            .chunks(self.width / 8)
            .into_iter()
            .map(|row| row.map(|byte| format!("{byte:08b}")).join(""))
            .join("\n")
            .replace("0", "░")
            .replace("1", "▓");

        f.write_str(&string)
    }
}

impl Screen {
    pub fn from_device(
        device: impl HidAdapter + 'static,
        width: usize,
        height: usize,
    ) -> Result<Self, HidError> {
        Ok(Self {
            data: vec![0; (width * height) / 8],
            device: Box::new(device),
            width,
            height,
            _prev_packets: None,
        })
    }

    pub(crate) fn to_packets(&self) -> Vec<DataPacket> {
        self.data
            .iter()
            .chunks(PAYLOAD_SIZE - 2)
            .into_iter()
            .map(|chunk| {
                let mut output_array: [u8; PAYLOAD_SIZE - 2] = [0; PAYLOAD_SIZE - 2];
                chunk
                    .take(PAYLOAD_SIZE - 2)
                    .enumerate()
                    .for_each(|(index, byte)| output_array[index] = *byte);
                output_array
            })
            .enumerate()
            .map(|(index, chunk)| DataPacket::new(index.try_into().unwrap(), chunk))
            .collect()
    }

    pub fn draw_text(
        &mut self,
        text: &str,
        x: isize,
        y: isize,
        size: f32,
        font_path: Option<&str>,
        spacing: isize,
    ) {
        let font = if let Some(font_path) = font_path {
            let font_bytes = fs::read(&font_path).unwrap();
            Font::from_bytes(font_bytes, fontdue::FontSettings::default()).unwrap()
        } else {
            Font::from_bytes(
                include_bytes!("../NANOTYPE.ttf") as &[u8],
                fontdue::FontSettings::default(),
            )
            .unwrap()
        };

        let mut x_cursor = x;

        for letter in text.chars() {
            let width = font.metrics(letter, size).width as isize;
            self.draw_letter(letter, x_cursor, y, size, &font);

            // FIXME: Use horizontal kerning as opposed to abstract value of "2"
            x_cursor += width + spacing;
        }
    }

    fn draw_time(&mut self, time: SystemTime, font_size: f64, font_path: Option<String>) {
        let font = if let Some(font_path) = font_path {
            let font_bytes = fs::read(&font_path).unwrap();
            Font::from_bytes(font_bytes, fontdue::FontSettings::default()).unwrap()
        } else {
            Font::from_bytes(
                include_bytes!("../NANOTYPE.ttf") as &[u8],
                fontdue::FontSettings::default(),
            )
            .unwrap()
        };

        let formatted_time: DateTime<Local> = time.into();
        let time_string = formatted_time.format("%I:%M %p").to_string();
        let mut width_needed = 0;

        time_string.chars().into_iter().for_each(|c| {
            let (metrics, _) = font.rasterize(c, font_size as f32);
            width_needed += metrics.width as isize + font_size as isize / 24;
        });

        self.draw_text(
            &time_string,
            (128 - width_needed) / 2,
            10,
            font_size as f32,
            None,
            font_size as isize / 24,
        )
    }

    fn draw_letter(&mut self, letter: char, x: isize, y: isize, size: f32, font: &Font) {
        let (metrics, bitmap) = font.rasterize(letter, size);

        let flipped = match letter {
            '.' => bitmap,
            _ => flip_vertical(&bitmap, metrics.width as usize, metrics.height as usize),
        };

        for (index, byte) in flipped.into_iter().enumerate() {
            let index = index as isize;

            let width = metrics.width as isize;
            let height = metrics.height as isize;

            let row = x + (index % width);
            let col = y + height - (index / width);
            let enabled = (byte as f32 / 255.0).round() as i32 == 1;
            self.set_pixel(col, row, enabled)
        }
    }

    fn render_centered(&mut self, text: String, font_size: f64, y: usize, font_path: Option<&str>) {
        let font = if let Some(font_path) = font_path {
            let font_bytes = fs::read(&font_path).unwrap();
            Font::from_bytes(font_bytes, fontdue::FontSettings::default()).unwrap()
        } else {
            Font::from_bytes(
                include_bytes!("../NANOTYPE.ttf") as &[u8],
                fontdue::FontSettings::default(),
            )
            .unwrap()
        };

        let mut width_needed = 0;

        text.chars().into_iter().for_each(|c| {
            let (metrics, _) = font.rasterize(c, font_size as f32);
            width_needed += metrics.width as usize + font_size as usize / 24;
        });

        self.draw_text(
            &text,
            ((128 - width_needed) / 2).try_into().unwrap(),
            y.try_into().unwrap(),
            font_size as f32,
            font_path,
            font_size as isize / 24,
        );
    }

    pub fn send(&mut self) -> Result<(), HidError> {
        let mut packets = self.to_packets();

        // Filter out packets for regions of the screen which haven't changed since last time
        if let Some(prev_packets) = &self._prev_packets {
            packets.retain(|packet| !prev_packets.contains(packet))
        };

        self._prev_packets = Some(self.to_packets());

        for packet in packets {
            packet.send(self.device.as_ref())?;
        }

        Ok(())
    }

    pub fn clear(&mut self) {
        self.data = vec![0; (self.width * self.height) / 8_usize];
    }

    pub fn fill_all(&mut self) {
        self.data = vec![1; (self.width * self.height) / 8_usize];
    }

    pub fn paint_region(
        &mut self,
        min_x: isize,
        min_y: isize,
        max_x: isize,
        max_y: isize,
        enabled: bool,
    ) {
        for x in min_x..max_x {
            for y in min_y..max_y {
                self.set_pixel(x, y, enabled)
            }
        }
    }

    pub fn get_pixel(&self, x: usize, y: usize) -> bool {
        let byte_index = (x + y * self.width) / 8;
        let bit_index: u8 = 7 - ((x % 8) as u8);

        let byte = self.data[byte_index];
        get_bit_at_index(byte, bit_index)
    }

    /// Underlying function for drawing to the canvas, if provided coordinates are out of range,
    /// this function will fail silently
    ///
    /// # Arguments
    /// * `x` - The x coordinate of the pixel to set
    /// * `y` - The y coordinate of the pixel to set
    /// * `enabled` - Whether to set the pixel to an enabled or disabled state (on/off)
    pub fn set_pixel(&mut self, x: isize, y: isize, enabled: bool) {
        if x >= self.width as isize || y >= self.height as isize || x < 0 || y < 0 {
            // If a pixel is rendered outside of the canvas, fail silently
            return;
        }

        let x = x as usize;
        let y = y as usize;

        let target_byte = (x / 8) * self.height + y;
        let target_bit: u8 = 7 - ((x % 8) as u8);

        self.data[target_byte] = set_bit_at_index(self.data[target_byte], target_bit, enabled);
    }
}

fn flip_vertical(bitmap: &Vec<u8>, width: usize, height: usize) -> Vec<u8> {
    let mut flipped = Vec::with_capacity(bitmap.len());

    // Assuming 1 byte per pixel here; adjust if your bitmap uses more bytes per pixel.
    let row_bytes = width;

    for y in (0..height).rev() {
        let row_start = y * row_bytes;
        let row_end = (y + 1) * row_bytes;
        flipped.extend_from_slice(&bitmap[row_start..row_end]);
    }

    flipped
}

pub fn get_bit_at_index(byte: u8, bit_index: u8) -> bool {
    let mask = 0b10000000 >> bit_index;

    mask & byte != 0
}

pub fn set_bit_at_index(byte: u8, bit_index: u8, enabled: bool) -> u8 {
    let mask = 0b10000000 >> bit_index;

    if enabled {
        mask | byte
    } else {
        (mask ^ 0b11111111) & byte
    }
}

fn main() {
    let api = HidApi::new().unwrap_or_else(|e| {
        eprintln!("Failed to initialize HID API: {}", e);
        std::process::exit(1);
    });

    let mut sys = System::new_all();

    // loop {
    //     sys.refresh_cpu();
    //     println!("cpu: {}%", sys.global_cpu_info().cpu_usage());
    //     std::thread::sleep(System::MINIMUM_CPU_UPDATE_INTERVAL);
    // }

    let device = api
        .device_list()
        .find(|device| is_my_device(device))
        .unwrap_or_else(|| {
            eprintln!("Failed to find device");
            std::process::exit(1);
        })
        .open_device(&api)
        .unwrap_or_else(|e| {
            eprintln!("Failed to open device: {}", e);
            std::process::exit(1);
        });

    let mut screen = Screen::from_device(device, 62, 128).unwrap();

    loop {
        sys.refresh_cpu();
        sys.refresh_memory();
        let cpu_usage = sys.global_cpu_info().cpu_usage();
        let current_ram = sys.used_memory();

        // clear bg
        screen.clear();

        screen.draw_time(SystemTime::now(), 64.0, None);

        // screen.draw_text("CPU:", 10, 10, 32.0, None);
        // screen.draw_text(
        //     &format!("{:.2}%", cpu_usage).to_string(),
        //     40,
        //     10,
        //     32.0,
        //     None,
        // );
        //
        // screen.draw_text("MEM:", 10, 24, 32.0, None);
        // screen.draw_text(
        //     &format!(
        //         "{:.2}/{:.2}GB",
        //         bytes_to_gb(current_ram),
        //         bytes_to_gb(total_ram)
        //     )
        //     .to_string(),
        //     40,
        //     24,
        //     32.0,
        //     None,
        // );

        let text = format!(
            "C    {:.1}%         M    {:.1} G",
            cpu_usage,
            bytes_to_gb(current_ram),
        );

        screen.render_centered(text, 32.0, 42, None);

        screen.send().unwrap();
        thread::sleep(System::MINIMUM_CPU_UPDATE_INTERVAL);
    }
}

fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / (1 << 30) as f64
}
