use std::time::Duration;

pub struct Config {
    locale: Option<String>,
    default_working_directory: Option<String>,
    timeout: Duration,
    scenario_timeout: Duration,
    scenario_start_timeout: Duration,
    scenario_success_timeout: Duration,
    scenario_failure_timeout: Duration,
    test_success_timeout: Duration,
    test_failure_timeout: Duration,
}

impl Config {
    pub fn new() -> Config {
        Config {
            locale: None,
            default_working_directory: None,
            timeout: Duration::from_secs(3600),
            scenario_timeout: Duration::from_secs(7200),
            scenario_start_timeout: Duration::from_secs(10),
            scenario_success_timeout: Duration::from_secs(10),
            scenario_failure_timeout: Duration::from_secs(10),
            test_success_timeout: Duration::from_secs(10),
            test_failure_timeout: Duration::from_secs(10),
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

    pub fn set_default_working_directory(&mut self, wd: Option<&str>) {
        self.default_working_directory = match wd {
            None => None,
            Some(s) => Some(s.to_string()),
        };
    }

    pub fn locale(&self) -> &Option<String> {
        &self.locale
    }

    pub fn default_working_directory(&self) -> &Option<String> {
        &self.default_working_directory
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn test_success_timeout(&self) -> Duration {
        self.test_success_timeout
    }

    pub fn test_failure_timeout(&self) -> Duration {
        self.test_failure_timeout
    }

    pub fn scenario_timeout(&self) -> Duration {
        self.scenario_timeout
    }

    pub fn scenario_start_timeout(&self) -> Duration {
        self.scenario_start_timeout
    }

    pub fn scenario_success_timeout(&self) -> Duration {
        self.scenario_success_timeout
    }

    pub fn scenario_failure_timeout(&self) -> Duration {
        self.scenario_failure_timeout
    }
}