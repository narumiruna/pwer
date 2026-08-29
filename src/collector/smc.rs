use std::ffi::{c_char, c_void};
use std::mem::{size_of, zeroed};

use crate::collector::smc_parser::{bytes_to_float, str_to_key, type_to_string};
use crate::collector::{CollectorError, IoregCollector, PowerCollector};
use crate::models::PowerReading;

const KERN_SUCCESS: i32 = 0;
const KERNEL_INDEX_SMC: u32 = 2;
const SMC_CMD_READ_BYTES: u8 = 5;
const SMC_CMD_READ_KEYINFO: u8 = 9;
const SENSOR_KEYS: [&str; 7] = ["PPBR", "PDTR", "PSTR", "PHPC", "PDBR", "TB0T", "CHCC"];

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SmcVersion {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SmcPowerLimitData {
    version: u16,
    length: u16,
    cpu_plimit: u32,
    gpu_plimit: u32,
    mem_plimit: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct KeyInfo {
    data_size: u32,
    data_type: u32,
    data_attributes: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SmcKeyData {
    key: u32,
    version: SmcVersion,
    power_limit: SmcPowerLimitData,
    key_info: KeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOMasterPort(bootstrap_port: u32, master_port: *mut u32) -> i32;
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingServices(
        master_port: u32,
        matching: *mut c_void,
        iterator: *mut u32,
    ) -> i32;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOServiceOpen(service: u32, owning_task: u32, r#type: u32, connect: *mut u32) -> i32;
    fn IOServiceClose(connect: u32) -> i32;
    fn IOConnectCallStructMethod(
        connection: u32,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> i32;
}

#[link(name = "System")]
unsafe extern "C" {
    static mach_task_self_: u32;
}

#[derive(Default, Debug)]
struct SmcPowerData {
    battery_power: Option<f64>,
    power_input: Option<f64>,
    system_power: Option<f64>,
    heatpipe_power: Option<f64>,
    display_power: Option<f64>,
    battery_temp: Option<f64>,
    charging_status: Option<f64>,
}

pub struct IokitCollector {
    verbose: bool,
    fallback: IoregCollector,
}

impl IokitCollector {
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            fallback: IoregCollector,
        }
    }

    fn collect_with_smc(&self) -> Result<PowerReading, CollectorError> {
        let data = self.read_smc_sensors()?;
        let mut reading = self.fallback.collect()?;
        if let Some(power_input) = data.power_input {
            reading.watts_actual = power_input;
        }
        if self.verbose {
            log::debug!("SMC sensor data: {data:?}");
        }
        Ok(reading)
    }

    fn read_smc_sensors(&self) -> Result<SmcPowerData, CollectorError> {
        let connection = SmcConnection::open()?;
        let mut data = SmcPowerData::default();
        for key in SENSOR_KEYS {
            let Ok(value) = connection.read_key(key) else {
                continue;
            };
            match key {
                "PPBR" => data.battery_power = Some(value),
                "PDTR" => data.power_input = Some(value),
                "PSTR" => data.system_power = Some(value),
                "PHPC" => data.heatpipe_power = Some(value),
                "PDBR" => data.display_power = Some(value),
                "TB0T" => data.battery_temp = Some(value),
                "CHCC" => data.charging_status = Some(value),
                _ => {}
            }
        }
        Ok(data)
    }
}

impl PowerCollector for IokitCollector {
    fn collect(&self) -> Result<PowerReading, CollectorError> {
        match self.collect_with_smc() {
            Ok(reading) => Ok(reading),
            Err(error) => {
                if self.verbose {
                    log::warn!("SMC access failed: {error}; falling back to ioreg");
                }
                self.fallback.collect()
            }
        }
    }
}

struct SmcConnection {
    connection: u32,
    service: u32,
}

impl SmcConnection {
    fn open() -> Result<Self, CollectorError> {
        let mut master_port = 0;
        // SAFETY: IOMasterPort receives a valid pointer to an initialized u32.
        check_return(unsafe { IOMasterPort(0, &mut master_port) }, "IOMasterPort")?;
        // SAFETY: The argument is a static NUL-terminated service name.
        let matching = unsafe { IOServiceMatching(c"AppleSMC".as_ptr()) };
        if matching.is_null() {
            return Err(CollectorError::Smc(
                "IOServiceMatching returned NULL".into(),
            ));
        }
        let mut iterator = 0;
        // SAFETY: matching is returned by IOServiceMatching and consumed by this call.
        check_return(
            unsafe { IOServiceGetMatchingServices(master_port, matching, &mut iterator) },
            "IOServiceGetMatchingServices",
        )?;
        // SAFETY: iterator is a valid IOKit iterator returned above.
        let service = unsafe { IOIteratorNext(iterator) };
        // SAFETY: iterator is no longer used after this release.
        let _ = unsafe { IOObjectRelease(iterator) };
        if service == 0 {
            return Err(CollectorError::Smc("AppleSMC service not found".into()));
        }
        let mut connection = 0;
        // SAFETY: service is valid and connection points to writable storage.
        let result = unsafe { IOServiceOpen(service, mach_task_self_, 0, &mut connection) };
        if result != KERN_SUCCESS {
            // SAFETY: service was acquired above and has not yet been released.
            let _ = unsafe { IOObjectRelease(service) };
            return Err(smc_error("IOServiceOpen", result));
        }
        Ok(Self {
            connection,
            service,
        })
    }

    fn read_key(&self, key: &str) -> Result<f64, CollectorError> {
        if key.len() != 4 {
            return Err(CollectorError::Smc(format!(
                "SMC key must be exactly 4 characters: {key}"
            )));
        }
        let key = str_to_key(key);
        let key_info = self.read_key_info(key)?;
        let bytes = self.read_key_bytes(key, key_info)?;
        Ok(bytes_to_float(
            &bytes,
            &type_to_string(key_info.data_type),
            key_info.data_size as usize,
        ))
    }

    fn read_key_info(&self, key: u32) -> Result<KeyInfo, CollectorError> {
        let mut input = SmcKeyData {
            key,
            data8: SMC_CMD_READ_KEYINFO,
            ..Default::default()
        };
        let output = self.call(&mut input)?;
        Ok(output.key_info)
    }

    fn read_key_bytes(&self, key: u32, key_info: KeyInfo) -> Result<Vec<u8>, CollectorError> {
        let mut input = SmcKeyData {
            key,
            key_info,
            data8: SMC_CMD_READ_BYTES,
            ..Default::default()
        };
        let output = self.call(&mut input)?;
        let size = usize::try_from(key_info.data_size)
            .unwrap_or(0)
            .min(output.bytes.len());
        Ok(output.bytes[..size].to_vec())
    }

    fn call(&self, input: &mut SmcKeyData) -> Result<SmcKeyData, CollectorError> {
        // SAFETY: A zeroed C-compatible output structure is valid for this IOKit call.
        let mut output: SmcKeyData = unsafe { zeroed() };
        let mut output_size = size_of::<SmcKeyData>();
        // SAFETY: Both structures are valid for their declared sizes during the call.
        let result = unsafe {
            IOConnectCallStructMethod(
                self.connection,
                KERNEL_INDEX_SMC,
                std::ptr::from_ref(input).cast(),
                size_of::<SmcKeyData>(),
                std::ptr::from_mut(&mut output).cast(),
                &mut output_size,
            )
        };
        check_return(result, "IOConnectCallStructMethod")?;
        Ok(output)
    }
}

impl Drop for SmcConnection {
    fn drop(&mut self) {
        if self.connection != 0 {
            // SAFETY: This object owns the open connection and closes it once here.
            let _ = unsafe { IOServiceClose(self.connection) };
        }
        if self.service != 0 {
            // SAFETY: This object owns the service reference and releases it once here.
            let _ = unsafe { IOObjectRelease(self.service) };
        }
    }
}

fn check_return(code: i32, operation: &str) -> Result<(), CollectorError> {
    if code == KERN_SUCCESS {
        Ok(())
    } else {
        Err(smc_error(operation, code))
    }
}

fn smc_error(operation: &str, code: i32) -> CollectorError {
    CollectorError::Smc(format!(
        "{operation} failed: {} ({code})",
        kern_return_name(code as u32)
    ))
}

fn kern_return_name(code: u32) -> String {
    match code {
        0 => "KERN_SUCCESS".into(),
        1 => "KERN_INVALID_ADDRESS".into(),
        2 => "KERN_PROTECTION_FAILURE".into(),
        3 => "KERN_NO_SPACE".into(),
        4 => "KERN_INVALID_ARGUMENT".into(),
        5 => "KERN_FAILURE".into(),
        0xE00002C0 => "kIOReturnError".into(),
        0xE00002C1 => "kIOReturnNoMemory".into(),
        0xE00002C2 => "kIOReturnNoDevice".into(),
        0xE00002C3 => "kIOReturnNoResources".into(),
        0xE00002C7 => "kIOReturnBusy".into(),
        0xE00002C8 => "kIOReturnTimeout".into(),
        0xE00002D8 => "kIOReturnNotPrivileged".into(),
        0xE00002E2 => "kIOReturnExclusiveAccess".into(),
        _ => format!("Unknown error 0x{code:08X}"),
    }
}
