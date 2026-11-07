use std::str::FromStr;
use std::time::Duration;

use crate::error::ConfigError;

pub(super) struct Reader<F> {
    pub(super) get: F,
    pub(super) missing: Vec<&'static str>,
}

impl<F: Fn(&str) -> Option<String>> Reader<F> {
    pub(super) fn text(&self, name: &str) -> Option<String> {
        (self.get)(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    pub(super) fn required(&mut self, name: &'static str) -> String {
        match self.text(name) {
            Some(v) => v,
            None => {
                self.missing.push(name);
                String::new()
            }
        }
    }

    pub(super) fn positive<T: FromStr + PartialOrd + Default>(
        &self,
        name: &'static str,
        default: T,
    ) -> Result<T, ConfigError> {
        let v = match self.text(name) {
            None => default,
            Some(v) => v.parse().map_err(|_| ConfigError::Invalid(name))?,
        };
        if v > T::default() {
            Ok(v)
        } else {
            Err(ConfigError::Invalid(name))
        }
    }

    pub(super) fn secs(&self, name: &'static str, default: u64) -> Result<Duration, ConfigError> {
        self.positive(name, default).map(Duration::from_secs)
    }
}
