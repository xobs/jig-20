use std::time::Duration;

pub struct Config {
    locale: Option<String>,
    timeout: Duration,
    scenario_timeout: Duration,
}

impl Config {
    pub fn new() -> Config {
        Config {
            locale: None,
            timeout: Duration::from_secs(3600),
            scenario_timeout: Duration::from_secs(7200),
        }
    }

    pub fn set_locale(&mut self, locale: Option<&str>) {
        self.locale = match locale {
            None => None,
            Some(s) => Some(s.to_string()),
        };
    }

    pub fn set_timeout(&mut self, timeout: u64) {
        self.timeout = Duration::new(timeout, 0);
    }

    pub fn locale(&self) -> &Option<String> {
        &self.locale
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn scenario_timeout(&self) -> Duration {
        self.scenario_timeout
    }
}