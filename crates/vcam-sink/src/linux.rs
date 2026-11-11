use crate::{LoopbackDeviceInfo, VcamConfig, VcamError, VirtualCameraSink};
use core_protocol::PixelFormat;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use tracing::{debug, error, info};

// Linux V4L2 Constants
const VIDIOC_QUERYCAP: libc::c_ulong = 0x80685600;
const VIDIOC_S_FMT: libc::c_ulong = 0xc0d05605;
const V4L2_BUF_TYPE_VIDEO_OUTPUT: u32 = 2;
const V4L2_FIELD_NONE: u32 = 1;
const V4L2_CAP_VIDEO_OUTPUT: u32 = 0x00000002;
const V4L2_CAP_DEVICE_CAPS: u32 = 0x80000000;

// FourCC formats
const V4L2_PIX_FMT_YUYV: u32 = 0x56595559; // 'Y' | 'U' << 8 | 'Y' << 16 | 'V' << 24
const V4L2_PIX_FMT_RGB24: u32 = 0x33424752; // 'R' | 'G' << 8 | 'B' << 16 | '3' << 24
const V4L2_PIX_FMT_NV12: u32 = 0x3231564e; // 'N' | 'V' << 8 | '1' << 16 | '2' << 24

#[repr(C)]
struct V4l2Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
struct V4l2PixFormat {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_data: u32,
    flags: u32,
    ycbcr_or_hsv_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

#[repr(C)]
struct V4l2Format {
    buf_type: u32,
    _pad: u32, // Ensures 8-byte alignment of the union on 64-bit systems
    pix: V4l2PixFormat,
    _raw_remainder: [u8; 152], // Total 208 bytes (4 + 4 + 48 + 152 = 208)
}

/// Linux v4l2loopback virtual camera sink.
pub struct V4l2LoopbackSink {
    file: Option<File>,
    config: Option<VcamConfig>,
}

impl Default for V4l2LoopbackSink {
    fn default() -> Self {
        Self::new()
    }
}

impl V4l2LoopbackSink {
    pub fn new() -> Self {
        Self {
            file: None,
            config: None,
        }
    }
}

impl VirtualCameraSink for V4l2LoopbackSink {
    fn open(&mut self, config: VcamConfig) -> Result<(), VcamError> {
        info!("Opening V4L2 loopback device at {}", config.device_path);

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&config.device_path)
            .map_err(|e| VcamError::DeviceOpenFailed {
                device: config.device_path.clone(),
                source: e,
            })?;

        let fd = file.as_raw_fd();

        // 1. Verify capabilities via VIDIOC_QUERYCAP
        let mut cap: V4l2Capability = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::ioctl(fd, VIDIOC_QUERYCAP, &mut cap) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            error!("VIDIOC_QUERYCAP failed on {}: {}", config.device_path, err);
            return Err(VcamError::IoctlFailed {
                call: "VIDIOC_QUERYCAP",
                source: err,
            });
        }

        let caps = if (cap.capabilities & V4L2_CAP_DEVICE_CAPS) != 0 {
            cap.device_caps
        } else {
            cap.capabilities
        };

        if (caps & V4L2_CAP_VIDEO_OUTPUT) == 0 {
            return Err(VcamError::NotAnOutputDevice(config.device_path.clone()));
        }

        // 2. Configure video format via VIDIOC_S_FMT
        let fourcc = match config.format {
            PixelFormat::YUYV => V4L2_PIX_FMT_YUYV,
            PixelFormat::RGB24 => V4L2_PIX_FMT_RGB24,
            PixelFormat::NV12 => V4L2_PIX_FMT_NV12,
            PixelFormat::RGBA => {
                // v4l2 does not standardly write RGBA32 directly on all loopback, we convert to YUYV or RGB24
                V4L2_PIX_FMT_YUYV
            }
        };

        let bytes_per_line = match config.format {
            PixelFormat::YUYV => config.width * 2,
            PixelFormat::RGB24 => config.width * 3,
            PixelFormat::NV12 => config.width,
            PixelFormat::RGBA => config.width * 2,
        };

        let size_image = match config.format {
            PixelFormat::YUYV => config.width * config.height * 2,
            PixelFormat::RGB24 => config.width * config.height * 3,
            PixelFormat::NV12 => config.width * config.height * 3 / 2,
            PixelFormat::RGBA => config.width * config.height * 2,
        };

        let mut fmt: V4l2Format = unsafe { std::mem::zeroed() };
        fmt.buf_type = V4L2_BUF_TYPE_VIDEO_OUTPUT;
        fmt.pix.width = config.width;
        fmt.pix.height = config.height;
        fmt.pix.pixelformat = fourcc;
        fmt.pix.field = V4L2_FIELD_NONE;
        fmt.pix.bytesperline = bytes_per_line;
        fmt.pix.sizeimage = size_image;

        let ret = unsafe { libc::ioctl(fd, VIDIOC_S_FMT, &mut fmt) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            error!("VIDIOC_S_FMT failed on {}: {}", config.device_path, err);
            return Err(VcamError::IoctlFailed {
                call: "VIDIOC_S_FMT",
                source: err,
            });
        }

        info!(
            "V4L2 device {} configured successfully: {}x{} {:?} ({} bytes/frame)",
            config.device_path, config.width, config.height, config.format, size_image
        );

        self.file = Some(file);
        self.config = Some(config);

        Ok(())
    }

    fn write_frame(&mut self, frame: &[u8]) -> Result<(), VcamError> {
        let file = self.file.as_mut().ok_or(VcamError::DeviceNotOpen)?;
        let fd = file.as_raw_fd();

        let ret = unsafe { libc::write(fd, frame.as_ptr() as *const libc::c_void, frame.len()) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            // In non-blocking mode, EAGAIN or EWOULDBLOCK can happen if consumer is slow
            if err.raw_os_error() == Some(libc::EAGAIN) {
                debug!("v4l2 frame dropped: consumer buffer full (EAGAIN)");
                return Ok(());
            }
            return Err(VcamError::WriteFailed(err));
        }

        Ok(())
    }

    fn close(&mut self) -> Result<(), VcamError> {
        self.file = None;
        self.config = None;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.file.is_some()
    }

    fn config(&self) -> Option<&VcamConfig> {
        self.config.as_ref()
    }

    fn device_path(&self) -> &str {
        self.config
            .as_ref()
            .map(|c| c.device_path.as_str())
            .unwrap_or("")
    }
}

/// Scans `/dev/video*` devices on the Linux system and identifies loopback video output sinks.
pub fn scan_loopback_devices() -> Vec<LoopbackDeviceInfo> {
    let mut devices = Vec::new();

    for i in 0..64 {
        let path = format!("/dev/video{}", i);
        if !std::path::Path::new(&path).exists() {
            continue;
        }

        let fd = unsafe {
            libc::open(
                std::ffi::CString::new(path.clone()).unwrap().as_ptr(),
                libc::O_RDWR | libc::O_NONBLOCK,
            )
        };

        if fd < 0 {
            // Cannot open (maybe permission denied or in use), test read-only
            let fd_ro = unsafe {
                libc::open(
                    std::ffi::CString::new(path.clone()).unwrap().as_ptr(),
                    libc::O_RDONLY | libc::O_NONBLOCK,
                )
            };
            if fd_ro < 0 {
                continue;
            }
            if let Some(info) = query_device_caps(fd_ro, &path, false) {
                devices.push(info);
            }
            unsafe { libc::close(fd_ro) };
            continue;
        }

        if let Some(info) = query_device_caps(fd, &path, true) {
            devices.push(info);
        }
        unsafe { libc::close(fd) };
    }

    devices
}

fn query_device_caps(fd: RawFd, path: &str, is_writable: bool) -> Option<LoopbackDeviceInfo> {
    let mut cap: V4l2Capability = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::ioctl(fd, VIDIOC_QUERYCAP, &mut cap) };
    if ret != 0 {
        return None;
    }

    let card_label = cstr_to_string(&cap.card);
    let driver = cstr_to_string(&cap.driver);
    let bus_info = cstr_to_string(&cap.bus_info);

    let caps = if (cap.capabilities & V4L2_CAP_DEVICE_CAPS) != 0 {
        cap.device_caps
    } else {
        cap.capabilities
    };

    let is_output = (caps & V4L2_CAP_VIDEO_OUTPUT) != 0;
    let is_loopback = is_output || driver.contains("v4l2 loopback") || card_label.contains("Loopback") || card_label.contains("OpenCamLink");

    Some(LoopbackDeviceInfo {
        path: path.to_string(),
        card_label,
        driver,
        bus_info,
        is_loopback,
        is_available: is_writable,
    })
}

fn cstr_to_string(bytes: &[u8]) -> String {
    let nul_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..nul_pos]).trim().to_string()
}

pub fn get_setup_instructions() -> &'static str {
    "Pour créer des caméras virtuelles sous Linux avec v4l2loopback :\n\
    1. Installez le module : sudo apt install v4l2loopback-dkms (Debian/Ubuntu) ou sudo pacman -S v4l2loopback-dkms (Arch)\n\
    2. Chargez les périphériques virtuels :\n   \
       sudo modprobe v4l2loopback devices=2 video_nr=10,11 card_label=\"OpenCamLink 1\",\"OpenCamLink 2\" exclusive_caps=1"
}
