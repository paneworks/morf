//! The part of libpipewire's C ABI this backend uses, declared by hand.
//!
//! PipeWire's API is mostly inline functions over interface tables: a proxy
//! pointer *is* a `spa_interface`, whose callbacks point at a table of
//! methods that begins with its version. So there are only a handful of real
//! symbols to find; the rest is reading those tables. Each table below is
//! declared up to the last method used — the library never reads a table
//! from us beyond the version we claim, and we never read one of its tables
//! past what we declared. Every interface here is at a version PipeWire 0.3
//! already had, so any libpipewire a desktop ships will do.

#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_void};

use libloading::Library;

pub const PW_ID_CORE: u32 = 0;
pub const PW_ID_ANY: u32 = 0xffff_ffff;

pub const TYPE_NODE: &str = "PipeWire:Interface:Node";
pub const TYPE_DEVICE: &str = "PipeWire:Interface:Device";
pub const TYPE_LINK: &str = "PipeWire:Interface:Link";
pub const TYPE_METADATA: &str = "PipeWire:Interface:Metadata";

pub const VERSION_REGISTRY: u32 = 3;
pub const VERSION_NODE: u32 = 3;
pub const VERSION_DEVICE: u32 = 3;
pub const VERSION_METADATA: u32 = 3;

/// `enum spa_param_type`.
pub const PARAM_PROPS: u32 = 2;
pub const PARAM_ENUM_FORMAT: u32 = 3;
pub const PARAM_FORMAT: u32 = 4;
pub const PARAM_ROUTE: u32 = 13;

/// `enum pw_direction` / `enum spa_direction`.
pub const DIRECTION_INPUT: u32 = 0;
pub const DIRECTION_OUTPUT: u32 = 1;

/// `enum pw_stream_flags`.
pub const STREAM_FLAG_AUTOCONNECT: u32 = 1 << 0;
pub const STREAM_FLAG_MAP_BUFFERS: u32 = 1 << 2;

/// `enum pw_stream_state`.
pub const STREAM_STATE_ERROR: c_int = -1;
pub const STREAM_STATE_STREAMING: c_int = 3;

#[repr(C)]
pub struct SpaCallbacks {
    pub funcs: *const c_void,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct SpaInterface {
    pub type_: *const c_char,
    pub version: u32,
    pub cb: SpaCallbacks,
}

#[repr(C)]
pub struct SpaList {
    pub next: *mut SpaList,
    pub prev: *mut SpaList,
}

/// A listener's registration. The library links it into a list, so it must
/// not move while registered: every one lives in a `Box`.
#[repr(C)]
pub struct SpaHook {
    pub link: SpaList,
    pub cb: SpaCallbacks,
    pub removed: Option<unsafe extern "C" fn(*mut SpaHook)>,
    pub private: *mut c_void,
}

impl SpaHook {
    pub fn zeroed() -> Self {
        // SAFETY: all-zero is how C code initialises a hook: null pointers
        // and no callback.
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
pub struct SpaDictItem {
    pub key: *const c_char,
    pub value: *const c_char,
}

#[repr(C)]
pub struct SpaDict {
    pub flags: u32,
    pub n_items: u32,
    pub items: *const SpaDictItem,
}

#[repr(C)]
pub struct SpaPod {
    pub size: u32,
    pub type_: u32,
}

#[repr(C)]
pub struct PwLoop {
    pub system: *mut c_void,
    pub loop_: *mut c_void,
    pub control: *mut c_void,
    /// A `spa_loop_utils`, which is a `spa_interface`.
    pub utils: *mut SpaInterface,
    pub name: *const c_char,
}

#[repr(C)]
pub struct LoopUtilsMethods {
    pub version: u32,
    pub add_io: *const c_void,
    pub update_io: *const c_void,
    pub add_idle: *const c_void,
    pub enable_idle: *const c_void,
    pub add_event: Option<
        unsafe extern "C" fn(
            *mut c_void,
            Option<unsafe extern "C" fn(*mut c_void, u64)>,
            *mut c_void,
        ) -> *mut c_void,
    >,
    pub signal_event: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int>,
    pub add_timer: *const c_void,
    pub update_timer: *const c_void,
    pub add_signal: *const c_void,
    pub destroy_source: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
}

#[repr(C)]
pub struct CoreMethods {
    pub version: u32,
    pub add_listener: Option<
        unsafe extern "C" fn(*mut c_void, *mut SpaHook, *const CoreEvents, *mut c_void) -> c_int,
    >,
    pub hello: *const c_void,
    pub sync: Option<unsafe extern "C" fn(*mut c_void, u32, c_int) -> c_int>,
    pub pong: *const c_void,
    pub error: *const c_void,
    pub get_registry: Option<unsafe extern "C" fn(*mut c_void, u32, usize) -> *mut c_void>,
}

#[repr(C)]
pub struct CoreEvents {
    pub version: u32,
    pub info: Option<unsafe extern "C" fn(*mut c_void, *const c_void)>,
    pub done: Option<unsafe extern "C" fn(*mut c_void, u32, c_int)>,
    pub ping: Option<unsafe extern "C" fn(*mut c_void, u32, c_int)>,
    pub error: Option<unsafe extern "C" fn(*mut c_void, u32, c_int, c_int, *const c_char)>,
    pub remove_id: Option<unsafe extern "C" fn(*mut c_void, u32)>,
    pub bound_id: Option<unsafe extern "C" fn(*mut c_void, u32, u32)>,
    pub add_mem: Option<unsafe extern "C" fn(*mut c_void, u32, u32, c_int, u32)>,
    pub remove_mem: Option<unsafe extern "C" fn(*mut c_void, u32)>,
}

#[repr(C)]
pub struct RegistryMethods {
    pub version: u32,
    pub add_listener: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *mut SpaHook,
            *const RegistryEvents,
            *mut c_void,
        ) -> c_int,
    >,
    pub bind:
        Option<unsafe extern "C" fn(*mut c_void, u32, *const c_char, u32, usize) -> *mut c_void>,
}

#[repr(C)]
pub struct RegistryEvents {
    pub version: u32,
    pub global:
        Option<unsafe extern "C" fn(*mut c_void, u32, u32, *const c_char, u32, *const SpaDict)>,
    pub global_remove: Option<unsafe extern "C" fn(*mut c_void, u32)>,
}

/// Nodes and devices share this shape: listen, subscribe, enumerate, set.
#[repr(C)]
pub struct ParamMethods<E> {
    pub version: u32,
    pub add_listener:
        Option<unsafe extern "C" fn(*mut c_void, *mut SpaHook, *const E, *mut c_void) -> c_int>,
    pub subscribe_params: Option<unsafe extern "C" fn(*mut c_void, *mut u32, u32) -> c_int>,
    pub enum_params: *const c_void,
    pub set_param: Option<unsafe extern "C" fn(*mut c_void, u32, u32, *const SpaPod) -> c_int>,
}

#[repr(C)]
pub struct NodeInfo {
    pub id: u32,
    pub max_input_ports: u32,
    pub max_output_ports: u32,
    pub change_mask: u64,
    pub n_input_ports: u32,
    pub n_output_ports: u32,
    pub state: c_int,
    pub error: *const c_char,
    pub props: *const SpaDict,
    pub params: *const c_void,
    pub n_params: u32,
}

#[repr(C)]
pub struct NodeEvents {
    pub version: u32,
    pub info: Option<unsafe extern "C" fn(*mut c_void, *const NodeInfo)>,
    pub param: Option<unsafe extern "C" fn(*mut c_void, c_int, u32, u32, u32, *const SpaPod)>,
}

#[repr(C)]
pub struct DeviceEvents {
    pub version: u32,
    pub info: Option<unsafe extern "C" fn(*mut c_void, *const c_void)>,
    pub param: Option<unsafe extern "C" fn(*mut c_void, c_int, u32, u32, u32, *const SpaPod)>,
}

#[repr(C)]
pub struct MetadataMethods {
    pub version: u32,
    pub add_listener: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *mut SpaHook,
            *const MetadataEvents,
            *mut c_void,
        ) -> c_int,
    >,
    pub set_property: Option<
        unsafe extern "C" fn(
            *mut c_void,
            u32,
            *const c_char,
            *const c_char,
            *const c_char,
        ) -> c_int,
    >,
}

#[repr(C)]
pub struct MetadataEvents {
    pub version: u32,
    pub property: Option<
        unsafe extern "C" fn(
            *mut c_void,
            u32,
            *const c_char,
            *const c_char,
            *const c_char,
        ) -> c_int,
    >,
}

#[repr(C)]
pub struct StreamEvents {
    pub version: u32,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub state_changed: Option<unsafe extern "C" fn(*mut c_void, c_int, c_int, *const c_char)>,
    pub control_info: Option<unsafe extern "C" fn(*mut c_void, u32, *const c_void)>,
    pub io_changed: Option<unsafe extern "C" fn(*mut c_void, u32, *mut c_void, u32)>,
    pub param_changed: Option<unsafe extern "C" fn(*mut c_void, u32, *const SpaPod)>,
    pub add_buffer: Option<unsafe extern "C" fn(*mut c_void, *mut PwBuffer)>,
    pub remove_buffer: Option<unsafe extern "C" fn(*mut c_void, *mut PwBuffer)>,
    pub process: Option<unsafe extern "C" fn(*mut c_void)>,
    pub drained: Option<unsafe extern "C" fn(*mut c_void)>,
}

#[repr(C)]
pub struct PwBuffer {
    pub buffer: *mut SpaBuffer,
    pub user_data: *mut c_void,
    pub size: u64,
}

#[repr(C)]
pub struct SpaBuffer {
    pub n_metas: u32,
    pub n_datas: u32,
    pub metas: *mut c_void,
    pub datas: *mut SpaData,
}

#[repr(C)]
pub struct SpaData {
    pub type_: u32,
    pub flags: u32,
    pub fd: i64,
    pub mapoffset: u32,
    pub maxsize: u32,
    pub data: *mut c_void,
    pub chunk: *mut SpaChunk,
}

#[repr(C)]
pub struct SpaChunk {
    pub offset: u32,
    pub size: u32,
    pub stride: i32,
    pub flags: i32,
}

/// The table of an interface an object pointer stands for, and the pointer
/// its methods take — the two halves of every `spa_interface_call`.
///
/// # Safety
/// `object` must be a live PipeWire object whose interface uses `T` as its
/// method table.
pub unsafe fn methods<'a, T>(object: *mut c_void) -> Option<(&'a T, *mut c_void)> {
    if object.is_null() {
        return None;
    }
    // SAFETY: the caller promises `object` is a spa_interface of this type.
    unsafe {
        let interface = &*(object as *const SpaInterface);
        let table = interface.cb.funcs as *const T;
        (!table.is_null()).then(|| (&*table, interface.cb.data))
    }
}

type Init = unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char);
type MainLoopNew = unsafe extern "C" fn(*const SpaDict) -> *mut c_void;
type MainLoopGetLoop = unsafe extern "C" fn(*mut c_void) -> *mut PwLoop;
type MainLoopRun = unsafe extern "C" fn(*mut c_void) -> c_int;
type MainLoopQuit = unsafe extern "C" fn(*mut c_void) -> c_int;
type MainLoopDestroy = unsafe extern "C" fn(*mut c_void);
type ContextNew = unsafe extern "C" fn(*mut PwLoop, *mut c_void, usize) -> *mut c_void;
type ContextDestroy = unsafe extern "C" fn(*mut c_void);
type ContextConnect = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void;
type CoreDisconnect = unsafe extern "C" fn(*mut c_void) -> c_int;
type ProxyDestroy = unsafe extern "C" fn(*mut c_void);
type PropertiesNewDict = unsafe extern "C" fn(*const SpaDict) -> *mut c_void;
type StreamNew = unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void) -> *mut c_void;
type StreamAddListener =
    unsafe extern "C" fn(*mut c_void, *mut SpaHook, *const StreamEvents, *mut c_void);
type StreamConnect =
    unsafe extern "C" fn(*mut c_void, u32, u32, u32, *mut *const SpaPod, u32) -> c_int;
type StreamDestroy = unsafe extern "C" fn(*mut c_void);
type StreamDequeue = unsafe extern "C" fn(*mut c_void) -> *mut PwBuffer;
type StreamQueue = unsafe extern "C" fn(*mut c_void, *mut PwBuffer) -> c_int;

/// libpipewire, opened, with the functions this backend calls.
pub struct Pw {
    pub init: Init,
    pub main_loop_new: MainLoopNew,
    pub main_loop_get_loop: MainLoopGetLoop,
    pub main_loop_run: MainLoopRun,
    pub main_loop_quit: MainLoopQuit,
    pub main_loop_destroy: MainLoopDestroy,
    pub context_new: ContextNew,
    pub context_destroy: ContextDestroy,
    pub context_connect: ContextConnect,
    pub core_disconnect: CoreDisconnect,
    pub proxy_destroy: ProxyDestroy,
    pub properties_new_dict: PropertiesNewDict,
    pub stream_new: StreamNew,
    pub stream_add_listener: StreamAddListener,
    pub stream_connect: StreamConnect,
    pub stream_destroy: StreamDestroy,
    pub stream_dequeue_buffer: StreamDequeue,
    pub stream_queue_buffer: StreamQueue,
    /// Kept open for as long as the pointers above are.
    _library: Library,
}

/// Where a system keeps its libraries, for a build whose own loader does
/// not look there: a binary linked in a Nix shell searches only its store
/// paths, and a distro's libpipewire sits in /usr/lib.
const SYSTEM_LIBRARY_DIRS: &[&str] = &[
    "/usr/lib",
    "/usr/lib64",
    "/usr/lib/x86_64-linux-gnu",
    "/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/lib/aarch64-linux-gnu",
    "/run/current-system/sw/lib",
];

/// libpipewire by its soname, as the loader finds it, or else from the
/// system's library folders.
fn open_library() -> Result<Library, String> {
    const SONAME: &str = "libpipewire-0.3.so.0";
    // SAFETY: loading libpipewire runs no initialisation beyond its own
    // constructors, which only register types.
    let first = match unsafe { Library::new(SONAME) } {
        Ok(library) => return Ok(library),
        Err(error) => error,
    };
    for dir in SYSTEM_LIBRARY_DIRS {
        let path = std::path::Path::new(dir).join(SONAME);
        if !path.exists() {
            continue;
        }
        // SAFETY: as above.
        if let Ok(library) = unsafe { Library::new(&path) } {
            return Ok(library);
        }
    }
    Err(format!("libpipewire-0.3 is not available: {first}"))
}

impl Pw {
    /// Opens the library, or explains why not.
    pub fn open() -> Result<Self, String> {
        // SAFETY: loading libpipewire runs no initialisation beyond its own
        // constructors, which only register types.
        let library = open_library()?;
        macro_rules! symbol {
            ($name:literal) => {
                // SAFETY: the type is the function's documented C signature.
                *unsafe { library.get(concat!($name, "\0").as_bytes()) }
                    .map_err(|error| format!("libpipewire lacks {}: {error}", $name))?
            };
        }
        Ok(Self {
            init: symbol!("pw_init"),
            main_loop_new: symbol!("pw_main_loop_new"),
            main_loop_get_loop: symbol!("pw_main_loop_get_loop"),
            main_loop_run: symbol!("pw_main_loop_run"),
            main_loop_quit: symbol!("pw_main_loop_quit"),
            main_loop_destroy: symbol!("pw_main_loop_destroy"),
            context_new: symbol!("pw_context_new"),
            context_destroy: symbol!("pw_context_destroy"),
            context_connect: symbol!("pw_context_connect"),
            core_disconnect: symbol!("pw_core_disconnect"),
            proxy_destroy: symbol!("pw_proxy_destroy"),
            properties_new_dict: symbol!("pw_properties_new_dict"),
            stream_new: symbol!("pw_stream_new"),
            stream_add_listener: symbol!("pw_stream_add_listener"),
            stream_connect: symbol!("pw_stream_connect"),
            stream_destroy: symbol!("pw_stream_destroy"),
            stream_dequeue_buffer: symbol!("pw_stream_dequeue_buffer"),
            stream_queue_buffer: symbol!("pw_stream_queue_buffer"),
            _library: library,
        })
    }
}

/// A `spa_dict` borrowed as key/value strings. Entries that are not UTF-8
/// or are null are skipped.
///
/// # Safety
/// `dict` must be null or point at a valid `spa_dict`.
pub unsafe fn dict_entries(dict: *const SpaDict) -> Vec<(String, String)> {
    if dict.is_null() {
        return Vec::new();
    }
    // SAFETY: the caller promises a valid dict with `n_items` items.
    unsafe {
        let dict = &*dict;
        if dict.items.is_null() {
            return Vec::new();
        }
        std::slice::from_raw_parts(dict.items, dict.n_items as usize)
            .iter()
            .filter_map(|item| Some((string(item.key)?, string(item.value)?)))
            .collect()
    }
}

/// A C string as an owned one; `None` for null.
///
/// # Safety
/// `pointer` must be null or a NUL-terminated string.
pub unsafe fn string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: promised by the caller.
    Some(
        unsafe { std::ffi::CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned(),
    )
}
