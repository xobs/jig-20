use std::time::Duration;

pub struct Config {
    locale: Option<String>,
    timeout: Duration,
}

impl Config {
    pub fn new() -> Config {
        Config {
            locale: None,
            timeout: Duration::new(3600, 0),
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
}