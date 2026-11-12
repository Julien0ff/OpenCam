//! Windows DirectShow Virtual Camera Sink implementation.
//!
//! Embeds a lightweight DirectShow Video Capture Source Filter (`opencam_vcam.dll`),
//! registers it per-user in `HKCU\Software\Classes\CLSID` (no admin required),
//! and streams YUY2 frames via Windows Shared Memory (`Local\OpenCamVirtualCameraFrame`)
//! so apps like Discord, Chrome, OBS, and Zoom can select "OpenCam Virtual Camera".

use crate::{VcamConfig, VcamError, VirtualCameraSink};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};

const VCAM_DLL_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/opencam_vcam.dll"));

const CLSID_OPENCAM_VCAM: &str = "{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}";
const CLSID_VIDEO_INPUT_DEVICE_CATEGORY: &str = "{860BB310-5D01-11d0-BD3B-00A0C911CE86}";
const FILTER_FRIENDLY_NAME: &str = "OpenCam Virtual Camera";

const SHARED_MEM_NAME: &str = "Local\\OpenCamVirtualCameraFrame";
const SHARED_MAGIC: u32 = 0x4F43_414D; // "OCAM"
const SHARED_VERSION: u32 = 1;
const MAX_FRAME_BYTES: usize = 3840 * 2160 * 2; // 4K YUY2 max

#[repr(C, packed)]
struct OpenCamSharedHeader {
    magic: u32,
    version: u32,
    width: u32,
    height: u32,
    fps: u32,
    active: u32,
    frame_seq: u64,
}

fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn set_hkcu_reg_sz(subkey: &str, value_name: Option<&str>, data: &str) -> Result<(), std::io::Error> {
    let subkey_w = to_wide_null(subkey);
    let mut hkey: HKEY = ptr::null_mut();
    let res = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey_w.as_ptr(),
            0,
            ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            ptr::null(),
            &mut hkey,
            ptr::null_mut(),
        )
    };
    if res != 0 {
        return Err(std::io::Error::from_raw_os_error(res as i32));
    }

    let val_w = value_name.map(to_wide_null);
    let data_w = to_wide_null(data);
    let byte_len = (data_w.len() * std::mem::size_of::<u16>()) as u32;
    let res_set = unsafe {
        RegSetValueExW(
            hkey,
            val_w.as_ref().map_or(ptr::null(), |v| v.as_ptr()),
            0,
            REG_SZ,
            data_w.as_ptr() as *const u8,
            byte_len,
        )
    };
    unsafe {
        RegCloseKey(hkey);
    }
    if res_set != 0 {
        return Err(std::io::Error::from_raw_os_error(res_set as i32));
    }
    Ok(())
}

fn vcam_dll_install_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("OpenCam").join("opencam_vcam.dll")
}

/// Extracts `opencam_vcam.dll` and registers it in HKCU (no admin privileges needed)
/// so Discord, Chrome, OBS, and Zoom can discover "OpenCam Virtual Camera".
pub fn ensure_virtual_camera_registered() -> Result<(), VcamError> {
    let dll_path = vcam_dll_install_path();
    if let Some(parent) = dll_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let needs_write = match std::fs::read(&dll_path) {
        Ok(existing) => existing != VCAM_DLL_BYTES,
        Err(_) => true,
    };

    if needs_write {
        if let Err(e) = std::fs::write(&dll_path, VCAM_DLL_BYTES) {
            // If another app (e.g. Discord) already loaded the DLL, keep the existing file.
            if !dll_path.exists() {
                return Err(VcamError::DeviceOpenFailed {
                    device: dll_path.display().to_string(),
                    source: e,
                });
            }
            warn!("Could not overwrite loaded {}: {}", dll_path.display(), e);
        }
    }

    let dll_str = dll_path.to_string_lossy().to_string();
    let clsid_key = format!("Software\\Classes\\CLSID\\{}", CLSID_OPENCAM_VCAM);
    let inproc_key = format!("{}\\InprocServer32", clsid_key);
    let instance_key = format!(
        "Software\\Classes\\CLSID\\{}\\Instance\\{}",
        CLSID_VIDEO_INPUT_DEVICE_CATEGORY, CLSID_OPENCAM_VCAM
    );

    set_hkcu_reg_sz(&clsid_key, None, FILTER_FRIENDLY_NAME).map_err(|e| {
        VcamError::DeviceOpenFailed {
            device: FILTER_FRIENDLY_NAME.to_string(),
            source: e,
        }
    })?;
    set_hkcu_reg_sz(&inproc_key, None, &dll_str).map_err(|e| {
        VcamError::DeviceOpenFailed {
            device: FILTER_FRIENDLY_NAME.to_string(),
            source: e,
        }
    })?;
    set_hkcu_reg_sz(&inproc_key, Some("ThreadingModel"), "Both").map_err(|e| {
        VcamError::DeviceOpenFailed {
            device: FILTER_FRIENDLY_NAME.to_string(),
            source: e,
        }
    })?;
    set_hkcu_reg_sz(&instance_key, Some("FriendlyName"), FILTER_FRIENDLY_NAME).map_err(|e| {
        VcamError::DeviceOpenFailed {
            device: FILTER_FRIENDLY_NAME.to_string(),
            source: e,
        }
    })?;
    set_hkcu_reg_sz(&instance_key, Some("CLSID"), CLSID_OPENCAM_VCAM).map_err(|e| {
        VcamError::DeviceOpenFailed {
            device: FILTER_FRIENDLY_NAME.to_string(),
            source: e,
        }
    })?;

    info!("Registered '{}' at {}", FILTER_FRIENDLY_NAME, dll_str);
    Ok(())
}

/// Windows DirectShow Virtual Camera Sink.
pub struct WindowsMfVirtualCameraSink {
    config: Option<VcamConfig>,
    is_active: bool,
    map_handle: HANDLE,
    view_addr: *mut u8,
    seq: AtomicU64,
}

unsafe impl Send for WindowsMfVirtualCameraSink {}
unsafe impl Sync for WindowsMfVirtualCameraSink {}

impl Default for WindowsMfVirtualCameraSink {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsMfVirtualCameraSink {
    pub fn new() -> Self {
        Self {
            config: None,
            is_active: false,
            map_handle: ptr::null_mut(),
            view_addr: ptr::null_mut(),
            seq: AtomicU64::new(0),
        }
    }
}

impl Drop for WindowsMfVirtualCameraSink {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl VirtualCameraSink for WindowsMfVirtualCameraSink {
    fn open(&mut self, config: VcamConfig) -> Result<(), VcamError> {
        if self.is_active {
            let _ = self.close();
        }

        if let Err(e) = ensure_virtual_camera_registered() {
            warn!("Virtual camera registration warning: {}", e);
        }

        let total_size = (std::mem::size_of::<OpenCamSharedHeader>() + MAX_FRAME_BYTES) as u32;
        let name_w = to_wide_null(SHARED_MEM_NAME);

        let h_map = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null(),
                PAGE_READWRITE,
                0,
                total_size,
                name_w.as_ptr(),
            )
        };
        if h_map.is_null() {
            return Err(VcamError::DeviceOpenFailed {
                device: FILTER_FRIENDLY_NAME.to_string(),
                source: std::io::Error::last_os_error(),
            });
        }

        let mapped = unsafe { MapViewOfFile(h_map, FILE_MAP_ALL_ACCESS, 0, 0, total_size as usize) };
        if mapped.Value.is_null() {
            let err = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(h_map);
            }
            return Err(VcamError::DeviceOpenFailed {
                device: FILTER_FRIENDLY_NAME.to_string(),
                source: err,
            });
        }

        let view_ptr = mapped.Value as *mut u8;
        unsafe {
            let hdr = &mut *(view_ptr as *mut OpenCamSharedHeader);
            hdr.magic = SHARED_MAGIC;
            hdr.version = SHARED_VERSION;
            hdr.width = config.width;
            hdr.height = config.height;
            hdr.fps = config.fps.max(1);
            hdr.active = 1;
            hdr.frame_seq = 0;
        }

        info!(
            "Windows DirectShow Virtual Camera Sink opened: {}x{} @ {}fps",
            config.width, config.height, config.fps
        );

        self.map_handle = h_map;
        self.view_addr = view_ptr;
        self.config = Some(config);
        self.is_active = true;
        Ok(())
    }

    fn write_frame(&mut self, frame: &[u8]) -> Result<(), VcamError> {
        if !self.is_active || self.view_addr.is_null() {
            return Err(VcamError::DeviceNotOpen);
        }

        let copy_len = frame.len().min(MAX_FRAME_BYTES);
        let hdr_size = std::mem::size_of::<OpenCamSharedHeader>();

        unsafe {
            ptr::copy_nonoverlapping(frame.as_ptr(), self.view_addr.add(hdr_size), copy_len);
            let hdr = &mut *(self.view_addr as *mut OpenCamSharedHeader);
            if let Some(ref cfg) = self.config {
                hdr.width = cfg.width;
                hdr.height = cfg.height;
                hdr.fps = cfg.fps.max(1);
            }
            hdr.active = 1;
            hdr.frame_seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), VcamError> {
        if !self.view_addr.is_null() {
            unsafe {
                let hdr = &mut *(self.view_addr as *mut OpenCamSharedHeader);
                hdr.active = 0;
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view_addr as *mut core::ffi::c_void,
                });
            }
            self.view_addr = ptr::null_mut();
        }
        if !self.map_handle.is_null() {
            unsafe {
                CloseHandle(self.map_handle);
            }
            self.map_handle = ptr::null_mut();
        }
        self.is_active = false;
        self.config = None;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.is_active
    }

    fn config(&self) -> Option<&VcamConfig> {
        self.config.as_ref()
    }

    fn device_path(&self) -> &str {
        FILTER_FRIENDLY_NAME
    }
}
