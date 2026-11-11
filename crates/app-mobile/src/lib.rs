pub mod transmitter;

pub use transmitter::{DiscoveredDesktop, TransmitterEngine};

/// Entry point when invoked as a shared dynamic library from Android NativeActivity.
#[cfg(target_os = "android")]
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn android_main(_app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    tracing::info!("OpenCamLink Android Native Activity launched!");
    // Runs mobile engine with native window
}

