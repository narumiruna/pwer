mod ioreg;
#[cfg(target_os = "macos")]
mod smc;
#[cfg(any(target_os = "macos", test))]
mod smc_parser;

pub use ioreg::{CollectorError, IoregCollector};

use crate::models::PowerReading;

pub trait PowerCollector: Send {
    fn collect(&self) -> Result<PowerReading, CollectorError>;
}

impl PowerCollector for IoregCollector {
    fn collect(&self) -> Result<PowerReading, CollectorError> {
        IoregCollector::collect(self)
    }
}

#[cfg(target_os = "macos")]
pub use smc::IokitCollector;

pub fn default_collector(verbose: bool) -> Result<Box<dyn PowerCollector>, CollectorError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(IokitCollector::new(verbose)))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = verbose;
        Err(CollectorError::UnsupportedPlatform)
    }
}
