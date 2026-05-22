//! System sleep/resume notification.
//!
//! On Windows, registers a callback with `RegisterSuspendResumeNotification` so
//! the server can refresh the LAN IP after the network stack comes back up.
//! On other platforms, this is a no-op stub.

use tokio::sync::mpsc::UnboundedSender;

#[cfg(target_os = "windows")]
mod imp {
    use std::ffi::c_void;

    use tokio::sync::mpsc::UnboundedSender;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Power::{
        RegisterSuspendResumeNotification, UnregisterSuspendResumeNotification,
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
    };

    pub struct ResumeRegistration {
        handle: HPOWERNOTIFY,
        // Boxed so the pointer handed to Win32 stays stable; freed in Drop.
        _ctx: Box<UnboundedSender<()>>,
        _params: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
    }

    // Safety: the only field touched across threads is the boxed UnboundedSender,
    // which is itself Send + Sync. The HPOWERNOTIFY isize is opaque to us.
    unsafe impl Send for ResumeRegistration {}
    unsafe impl Sync for ResumeRegistration {}

    unsafe extern "system" fn callback(
        context: *const c_void,
        r#type: u32,
        _setting: *const c_void,
    ) -> u32 {
        if (r#type == PBT_APMRESUMESUSPEND || r#type == PBT_APMRESUMEAUTOMATIC)
            && !context.is_null()
        {
            let sender = unsafe { &*(context as *const UnboundedSender<()>) };
            sender.send(()).ok();
        }
        0 // S_OK
    }

    pub fn register(tx: UnboundedSender<()>) -> Option<ResumeRegistration> {
        let ctx = Box::new(tx);
        let context_ptr = &*ctx as *const UnboundedSender<()> as *const c_void;

        let mut params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(callback),
            Context: context_ptr as *mut c_void,
        });

        let handle = unsafe {
            RegisterSuspendResumeNotification(
                &mut *params as *mut _ as HANDLE,
                DEVICE_NOTIFY_CALLBACK,
            )
        };

        if handle == 0 {
            tracing::warn!("RegisterSuspendResumeNotification failed; resume detection disabled");
            return None;
        }

        Some(ResumeRegistration {
            handle,
            _ctx: ctx,
            _params: params,
        })
    }

    impl Drop for ResumeRegistration {
        fn drop(&mut self) {
            unsafe {
                UnregisterSuspendResumeNotification(self.handle);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use tokio::sync::mpsc::UnboundedSender;

    pub struct ResumeRegistration;

    pub fn register(_tx: UnboundedSender<()>) -> Option<ResumeRegistration> {
        None
    }
}

pub use imp::ResumeRegistration;

pub fn register_resume_notifier(tx: UnboundedSender<()>) -> Option<ResumeRegistration> {
    imp::register(tx)
}
