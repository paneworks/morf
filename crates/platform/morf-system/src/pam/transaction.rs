//! The libpam side of a transaction: loading the library and driving one
//! `pam_start` .. `pam_end` through the conversation.

use std::ffi::{CStr, c_char, c_int};
use std::ptr;
use std::sync::atomic::Ordering;

use libloading::{Library, Symbol};

use super::{PAM_CANCELLED, PamError};
use crate::pam_conversation::{
    Bridge, PAM_CONV_ERR, PAM_SUCCESS, PamMessage, PamResponse, conversation,
};

const PAM_DISALLOW_NULL_AUTHTOK: c_int = 1;

#[repr(C)]
pub(super) struct PamHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct PamConversation {
    callback: Option<
        unsafe extern "C" fn(
            c_int,
            *const *const PamMessage,
            *mut *mut PamResponse,
            *mut std::ffi::c_void,
        ) -> c_int,
    >,
    data: *mut std::ffi::c_void,
}

pub(super) type PamStart = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const PamConversation,
    *mut *mut PamHandle,
) -> c_int;
pub(super) type PamStartConfdir = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const PamConversation,
    *const c_char,
    *mut *mut PamHandle,
) -> c_int;
pub(super) type PamCall = unsafe extern "C" fn(*mut PamHandle, c_int) -> c_int;
pub(super) type PamStrerror = unsafe extern "C" fn(*mut PamHandle, c_int) -> *const c_char;

/// How the transaction is started: the system's services, or a directory.
pub(super) enum Start {
    System(PamStart),
    Confdir(PamStartConfdir),
}

/// The entry points one transaction uses, resolved from a loaded libpam.
pub(super) struct PamApi {
    pub(super) start: Start,
    pub(super) authenticate: PamCall,
    pub(super) account: PamCall,
    pub(super) end: PamCall,
    pub(super) strerror: PamStrerror,
}

impl PamApi {
    /// # Safety
    /// The returned pointers are valid only while `library` stays loaded.
    unsafe fn load(library: &Library, confdir: bool) -> Result<Self, PamError> {
        let start = if confdir {
            // Optional in the library: absent before Linux-PAM 1.4, and a
            // caller who named a directory wants to know it was ignored rather
            // than have the system's service silently used instead.
            let start: Symbol<'_, PamStartConfdir> = unsafe { library.get(b"pam_start_confdir\0") }
                .map_err(|_| {
                    PamError::plain("this libpam has no pam_start_confdir; it needs Linux-PAM 1.4")
                })?;
            Start::Confdir(*start)
        } else {
            Start::System(*unsafe { symbol::<PamStart>(library, b"pam_start\0")? })
        };
        Ok(Self {
            start,
            authenticate: *unsafe { symbol::<PamCall>(library, b"pam_authenticate\0")? },
            account: *unsafe { symbol::<PamCall>(library, b"pam_acct_mgmt\0")? },
            end: *unsafe { symbol::<PamCall>(library, b"pam_end\0")? },
            strerror: *unsafe { symbol::<PamStrerror>(library, b"pam_strerror\0")? },
        })
    }
}

/// Runs one whole transaction on the calling thread.
pub(super) fn transact(
    service: &CStr,
    username: &CStr,
    confdir: Option<&CStr>,
    bridge: Bridge,
) -> Result<(), PamError> {
    let library = load_pam()?;
    let api = unsafe { PamApi::load(&library, confdir.is_some())? };
    unsafe { run(&api, service, username, confdir, bridge) }
}

/// # Safety
/// Every pointer in `api` must follow the Linux-PAM contracts.
pub(super) unsafe fn run(
    api: &PamApi,
    service: &CStr,
    username: &CStr,
    confdir: Option<&CStr>,
    bridge: Bridge,
) -> Result<(), PamError> {
    // Boxed so its address is stable for the whole transaction: PAM keeps the
    // pointer and calls back through it from inside every module.
    let bridge = Box::new(bridge);
    let conversation = PamConversation {
        callback: Some(conversation),
        data: ptr::from_ref(&*bridge).cast_mut().cast(),
    };
    let mut handle = ptr::null_mut();
    let mut status = match (&api.start, confdir) {
        (Start::Confdir(start), Some(confdir)) => unsafe {
            start(
                service.as_ptr(),
                username.as_ptr(),
                &conversation,
                confdir.as_ptr(),
                &mut handle,
            )
        },
        (Start::System(start), None) => unsafe {
            start(
                service.as_ptr(),
                username.as_ptr(),
                &conversation,
                &mut handle,
            )
        },
        _ => return Err(PamError::plain("PAM start does not match the confdir")),
    };
    if status != PAM_SUCCESS {
        let error = unsafe { pam_error(handle, status, &api.strerror) };
        if !handle.is_null() {
            unsafe { (api.end)(handle, status) };
        }
        return Err(error);
    }
    status = unsafe { (api.authenticate)(handle, PAM_DISALLOW_NULL_AUTHTOK) };
    if status == PAM_SUCCESS {
        status = unsafe { (api.account)(handle, PAM_DISALLOW_NULL_AUTHTOK) };
    }
    let result = if status == PAM_SUCCESS {
        Ok(())
    } else if bridge.cancelled.load(Ordering::SeqCst) && status == PAM_CONV_ERR {
        Err(PamError {
            code: Some(PAM_CANCELLED),
            message: "authentication cancelled".to_owned(),
        })
    } else {
        Err(unsafe { pam_error(handle, status, &api.strerror) })
    };
    let end_status = unsafe { (api.end)(handle, status) };
    if result.is_ok() && end_status != PAM_SUCCESS {
        return Err(unsafe { pam_error(ptr::null_mut(), end_status, &api.strerror) });
    }
    result
}

fn load_pam() -> Result<Library, PamError> {
    let architecture = std::env::consts::ARCH;
    let mut candidates = std::env::var_os("MORF_PAM_LIBRARY")
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    candidates.extend([
        // The loader's own search path first, then the two layouts distributions
        // actually use: Debian's multiarch directories and everyone else's flat
        // /usr/lib. Trying only the multiarch pair means no authentication at all
        // on Arch, Fedora or SUSE.
        std::path::PathBuf::from("libpam.so.0"),
        format!("/usr/lib/{architecture}-linux-gnu/libpam.so.0").into(),
        format!("/lib/{architecture}-linux-gnu/libpam.so.0").into(),
        std::path::PathBuf::from("/usr/lib/libpam.so.0"),
        std::path::PathBuf::from("/lib/libpam.so.0"),
        std::path::PathBuf::from("/usr/lib64/libpam.so.0"),
    ]);
    let mut last_error = None;
    for candidate in candidates {
        match unsafe { Library::new(&candidate) } {
            Ok(library) => return Ok(library),
            Err(error) => last_error = Some(error),
        }
    }
    Err(PamError::plain(format!(
        "could not load libpam: {}",
        last_error.expect("PAM library candidates are non-empty")
    )))
}

unsafe fn symbol<'library, T>(
    library: &'library Library,
    name: &[u8],
) -> Result<Symbol<'library, T>, PamError> {
    unsafe { library.get(name) }
        .map_err(|error| PamError::plain(format!("could not load PAM symbol: {error}")))
}

unsafe fn pam_error(handle: *mut PamHandle, code: c_int, strerror: &PamStrerror) -> PamError {
    let pointer = unsafe { strerror(handle, code) };
    let message = if pointer.is_null() {
        format!("PAM failed with code {code}")
    } else {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    };
    PamError {
        code: Some(code),
        message,
    }
}
