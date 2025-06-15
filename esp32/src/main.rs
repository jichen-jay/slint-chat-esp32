mod esp32;

slint::include_modules!();
use esp_idf_svc::sys::configTICK_RATE_HZ;
use log::info;
use esp_idf_svc::wifi::ClientConfiguration;
use embedded_svc::http::client::Client;
use esp_idf_svc::io::Read;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use std::fs::File;
use std::io::Write;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::hal::spi::*;
use esp_idf_svc::sd::spi::*;
use esp_idf_svc::sd::*;

type Wifi = esp_idf_svc::wifi::BlockingWifi<esp_idf_svc::wifi::EspWifi<'static>>;

pub struct Model {
    wifi: std::rc::Rc<std::cell::RefCell<Wifi>>,
    audio_recorder: std::rc::Rc<std::cell::RefCell<Option<AudioRecorder>>>,
}

pub struct AudioRecorder {
    sd_mounted: bool,
}

impl AudioRecorder {

fn new(peripherals: &mut esp_idf_svc::hal::peripherals::Peripherals) -> anyhow::Result<Self> {
        info!("Initializing audio recorder...");
        
        let sd_mounted = Self::init_sd_card_hal(peripherals)?;
        let i2s_initialized = Self::init_i2s()?;
        
        if !i2s_initialized {
            info!("Failed to initialize I2S, audio recording will be disabled");
        }
        
        info!("Audio recorder initialized successfully - SD: {}, I2S: {}", sd_mounted, i2s_initialized);
        
        Ok(Self {
            sd_mounted: sd_mounted && i2s_initialized,
        })
    }

    


fn init_sd_card_hal(peripherals: &mut esp_idf_svc::hal::peripherals::Peripherals) -> anyhow::Result<bool> {
    info!("Initializing SD card using HAL...");
    
    // Use high-level SPI driver
    let spi_driver = SpiDriver::new(
        peripherals.spi2.take().unwrap(),
        19, // MOSI
        22, // MISO  
        21, // SCLK
        &DriverConfig::default(),
    )?;
    
    // Create SD SPI device
    let sd_device = SpiDevice::new(
        spi_driver,
        0, // CS pin
        Option::<esp_idf_svc::hal::gpio::AnyInputPin>::None, // CD
        Option::<esp_idf_svc::hal::gpio::AnyInputPin>::None, // WP
        Option::<esp_idf_svc::hal::gpio::AnyInputPin>::None, // INT
    );
    
    // Use default SD configuration
    let config = SdConfiguration::new();
    
    // Mount the SD card
    match SdCard::mount(config, sd_device) {
        Ok(_card) => {
            info!("SD card mounted successfully using HAL");
            Ok(true)
        }
        Err(e) => {
            info!("SD card mount failed: {:?}", e);
            Ok(false)
        }
    }
}

    
    fn init_i2s() -> anyhow::Result<bool> {
        info!("Initializing I2S for microphone...");
        
        unsafe {
            let mut i2s_config: esp_idf_svc::sys::i2s_config_t = std::mem::zeroed();
            i2s_config.mode = esp_idf_svc::sys::i2s_mode_t_I2S_MODE_MASTER 
                | esp_idf_svc::sys::i2s_mode_t_I2S_MODE_RX;
            i2s_config.sample_rate = 16000;
            i2s_config.bits_per_sample = esp_idf_svc::sys::i2s_bits_per_sample_t_I2S_BITS_PER_SAMPLE_16BIT;
            i2s_config.channel_format = esp_idf_svc::sys::i2s_channel_fmt_t_I2S_CHANNEL_FMT_ONLY_LEFT;
            i2s_config.communication_format = esp_idf_svc::sys::i2s_comm_format_t_I2S_COMM_FORMAT_STAND_I2S;
            i2s_config.intr_alloc_flags = 0;
            
            // Fixed I2S configuration field access
            i2s_config.__bindgen_anon_1.dma_buf_count = 8;
            i2s_config.__bindgen_anon_1.dma_buf_len = 1024;
            
            i2s_config.use_apll = false;
            i2s_config.tx_desc_auto_clear = false;
            i2s_config.fixed_mclk = 0;
            i2s_config.mclk_multiple = esp_idf_svc::sys::i2s_mclk_multiple_t_I2S_MCLK_MULTIPLE_256;
            i2s_config.bits_per_chan = esp_idf_svc::sys::i2s_bits_per_chan_t_I2S_BITS_PER_CHAN_DEFAULT;
            
            let pin_config = esp_idf_svc::sys::i2s_pin_config_t {
                bck_io_num: 14,
                ws_io_num: 32,
                data_out_num: esp_idf_svc::sys::I2S_PIN_NO_CHANGE,
                data_in_num: 33,
                mck_io_num: esp_idf_svc::sys::I2S_PIN_NO_CHANGE,
            };
            
            let ret = esp_idf_svc::sys::i2s_driver_install(
                esp_idf_svc::sys::i2s_port_t_I2S_NUM_1,
                &i2s_config,
                0,
                std::ptr::null_mut(),
            );
            
            if ret != esp_idf_svc::sys::ESP_OK {
                info!("I2S driver install failed: {}", ret);
                return Ok(false);
            }
            
            let ret = esp_idf_svc::sys::i2s_set_pin(
                esp_idf_svc::sys::i2s_port_t_I2S_NUM_1,
                &pin_config,
            );
            
            if ret != esp_idf_svc::sys::ESP_OK {
                info!("I2S pin config failed: {}", ret);
                info!("Continuing despite pin config failure...");
            }
            
            info!("I2S initialized successfully");
            Ok(true)
        }
    }
    
    fn record_audio(&mut self) -> anyhow::Result<()> {
        if !self.sd_mounted {
            info!("SD card not available, simulating recording (no save)");
        } else {
            info!("Starting 10-second audio recording to SD card...");
        }
        
        let sample_rate = 16000;
        let duration_seconds = 10;
        let bytes_per_sample = 2;
        let total_samples = sample_rate * duration_seconds;
        let total_bytes = total_samples * bytes_per_sample;
        
        let mut audio_buffer = Vec::with_capacity(total_bytes);
        let mut temp_buffer = [0u8; 2048];
        
        info!("Recording {} samples ({} bytes)...", total_samples, total_bytes);
        
        let start_time = std::time::Instant::now();
        while audio_buffer.len() < total_bytes && start_time.elapsed().as_secs() < 10 {
            let mut bytes_read = 0;
            
            unsafe {
                let ret = esp_idf_svc::sys::i2s_read(
                    esp_idf_svc::sys::i2s_port_t_I2S_NUM_1,
                    temp_buffer.as_mut_ptr() as *mut std::ffi::c_void,
                    temp_buffer.len(),
                    &mut bytes_read,
                    (1000 * configTICK_RATE_HZ) / 1000,
                );
                
                if ret == esp_idf_svc::sys::ESP_OK && bytes_read > 0 {
                    let remaining_bytes = total_bytes - audio_buffer.len();
                    let bytes_to_copy = bytes_read.min(remaining_bytes);
                    audio_buffer.extend_from_slice(&temp_buffer[..bytes_to_copy]);
                    
                    if audio_buffer.len() % (sample_rate * bytes_per_sample) == 0 {
                        let seconds_recorded = audio_buffer.len() / (sample_rate * bytes_per_sample);
                        info!("Recorded {} seconds...", seconds_recorded);
                    }
                } else {
                    info!("I2S read error or timeout: {}, bytes_read: {}", ret, bytes_read);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            
            esp_idf_svc::hal::task::do_yield();
        }
        
        info!("Recorded {} bytes in {:?}", audio_buffer.len(), start_time.elapsed());
        
        if self.sd_mounted {
            let filename = format!("/sdcard/rec_{}.wav", 
                                  std::time::SystemTime::now()
                                      .duration_since(std::time::UNIX_EPOCH)
                                      .unwrap_or_default()
                                      .as_secs());
            
            self.save_wav_file(&filename, &audio_buffer, sample_rate as u32)?;
            info!("Audio saved to: {}", filename);
        } else {
            info!("Audio recording completed (not saved - no SD card)");
        }
        
        Ok(())
    }
    
    fn save_wav_file(&self, filename: &str, audio_data: &[u8], sample_rate: u32) -> anyhow::Result<()> {
        let mut file = File::create(filename)?;
        
        let data_size = audio_data.len() as u32;
        let file_size = 36 + data_size;
        
        // RIFF header
        file.write_all(b"RIFF")?;
        file.write_all(&file_size.to_le_bytes())?;
        file.write_all(b"WAVE")?;
        
        // fmt chunk
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&sample_rate.to_le_bytes())?;
        file.write_all(&(sample_rate * 2).to_le_bytes())?;
        file.write_all(&2u16.to_le_bytes())?;
        file.write_all(&16u16.to_le_bytes())?;
        
        // data chunk
        file.write_all(b"data")?;
        file.write_all(&data_size.to_le_bytes())?;
        file.write_all(audio_data)?;
        
        file.flush()?;
        Ok(())
    }
}

impl Model {
    fn connect_to_wifi(&self) -> anyhow::Result<()> {
        info!("Connecting to WiFi...");
        
        let wifi_config = ClientConfiguration {
            ssid: "WuandChen".try_into().unwrap(),
            password: "Sunday228!".try_into().unwrap(),
            ..Default::default()
        };
        
        self.wifi.borrow_mut().set_configuration(&esp_idf_svc::wifi::Configuration::Client(wifi_config))?;
        self.wifi.borrow_mut().start()?;
        self.wifi.borrow_mut().connect()?;
        
        let start = std::time::Instant::now();
        while !self.wifi.borrow().is_connected()? {
            if start.elapsed() > std::time::Duration::from_secs(10) {
                return Err(anyhow::anyhow!("WiFi connection timeout"));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        
        self.wifi.borrow_mut().wait_netif_up()?;
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        info!("WiFi connected!");
        Ok(())
    }
    
    fn start_audio_recording(&self) -> anyhow::Result<()> {
        if let Some(recorder) = self.audio_recorder.borrow_mut().as_mut() {
            recorder.record_audio()?;
        } else {
            info!("Audio recorder not available");
        }
        Ok(())
    }
}

fn fetch_weather_simple() -> Result<(f64, f64, f64), Box<dyn std::error::Error>> {
    info!("Fetching weather data...");
    
    let config = HttpConfig {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };
    
    let connection = EspHttpConnection::new(&config)?;
    let mut client = Client::wrap(connection);

    let url = "http://api.open-meteo.com/v1/forecast?latitude=43.45&longitude=-80.49&current=temperature_2m,relative_humidity_2m,wind_speed_10m";
    
    info!("Making http request...");
    let request = client.get(url)?;
    let mut response = request.submit()?;
    
    let status = response.status();
    info!("Response status: {}", status);
    
    if status != 200 {
        return Err(format!("HTTP error: {}", status).into());
    }
    
    let mut buf = Vec::new();
    let mut temp_buf = [0u8; 256];
    
    loop {
        match response.read(&mut temp_buf) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&temp_buf[..n]),
            Err(e) => return Err(Box::new(e)),
        }
    }
    
    let response_str = String::from_utf8_lossy(&buf);
    info!("Response: {}", response_str);
    
    use serde_json::Value;
    let v: Value = serde_json::from_str(&response_str)?;
    
    let temp = v["current"]["temperature_2m"].as_f64().ok_or("Missing temperature")?;
    let humidity = v["current"]["relative_humidity_2m"].as_f64().ok_or("Missing humidity")?;
    let wind = v["current"]["wind_speed_10m"].as_f64().ok_or("Missing wind speed")?;
    
    info!("Weather: {}°C, {}%, {} m/s", temp, humidity, wind);
    
    Ok((temp, humidity, wind))
}

struct App {
    ui: MainWindow,
    model: Model,
}

impl App {
    fn new(wifi: std::rc::Rc<std::cell::RefCell<Wifi>>) -> anyhow::Result<Self> {
        let ui = MainWindow::new().map_err(|e| anyhow::anyhow!(e))?;
        
        let audio_recorder = match AudioRecorder::new() {
            Ok(recorder) => Some(recorder),
            Err(e) => {
                info!("Failed to initialize audio recorder: {:?}", e);
                None
            }
        };
        
        let model = Model { 
            wifi,
            audio_recorder: std::rc::Rc::new(std::cell::RefCell::new(audio_recorder)),
        };
        
        Ok(Self { ui, model })
    }

    fn run(self) -> anyhow::Result<()> {
        let ui_weak = self.ui.as_weak();
        let model_rc = std::rc::Rc::new(self.model);
        
        info!("Connecting to WiFi at startup...");
        let wifi_connected = match model_rc.connect_to_wifi() {
            Ok(_) => {
                info!("WiFi connected successfully!");
                true
            }
            Err(e) => {
                info!("WiFi connection failed: {:?}", e);
                false
            }
        };
        
        if wifi_connected {
            match fetch_weather_simple() {
                Ok((temp, humidity, wind)) => {
                    let weather_info = WeatherInfo {
                        temperature: temp as f32,
                        humidity: humidity as f32,
                        wind_speed: wind as f32,
                    };
                    self.ui.set_weather(weather_info);
                    info!("Initial weather data loaded");
                }
                Err(e) => {
                    info!("Initial weather fetch failed: {:?}", e);
                }
            }
        }
        
        let ui_weak_weather = ui_weak.clone();
        let weather_timer = slint::Timer::default();
        
        weather_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(30),
            move || {
                info!("Timer triggered - fetching weather...");
                match fetch_weather_simple() {
                    Ok((temp, humidity, wind)) => {
                        let weather_info = WeatherInfo {
                            temperature: temp as f32,
                            humidity: humidity as f32,
                            wind_speed: wind as f32,
                        };
                        ui_weak_weather.unwrap().set_weather(weather_info);
                        info!("Weather updated via timer");
                    }
                    Err(e) => {
                        info!("Weather fetch error: {:?}", e);
                    }
                }
            },
        );
        
        let model_audio = model_rc.clone();
        let audio_timer = slint::Timer::default();
        
        audio_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(60),
            move || {
                info!("Audio timer triggered - starting recording...");
                if let Err(e) = model_audio.start_audio_recording() {
                    info!("Audio recording failed: {:?}", e);
                }
            },
        );
        
        let model_first_audio = model_rc.clone();
        let first_audio_timer = slint::Timer::default();
        first_audio_timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_secs(5),
            move || {
                info!("Starting first audio recording...");
                if let Err(e) = model_first_audio.start_audio_recording() {
                    info!("First audio recording failed: {:?}", e);
                }
            },
        );
        
        self.ui.run().map_err(|e| anyhow::anyhow!(e))
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Starting Slint Workshop ESP with ST7789 display and audio recording");

    let platform = esp32::EspPlatform::new();
    let wifi = platform.wifi.clone();

    slint::platform::set_platform(platform).unwrap();

    info!("Platform initialized, creating app");

    let app = App::new(wifi)?;

    info!("App created, starting main loop with Slint UI and audio recording");

    app.run()
}
