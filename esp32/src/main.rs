mod esp32;

slint::include_modules!();

use log::info;
use esp_idf_svc::wifi::ClientConfiguration;
use embedded_svc::http::client::Client;
use esp_idf_svc::io::Read;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use std::fs::File;
use std::io::Write;

type Wifi = esp_idf_svc::wifi::BlockingWifi<esp_idf_svc::wifi::EspWifi<'static>>;

pub struct Model {
    wifi: std::rc::Rc<std::cell::RefCell<Wifi>>,
    audio_recorder: std::rc::Rc<std::cell::RefCell<Option<AudioRecorder>>>,
}

pub struct AudioRecorder {
    sd_mounted: bool,
}

impl AudioRecorder {
    fn new() -> anyhow::Result<Self> {
        info!("Initializing audio recorder...");
        
        // Initialize SD card and I2S using C API for better compatibility
        let sd_mounted = Self::init_sd_card()?;
        let i2s_initialized = Self::init_i2s()?;
        
        if !i2s_initialized {
            info!("Failed to initialize I2S, audio recording will be disabled");
        }
        
        info!("Audio recorder initialized successfully");
        
        Ok(Self {
            sd_mounted: sd_mounted && i2s_initialized,
        })
    }
    
    fn init_sd_card() -> anyhow::Result<bool> {
        info!("Initializing SD card...");
        
        unsafe {
            // Use ESP-IDF C API for SD card initialization
            let mount_config = esp_idf_svc::sys::esp_vfs_fat_mount_config_t {
                format_if_mount_failed: true,
                max_files: 5,
                allocation_unit_size: 0,
                disk_status_check_enable: false,
                use_one_fat: false,
            };
            
            let base_path = std::ffi::CString::new("/sdcard").unwrap();
            
            // Try to mount SD card via SPI
            let mut card: *mut esp_idf_svc::sys::sdmmc_card_t = std::ptr::null_mut();
            let ret = esp_idf_svc::sys::esp_vfs_fat_sdspi_mount(
                base_path.as_ptr(),
                std::ptr::null(), // Use default SPI host
                std::ptr::null(), // Use default slot config  
                &mount_config,
                &mut card,
            );
            
            if ret == esp_idf_svc::sys::ESP_OK {
                info!("SD card mounted successfully");
                return Ok(true);
            } else {
                info!("SD card mount failed with error: {}", ret);
                return Ok(false);
            }
        }
    }
    
    fn init_i2s() -> anyhow::Result<bool> {
        info!("Initializing I2S for microphone...");
        
        unsafe {
            // Configure I2S for recording from MSM261S4030H0 microphone
            let i2s_config = esp_idf_svc::sys::i2s_config_t {
                mode: esp_idf_svc::sys::i2s_mode_t_I2S_MODE_MASTER 
                    | esp_idf_svc::sys::i2s_mode_t_I2S_MODE_RX,
                sample_rate: 16000,
                bits_per_sample: esp_idf_svc::sys::i2s_bits_per_sample_t_I2S_BITS_PER_SAMPLE_16BIT,
                channel_format: esp_idf_svc::sys::i2s_channel_fmt_t_I2S_CHANNEL_FMT_ONLY_LEFT,
                communication_format: esp_idf_svc::sys::i2s_comm_format_t_I2S_COMM_FORMAT_STAND_I2S,
                intr_alloc_flags: 0,
                dma_buf_count: 4,
                dma_buf_len: 1024,
                use_apll: false,
                tx_desc_auto_clear: false,
                fixed_mclk: 0,
                mclk_multiple: esp_idf_svc::sys::i2s_mclk_multiple_t_I2S_MCLK_MULTIPLE_256,
                bits_per_chan: esp_idf_svc::sys::i2s_bits_per_chan_t_I2S_BITS_PER_CHAN_DEFAULT,
            };
            
            // Pin configuration based on schematic
            let pin_config = esp_idf_svc::sys::i2s_pin_config_t {
                bck_io_num: 14,  // MIC_14 (SCK)
                ws_io_num: 32,   // MIC_32 (WS)  
                data_out_num: esp_idf_svc::sys::I2S_PIN_NO_CHANGE,
                data_in_num: 33, // MIC_33 (SD)
            };
            
            // Install and start I2S driver
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
                return Ok(false);
            }
            
            info!("I2S initialized successfully");
            Ok(true)
        }
    }
    
    fn record_audio(&mut self) -> anyhow::Result<()> {
        if !self.sd_mounted {
            info!("SD card not available, skipping recording");
            return Ok(());
        }
        
        info!("Starting 10-second audio recording...");
        
        // Calculate buffer size for 10 seconds at 16kHz, 16-bit, mono
        let sample_rate = 16000;
        let duration_seconds = 10;
        let bytes_per_sample = 2; // 16-bit = 2 bytes
        let total_samples = sample_rate * duration_seconds;
        let total_bytes = total_samples * bytes_per_sample;
        
        let mut audio_buffer = Vec::with_capacity(total_bytes);
        let mut temp_buffer = [0u8; 2048];
        
        info!("Recording {} samples ({} bytes)...", total_samples, total_bytes);
        
        // Record audio data using I2S C API
        let start_time = std::time::Instant::now();
        while audio_buffer.len() < total_bytes && start_time.elapsed().as_secs() < 10 {
            let mut bytes_read = 0;
            
            unsafe {
                let ret = esp_idf_svc::sys::i2s_read(
                    esp_idf_svc::sys::i2s_port_t_I2S_NUM_1,
                    temp_buffer.as_mut_ptr() as *mut std::ffi::c_void,
                    temp_buffer.len(),
                    &mut bytes_read,
                    1000 / esp_idf_svc::sys::portTICK_PERIOD_MS, // 1 second timeout
                );
                
                if ret == esp_idf_svc::sys::ESP_OK && bytes_read > 0 {
                    let remaining_bytes = total_bytes - audio_buffer.len();
                    let bytes_to_copy = bytes_read.min(remaining_bytes);
                    audio_buffer.extend_from_slice(&temp_buffer[..bytes_to_copy]);
                } else {
                    info!("I2S read error or timeout: {}", ret);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
        
        info!("Recorded {} bytes in {:?}", audio_buffer.len(), start_time.elapsed());
        
        // Generate filename with timestamp
        let filename = format!("/sdcard/rec_{}.wav", 
                              std::time::SystemTime::now()
                                  .duration_since(std::time::UNIX_EPOCH)
                                  .unwrap_or_default()
                                  .as_secs());
        
        // Save as WAV file
        self.save_wav_file(&filename, &audio_buffer, sample_rate as u32)?;
        
        info!("Audio saved to: {}", filename);
        Ok(())
    }
    
    fn save_wav_file(&self, filename: &str, audio_data: &[u8], sample_rate: u32) -> anyhow::Result<()> {
        let mut file = File::create(filename)?;
        
        // WAV file header
        let data_size = audio_data.len() as u32;
        let file_size = 36 + data_size;
        
        // RIFF header
        file.write_all(b"RIFF")?;
        file.write_all(&file_size.to_le_bytes())?;
        file.write_all(b"WAVE")?;
        
        // fmt chunk
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?; // chunk size
        file.write_all(&1u16.to_le_bytes())?;  // PCM format
        file.write_all(&1u16.to_le_bytes())?;  // mono
        file.write_all(&sample_rate.to_le_bytes())?; // sample rate
        file.write_all(&(sample_rate * 2).to_le_bytes())?; // byte rate
        file.write_all(&2u16.to_le_bytes())?;  // block align
        file.write_all(&16u16.to_le_bytes())?; // bits per sample
        
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
        
        // Configure your WiFi credentials here
        let wifi_config = ClientConfiguration {
            ssid: "WuandChen".try_into().unwrap(),
            password: "Sunday228!".try_into().unwrap(),
            ..Default::default()
        };
        
        self.wifi.borrow_mut().set_configuration(&esp_idf_svc::wifi::Configuration::Client(wifi_config))?;
        self.wifi.borrow_mut().start()?;
        self.wifi.borrow_mut().connect()?;
        
        // Wait for connection with timeout
        let start = std::time::Instant::now();
        while !self.wifi.borrow().is_connected()? {
            if start.elapsed() > std::time::Duration::from_secs(10) {
                return Err(anyhow::anyhow!("WiFi connection timeout"));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        
        // Wait for IP
        self.wifi.borrow_mut().wait_netif_up()?;
        
        // Wait a bit for network to stabilize
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
    
    // Use a simple HTTP client
    let config = HttpConfig {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };
    
    let connection = EspHttpConnection::new(&config)?;
    let mut client = Client::wrap(connection);

    // Use the exact URL that works
    let url = "http://api.open-meteo.com/v1/forecast?latitude=43.45&longitude=-80.49&current=temperature_2m,relative_humidity_2m,wind_speed_10m";
    
    info!("Making http request...");
   let request = client.get(url)?;
    
    let mut response = request.submit()?;
    
    let status = response.status();
    info!("Response status: {}", status);
    
    if status != 200 {
        return Err(format!("HTTP error: {}", status).into());
    }
    use embedded_svc::io::Read; 
    // Read response
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
    
    // Parse JSON
    use serde_json::Value;
    let v: Value = serde_json::from_str(&response_str)?;
    
    let temp = v["current"]["temperature_2m"]
        .as_f64()
        .ok_or("Missing temperature")?;
    
    let humidity = v["current"]["relative_humidity_2m"]
        .as_f64()
        .ok_or("Missing humidity")?;
    
    let wind = v["current"]["wind_speed_10m"]
        .as_f64()
        .ok_or("Missing wind speed")?;
    
    info!("Weather: {}°C, {}%, {} m/s", temp, humidity, wind);
    
    Ok((temp, humidity, wind))
}

/// Our App struct that holds the UI
struct App {
    ui: MainWindow,
    model: Model,
}

impl App {
    /// Create a new App struct.
    fn new(wifi: std::rc::Rc<std::cell::RefCell<Wifi>>) -> anyhow::Result<Self> {
        let ui = MainWindow::new().map_err(|e| anyhow::anyhow!(e))?;
        
        // Initialize audio recorder
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

    /// Run the App
    fn run(self) -> anyhow::Result<()> {
        let ui_weak = self.ui.as_weak();
        let model_rc = std::rc::Rc::new(self.model);
        
        // Connect to WiFi at startup
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
            // Try to fetch weather immediately
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
        
        // Set up periodic weather updates
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
        
        // Set up periodic audio recording (every 60 seconds)
        let model_audio = model_rc.clone();
        let audio_timer = slint::Timer::default();
        
        audio_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(60), // Record every minute
            move || {
                info!("Audio timer triggered - starting recording...");
                if let Err(e) = model_audio.start_audio_recording() {
                    info!("Audio recording failed: {:?}", e);
                }
            },
        );
        
        // Start first recording after 5 seconds
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
        
        // Run the UI
        self.ui.run().map_err(|e| anyhow::anyhow!(e))
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Starting Slint Workshop ESP with ST7789 display and audio recording");

    let platform = esp32::EspPlatform::new();
    let wifi = platform.wifi.clone();

    // Set the platform
    slint::platform::set_platform(platform).unwrap();

    info!("Platform initialized, creating app");

    let app = App::new(wifi)?;

    info!("App created, starting main loop with Slint UI and audio recording");

    app.run()
}