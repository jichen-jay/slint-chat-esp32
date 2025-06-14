mod esp32;

slint::include_modules!();

use log::info;
use esp_idf_svc::wifi::ClientConfiguration;

type Wifi = esp_idf_svc::wifi::BlockingWifi<esp_idf_svc::wifi::EspWifi<'static>>;

pub struct Model {
    wifi: std::rc::Rc<std::cell::RefCell<Wifi>>,
}

impl Model {
    fn scan_wifi_networks(&self) -> Vec<WifiNetwork> {
        self.wifi.borrow_mut().start().unwrap();
        info!("Wifi started");

        self.wifi
            .borrow_mut()
            .scan()
            .unwrap()
            .iter()
            .map(|access_point| WifiNetwork {
                ssid: slint::SharedString::from(access_point.ssid.to_string()),
            })
            .collect()
    }
    
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
        
        // Wait for connection
        self.wifi.borrow_mut().wait_netif_up()?;
        
        info!("WiFi connected!");
        Ok(())
    }
}

fn fetch_weather(lat: f64, lon: f64) -> Result<(f64, f64, f64), Box<dyn std::error::Error>> {
    info!("Fetching weather for coordinates: {}, {}", lat, lon);
    
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,wind_speed_10m",
        lat, lon
    );
    
    info!("Making request to: {}", url);
    
    // Use a simpler HTTP approach without TLS for now
    // The Open-Meteo API supports HTTP
    let url = url.replace("https://", "http://");
    
    // Parse URL components
    let (host, path) = if let Some(pos) = url.find('/') {
        let (h, p) = url.split_at(pos);
        (h.replace("http://", ""), p.to_string())
    } else {
        return Err("Invalid URL".into());
    };
    
    // Create a TCP connection
    use std::io::Write;
    use std::net::TcpStream;
    
    let mut stream = TcpStream::connect(format!("{}:80", host))?;
    
    // Send HTTP request
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: ESP32-Weather-App/1.0\r\nConnection: close\r\n\r\n",
        path, host
    );
    
    stream.write_all(request.as_bytes())?;
    
    // Read response
    use std::io::Read;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    
    // Find the JSON body (after the headers)
    let body_start = response.find("\r\n\r\n").ok_or("Invalid response")? + 4;
    let response_body = &response[body_start..];
    
    info!("Received response body: {}", response_body);
    
    // Parse JSON response
    use serde_json::Value;
    let v: Value = serde_json::from_str(response_body)?;
    
    let temp = v["current"]["temperature_2m"]
        .as_f64()
        .ok_or("Missing temperature data")?;
    
    let humidity = v["current"]["relative_humidity_2m"]
        .as_f64()
        .ok_or("Missing humidity data")?;
    
    let wind = v["current"]["wind_speed_10m"]
        .as_f64()
        .ok_or("Missing wind speed data")?;
    
    info!("Parsed weather data - Temp: {}°C, Humidity: {}%, Wind: {} m/s", temp, humidity, wind);
    
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
        let ui_weak_weather = ui_weak.clone(); // Clone for weather callback
        let model_rc = std::rc::Rc::new(self.model);
        
        // Set up WiFi scan callback
        let model_clone = model_rc.clone();
        self.ui.on_scan_wifi(move || {
            let networks = model_clone.scan_wifi_networks();
            let wifi_model = std::rc::Rc::new(slint::VecModel::from(networks));
            ui_weak.unwrap().set_wifi_networks(wifi_model.into());
        });
        
        // Set up weather fetch callback
        let model_clone = model_rc.clone();
        self.ui.on_fetch_weather(move || {
            // First ensure WiFi is connected
            if let Err(e) = model_clone.connect_to_wifi() {
                info!("WiFi connection error: {:?}", e);
                return;
            }
            
            // Kitchener, ON coordinates
            let lat = 43.4516;
            let lon = -80.4925;
            
            // Fetch weather
            match fetch_weather(lat, lon) {
                Ok((temp, humidity, wind)) => {
                    let weather_info = WeatherInfo {
                        temperature: temp as f32,
                        humidity: humidity as f32,
                        wind_speed: wind as f32,
                    };
                    ui_weak_weather.unwrap().set_weather(weather_info);
                }
                Err(e) => {
                    info!("Weather fetch error: {:?}", e);
                }
            }
        });
        
        // Run the UI
        self.ui.run().map_err(|e| anyhow::anyhow!(e))
    }
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
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