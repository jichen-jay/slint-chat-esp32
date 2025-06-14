use std::cell::RefCell;

extern crate alloc;

#[allow(unused_imports)]
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::hal::spi::*;
use esp_idf_svc::hal::gpio::*;
use esp_idf_svc::hal::delay::*;
use embedded_hal::delay::DelayNs;
use slint::PhysicalPosition;

// ST7789 Commands
const ST7789_SWRESET: u8 = 0x01;
const ST7789_SLPOUT: u8 = 0x11;
const ST7789_COLMOD: u8 = 0x3A;
const ST7789_MADCTL: u8 = 0x36;
const ST7789_CASET: u8 = 0x2A;
const ST7789_RASET: u8 = 0x2B;
const ST7789_RAMWR: u8 = 0x2C;
const ST7789_DISPON: u8 = 0x29;

pub struct EspPlatform {
    display_width: usize,
    display_height: usize,
    // SPI display components - wrapped in RefCell for interior mutability
    spi_device: std::cell::RefCell<esp_idf_svc::hal::spi::SpiDeviceDriver<'static, SpiDriver<'static>>>,
    dc_pin: std::cell::RefCell<PinDriver<'static, AnyOutputPin, Output>>,
    backlight_pin: PinDriver<'static, AnyOutputPin, Output>,
    window: alloc::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
    timer: esp_idf_svc::timer::EspTimerService<esp_idf_svc::timer::Task>,
    pub wifi: std::rc::Rc<
        std::cell::RefCell<esp_idf_svc::wifi::BlockingWifi<esp_idf_svc::wifi::EspWifi<'static>>>,
    >,
}

impl EspPlatform {
    /// Create a new instance of the platform with ST7789 SPI display
    pub fn new() -> std::boxed::Box<Self> {
        use esp_idf_svc::hal::prelude::*;

        use esp_idf_svc::hal::prelude::*;

        let peripherals = Peripherals::take().unwrap();

        // Initialize SPI for ST7789 display
        // ESP32 pin mapping for LilyGo Camera Plus
        let spi = SpiDriver::new(
            peripherals.spi2,
            peripherals.pins.gpio21, // SCLK
            peripherals.pins.gpio19, // MOSI  
            Some(peripherals.pins.gpio18), // MISO (not used but required - using GPIO18)
            &SpiDriverConfig::new()
        ).unwrap();

        let cs_pin = peripherals.pins.gpio12;
        let dc_pin = PinDriver::output(peripherals.pins.gpio15.downgrade_output()).unwrap();
        let mut backlight_pin = PinDriver::output(peripherals.pins.gpio2.downgrade_output()).unwrap();

        // Turn on backlight
        backlight_pin.set_high().unwrap();

        let spi_config = config::Config::new()
            .baudrate(10.MHz().into()) // Reduced from 40MHz to 10MHz for ESP32 compatibility
            .data_mode(embedded_hal::spi::MODE_0);

        let spi_device = SpiDeviceDriver::new(spi, Some(cs_pin), &spi_config).unwrap();

        // Initialize ST7789 display - we need to do this after creating RefCells
        log::info!("Creating SPI device completed, initializing ST7789...");

        // Wrap SPI device and DC pin in RefCell for interior mutability
        let spi_device = std::cell::RefCell::new(spi_device);
        let dc_pin_cell = std::cell::RefCell::new(dc_pin);

        // Now initialize the display using the RefCell wrapped components
        {
            let mut spi_dev = spi_device.borrow_mut();
            let mut dc = dc_pin_cell.borrow_mut();
            Self::init_st7789(&mut *spi_dev, &mut *dc).unwrap();
        }

        // Remove all touch-related code since we don't need it
        log::info!("Skipping touch controller - display only mode");

        // Setup the window for 240x240 ST7789
        let display_width = 240;
        let display_height = 240;

        let window = slint::platform::software_renderer::MinimalSoftwareWindow::new(Default::default());
        window.set_size(slint::PhysicalSize::new(display_width as u32, display_height as u32));

        // Set scale factor for small display
        window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
            scale_factor: 1.0,
        });
        window.dispatch_event(slint::platform::WindowEvent::Resized {
            size: window.size().to_logical(1.0),
        });

        // Initialize WiFi
        let sys_loop = esp_idf_svc::eventloop::EspSystemEventLoop::take().unwrap();
        let nvs = esp_idf_svc::nvs::EspDefaultNvsPartition::take().unwrap();

        let wifi = std::rc::Rc::new(std::cell::RefCell::new(
            esp_idf_svc::wifi::BlockingWifi::wrap(
                esp_idf_svc::wifi::EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))
                    .unwrap(),
                sys_loop,
            ).unwrap(),
        ));

        std::boxed::Box::new(Self {
            display_width,
            display_height,
            spi_device,
            dc_pin: dc_pin_cell,
            backlight_pin,
            window,
            timer: esp_idf_svc::timer::EspTimerService::new().unwrap(),
            wifi,
        })
    }

    fn init_st7789(
        spi: &mut SpiDeviceDriver<'static, SpiDriver<'static>>,
        dc_pin: &mut PinDriver<'static, AnyOutputPin, Output>,
    ) -> Result<(), esp_idf_svc::sys::EspError> {
        let mut delay = FreeRtos {};

        log::info!("Starting ST7789 initialization...");

        // Hardware reset (if reset pin is available, implement here)
        delay.delay_ms(120_u32);

        // Software reset
        log::info!("Sending software reset...");
        Self::write_command(spi, dc_pin, ST7789_SWRESET, &[])?;
        delay.delay_ms(120_u32);

        // Sleep out
        log::info!("Sending sleep out...");
        Self::write_command(spi, dc_pin, ST7789_SLPOUT, &[])?;
        delay.delay_ms(120_u32);

        // Color mode: 16-bit RGB565
        log::info!("Setting color mode...");
        Self::write_command(spi, dc_pin, ST7789_COLMOD, &[0x05])?;

        // Memory access control (adjust rotation/mirroring as needed)
        log::info!("Setting memory access control...");
        Self::write_command(spi, dc_pin, ST7789_MADCTL, &[0x00])?;

        // Column address set (0 to 239)
        log::info!("Setting column address...");
        Self::write_command(spi, dc_pin, ST7789_CASET, &[0x00, 0x00, 0x00, 0xEF])?;

        // Row address set (0 to 239)  
        log::info!("Setting row address...");
        Self::write_command(spi, dc_pin, ST7789_RASET, &[0x00, 0x00, 0x00, 0xEF])?;

        // Display on
        log::info!("Turning display on...");
        Self::write_command(spi, dc_pin, ST7789_DISPON, &[])?;
        delay.delay_ms(120_u32);

        log::info!("ST7789 display initialized successfully");
        Ok(())
    }

    fn write_command(
        spi: &mut SpiDeviceDriver<'static, SpiDriver<'static>>,
        dc_pin: &mut PinDriver<'static, AnyOutputPin, Output>,
        command: u8,
        data: &[u8],
    ) -> Result<(), esp_idf_svc::sys::EspError> {
        // DC low for command
        let _ = dc_pin.set_low();
        spi.write(&[command])?;

        if !data.is_empty() {
            // DC high for data
            let _ = dc_pin.set_high();
            spi.write(data)?;
        }

        Ok(())
    }

    fn set_address_window(&self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<(), esp_idf_svc::sys::EspError> {
        let mut spi_device = self.spi_device.borrow_mut();
        let mut dc_pin = self.dc_pin.borrow_mut();
        
        // Column address set
        Self::write_command(&mut *spi_device, &mut *dc_pin, ST7789_CASET, &[
            (x0 >> 8) as u8, (x0 & 0xFF) as u8,
            (x1 >> 8) as u8, (x1 & 0xFF) as u8,
        ])?;

        // Row address set
        Self::write_command(&mut *spi_device, &mut *dc_pin, ST7789_RASET, &[
            (y0 >> 8) as u8, (y0 & 0xFF) as u8,
            (y1 >> 8) as u8, (y1 & 0xFF) as u8,
        ])?;

        // Memory write
        let _ = dc_pin.set_low();
        spi_device.write(&[ST7789_RAMWR])?;
        let _ = dc_pin.set_high();

        Ok(())
    }

    /// Fill the entire display with a single color (for testing)
    pub fn fill_screen(&self, color: u16) -> Result<(), esp_idf_svc::sys::EspError> {
        log::info!("Filling screen with color: 0x{:04X}", color);
        
        // Set address window to entire screen
        self.set_address_window(0, 0, (self.display_width - 1) as u16, (self.display_height - 1) as u16)?;
        
        // Create pixel data array
        let pixel_count = self.display_width * self.display_height;
        let mut pixel_data = Vec::with_capacity(pixel_count * 2);
        
        for _ in 0..pixel_count {
            pixel_data.push((color >> 8) as u8);
            pixel_data.push((color & 0xFF) as u8);
        }
        
        // Send to display
        let mut spi_device = self.spi_device.borrow_mut();
        spi_device.write(&pixel_data)?;
        
        log::info!("Screen filled successfully");
        Ok(())
    }
}

impl slint::platform::Platform for EspPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<alloc::rc::Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.timer.now()
    }

fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
    // Create a buffer to draw the scene
    use slint::platform::software_renderer::Rgb565Pixel;
    let mut buffer = Vec::new();
    buffer.resize(self.display_width * self.display_height, Rgb565Pixel(0x0));

    log::info!("Starting main event loop...");
    
    let mut last_yield = std::time::Instant::now();

    loop {
        slint::platform::update_timers_and_animations();

        // Draw the scene if something needs to be drawn
        self.window.draw_if_needed(|renderer| {
            // Render to buffer
            let region = renderer.render(&mut buffer, self.display_width);

            // Send buffer to ST7789 display
            for (origin, size) in region.iter() {
                if let Err(e) = self.update_display_region(&origin, &size, &buffer) {
                    log::error!("Failed to update display: {:?}", e);
                }
            }
        });

        // Yield regularly to prevent watchdog timeout
        if last_yield.elapsed() > std::time::Duration::from_millis(100) {
            esp_idf_svc::hal::task::do_yield();
            last_yield = std::time::Instant::now();
        }
        
        // Sleep when idle to save CPU
        if !self.window.has_active_animations() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
}

impl EspPlatform {
    fn update_display_region(
        &self,
        origin: &PhysicalPosition,
        size: &slint::PhysicalSize,
        buffer: &[slint::platform::software_renderer::Rgb565Pixel],
    ) -> Result<(), esp_idf_svc::sys::EspError> {
        let x0 = origin.x as u16;
        let y0 = origin.y as u16;
        let x1 = (origin.x + size.width as i32 - 1) as u16;
        let y1 = (origin.y + size.height as i32 - 1) as u16;

        // Set address window
        self.set_address_window(x0, y0, x1, y1)?;
        
        // Convert buffer region to byte array and send to display
        let start_idx = (origin.y * self.display_width as i32 + origin.x) as usize;
        let mut pixel_data = Vec::new();
        
        for row in 0..size.height {
            let row_start = start_idx + (row as usize * self.display_width);
            let row_end = row_start + size.width as usize;
            
            if row_end <= buffer.len() {
                for pixel in &buffer[row_start..row_end] {
                    let rgb565 = pixel.0;
                    pixel_data.push((rgb565 >> 8) as u8);
                    pixel_data.push((rgb565 & 0xFF) as u8);
                }
            }
        }
        
        // Send pixel data to display
        if !pixel_data.is_empty() {
            let mut spi_device = self.spi_device.borrow_mut();
            spi_device.write(&pixel_data)?;
        }
        
        Ok(())
    }
}