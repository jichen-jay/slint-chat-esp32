mod esp32;

slint::include_modules!();

use log::info;
use esp_idf_svc::wifi::ClientConfiguration;
use embedded_svc::http::client::Client;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};

type Wifi = esp_idf_svc::wifi::BlockingWifi<esp_idf_svc::wifi::EspWifi<'static>>;

pub struct Model {
    wifi: std::rc::Rc<std::cell::RefCell<Wifi>>,
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
        let model = Model { wifi };
        
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
        
        // Run the UI
        self.ui.run().map_err(|e| anyhow::anyhow!(e))
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Starting Slint Workshop ESP with ST7789 display");

    let platform = esp32::EspPlatform::new();
    let wifi = platform.wifi.clone();

    // Set the platform
    slint::platform::set_platform(platform).unwrap();

    info!("Platform initialized, creating app");

    let app = App::new(wifi)?;

    info!("App created, starting main loop with Slint UI");

    app.run()
}