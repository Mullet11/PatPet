//! Volume dan mute speaker utama lewat Core Audio. Berbasis event: Windows
//! memanggil `OnNotify` setiap kali volume berubah, dari thread miliknya.

use std::sync::Arc;

use windows::core::{implement, Result, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

/// Dipanggil dengan (volume 0–1, mute).
type OnVolume = Arc<dyn Fn(f32, bool) + Send + Sync>;
/// Dipanggil saat speaker utama berganti (mis. headphone dicolok).
type OnDevice = Arc<dyn Fn() + Send + Sync>;

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    on_volume: OnVolume,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        // SAFETY: Windows memberi pointer valid selama panggilan ini.
        if let Some(d) = unsafe { data.as_ref() } {
            (self.on_volume)(d.fMasterVolume, d.bMuted.as_bool());
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceWatcher {
    on_device: OnDevice,
}

impl IMMNotificationClient_Impl for DeviceWatcher_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eRender && role == eConsole {
            (self.on_device)();
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

/// Langganan volume speaker utama. Semua callback dilepas saat di-drop.
pub struct Audio {
    enumerator: IMMDeviceEnumerator,
    watcher: IMMNotificationClient,
    endpoint: Option<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)>,
    on_volume: OnVolume,
}

impl Audio {
    pub fn start(on_volume: impl Fn(f32, bool) + Send + Sync + 'static, on_device: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        // SAFETY: COM sudah aktif di thread UI (platform::install).
        let enumerator: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok()?;
        let watcher: IMMNotificationClient = DeviceWatcher { on_device: Arc::new(on_device) }.into();
        // SAFETY: watcher hidup selama Audio dan dilepas di Drop.
        unsafe { enumerator.RegisterEndpointNotificationCallback(&watcher) }.ok()?;
        let mut audio = Self { enumerator, watcher, endpoint: None, on_volume: Arc::new(on_volume) };
        audio.attach_default();
        Some(audio)
    }

    /// Pindah langganan ke speaker utama yang sekarang.
    pub fn attach_default(&mut self) {
        self.detach();
        // SAFETY: objek COM dilepas otomatis; callback dilepas di `detach`.
        let endpoint = unsafe {
            self.enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .and_then(|device| device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None))
        };
        let Ok(endpoint) = endpoint else { return };
        let callback: IAudioEndpointVolumeCallback = VolumeCallback { on_volume: self.on_volume.clone() }.into();
        // SAFETY: callback disimpan bersama endpoint sampai `detach`.
        if unsafe { endpoint.RegisterControlChangeNotify(&callback) }.is_ok() {
            self.endpoint = Some((endpoint, callback));
        }
    }

    fn detach(&mut self) {
        if let Some((endpoint, callback)) = self.endpoint.take() {
            // SAFETY: callback yang sama dengan yang didaftarkan.
            let _ = unsafe { endpoint.UnregisterControlChangeNotify(&callback) };
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.detach();
        // SAFETY: watcher yang sama dengan yang didaftarkan.
        let _ = unsafe { self.enumerator.UnregisterEndpointNotificationCallback(&self.watcher) };
    }
}
