use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
}

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub temperature: f64,
    pub humidity: f64,
    pub wind_speed: f64,
}

pub trait WifiNetworkProvider {
    fn scan_wifi_networks(&self) -> Vec<WifiNetwork>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wifi_network() {
        let network = WifiNetwork {
            ssid: "TestNetwork".to_string(),
        };
        assert_eq!(network.ssid, "TestNetwork");
    }

    #[test]
    fn test_weather_data() {
        let weather = WeatherData {
            temperature: 20.5,
            humidity: 65.0,
            wind_speed: 5.2,
        };
        assert_eq!(weather.temperature, 20.5);
        assert_eq!(weather.humidity, 65.0);
        assert_eq!(weather.wind_speed, 5.2);
    }
}