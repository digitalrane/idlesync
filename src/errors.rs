use std::fmt;
use std::error::Error;

#[derive(Debug, Clone)]
pub struct MissingConfigError{
    details: String,
}

impl Error for MissingConfigError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl fmt::Display for MissingConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "configuration file missing")
    }
}
