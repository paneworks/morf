//! Opening the GPU device: the one instance of the process, and the devices
//! every backend on an adapter shares.

use super::super::backend_types::GpuError;

/// Opens the adapter's device with the dmabuf extensions enabled, when it can.
///
/// `None` is the ordinary case on anything that is not Vulkan, or is Vulkan
/// without the extensions: the caller opens the device the usual way and
/// captures go through shared memory. Enabling extensions is the one thing
/// that has to happen at creation, which is why this reaches into wgpu-hal at
/// all -- everything else dmabuf needs can be done on the device afterwards.
fn open_with_dmabuf(
    instance: &wgpu::Instance,
    adapter: &wgpu::Adapter,
    descriptor: &wgpu::DeviceDescriptor<'_>,
) -> Option<(wgpu::Device, wgpu::Queue, crate::gpu::dmabuf::DmabufSupport)> {
    use crate::gpu::dmabuf;
    use wgpu::hal::Instance as _;
    use wgpu::hal::api::Vulkan;
    if adapter.get_info().backend != wgpu::Backend::Vulkan {
        return None;
    }
    let hal_instance = unsafe { instance.as_hal::<Vulkan>() }?;
    let raw_instance = hal_instance.shared_instance().raw_instance();
    // The same physical device wgpu chose, found again among hal's adapters:
    // hal hands out its own adapter objects, and the one to open is the one
    // whose device is the device wgpu already reported.
    let wanted = adapter.get_info();
    let exposed = unsafe { hal_instance.enumerate_adapters(None) }
        .into_iter()
        .find(|exposed| exposed.info.device == wanted.device && exposed.info.name == wanted.name)?;
    let physical = exposed.adapter.raw_physical_device();
    let extensions = dmabuf::supported_extensions(raw_instance, physical)?;
    let render_node = dmabuf::render_node(raw_instance, physical);
    let sync_file = extensions.contains(&ash::khr::external_semaphore_fd::NAME);
    let open = unsafe {
        exposed.adapter.open_with_callback(
            descriptor.required_features,
            &descriptor.required_limits,
            &descriptor.memory_hints,
            Some(Box::new(
                |args: wgpu::hal::vulkan::CreateDeviceCallbackArgs<'_, '_, '_>| {
                    for extension in &extensions {
                        if !args.extensions.contains(extension) {
                            args.extensions.push(extension);
                        }
                    }
                },
            )),
        )
    }
    .ok()?;
    let queue_family = open.device.queue_family_index();
    let hal_adapter = unsafe { instance.create_adapter_from_hal::<Vulkan>(exposed) };
    let (device, queue) =
        unsafe { hal_adapter.create_device_from_hal::<Vulkan>(open, descriptor) }.ok()?;
    Some((
        device,
        queue,
        dmabuf::DmabufSupport {
            render_node,
            queue_family,
            sync_file,
        },
    ))
}

/// The one GPU instance of the process, which every backend opens its device
/// through.
///
/// One per backend was what the Vulkan loader could not survive. Enumerating
/// the physical devices of a fresh instance unloads the drivers that found
/// none, and the loader edits its lists of instances and drivers as it does —
/// while a device-level call from another thread, such as naming an object
/// for the validation layer, walks those same lists to find its driver. Two
/// outputs coming up at once, or the GPU tests run in parallel, crashed inside
/// the loader. Destroying an instance edits the lists the same way. With one
/// instance, made once and never destroyed, the drivers are sorted out once,
/// before anything has a device to call through.
pub(super) fn shared_instance() -> wgpu::Instance {
    static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
    INSTANCE
        .get_or_init(|| {
            // Vulkan alone when there is a Vulkan GPU: an instance with GL in
            // it also makes an EGL surface for every output, from each
            // output's own thread, and EGL fails that (`BadAccess`) with a
            // panic -- a second screen took the whole shell down even with
            // Vulkan doing the drawing. GL only where there is no Vulkan.
            let make = |backends| {
                let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
                descriptor.backends = backends;
                wgpu::Instance::new(descriptor)
            };
            let vulkan = make(wgpu::Backends::VULKAN);
            let found = ready(vulkan.enumerate_adapters(wgpu::Backends::VULKAN));
            if found.is_empty() {
                make(wgpu::Backends::VULKAN | wgpu::Backends::GL)
            } else {
                vulkan
            }
        })
        .clone()
}

/// A native wgpu future's answer: they are ready when made, and this polls
/// until it is, without a runtime to wait in.
fn ready<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::yield_now();
    }
}

/// One opened GPU device, which every backend on that adapter draws with.
///
/// Opening a device is most of what bringing a surface up costs -- a few
/// hundred milliseconds on a laptop's integrated GPU, for every layer
/// surface, popup and output -- and each one held its own copy of every
/// driver-side structure. The surfaces of one process draw on one GPU, so
/// they share one device and one queue; each keeps its own swapchain,
/// targets, pipelines and atlases.
#[derive(Clone)]
pub(crate) struct SharedDevice {
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) dmabuf: Option<crate::gpu::dmabuf::DmabufSupport>,
    pub(super) lcd_supported: bool,
}

/// The devices opened so far, one per adapter.
///
/// Usually one. A second appears only when a surface cannot be presented
/// from an adapter already open -- an output wired to another GPU -- which
/// is asked of each adapter before its device is reused.
fn shared_devices() -> &'static std::sync::Mutex<Vec<SharedDevice>> {
    static DEVICES: std::sync::OnceLock<std::sync::Mutex<Vec<SharedDevice>>> =
        std::sync::OnceLock::new();
    DEVICES.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// `MORF_GPU_SHARED=0`: every backend opens a device of its own, as before
/// devices were shared. For measuring the difference.
fn sharing_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| std::env::var("MORF_GPU_SHARED").map_or(true, |value| value != "0"))
}

/// How many devices this process has opened, for tests and diagnostics.
pub fn opened_device_count() -> usize {
    OPENED.load(std::sync::atomic::Ordering::Relaxed)
}

static OPENED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The device to draw `surface` with (or offscreen, without one): an open
/// one whose adapter can present to it, or a new one.
///
/// Two outputs coming up at once may both find nothing to share and open a
/// device each; the second to finish takes the first's instead, and its own
/// is dropped, so the process still ends up with one.
pub(super) async fn device_for(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<SharedDevice, GpuError> {
    if !sharing_wanted() {
        return open_device(instance, surface).await;
    }
    let reusable = |devices: &[SharedDevice]| {
        devices
            .iter()
            .find(|shared| {
                surface.is_none_or(|surface| shared.adapter.is_surface_supported(surface))
            })
            .cloned()
    };
    let lock = || {
        shared_devices()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    let found = reusable(&lock());
    if let Some(shared) = found {
        return Ok(shared);
    }
    let opened = open_device(instance, surface).await?;
    let mut devices = lock();
    if let Some(shared) = reusable(&devices) {
        return Ok(shared);
    }
    devices.push(opened.clone());
    Ok(opened)
}

/// Says so, once and loudly, when the GPU found is not a Vulkan one on real
/// hardware. The machine then has no Vulkan driver for its GPU (only another
/// vendor's, or none), and morf falls back to OpenGL -- which cannot drive
/// several outputs from their own threads -- or to a CPU renderer, which
/// draws every pixel of every screen on the processor. Nothing else would
/// tell the person why the shell is slow, hot or did not start.
fn warn_if_not_vulkan(info: &wgpu::AdapterInfo) {
    static SAID: std::sync::Once = std::sync::Once::new();
    let software = info.device_type == wgpu::DeviceType::Cpu;
    if info.backend == wgpu::Backend::Vulkan && !software {
        return;
    }
    SAID.call_once(|| {
        eprintln!(
            "morf: gpu: drawing with {:?} on {} ({}){} -- no Vulkan driver for this \
             machine's GPU was found. Install one (vulkan-intel, vulkan-radeon, or \
             nvidia's): without it morf is slow{} and may not start on several screens.",
            info.backend,
            info.name,
            info.driver,
            if software { ", a CPU renderer" } else { "" },
            if software {
                " and burns the processor"
            } else {
                ""
            },
        );
    });
}

async fn open_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<SharedDevice, GpuError> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|error| GpuError(format!("no compatible GPU adapter: {error}")))?;
    warn_if_not_vulkan(&adapter.get_info());
    let adapter_limits = adapter.limits();
    // Subpixel text blends each channel by its own coverage, which takes
    // a second fragment output to the blend unit. Asked for only where
    // the adapter has it; without it text stays greyscale.
    let lcd_supported = std::env::var_os("MORF_NO_DUAL_SOURCE").is_none()
        && adapter
            .features()
            .contains(wgpu::Features::DUAL_SOURCE_BLENDING);
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("morf device"),
        required_features: if lcd_supported {
            wgpu::Features::DUAL_SOURCE_BLENDING
        } else {
            wgpu::Features::empty()
        } | super::super::profile::features(&adapter)
            | (adapter.features() & wgpu::Features::SUBGROUP),
        required_limits: adapter_limits.clone(),
        ..Default::default()
    };
    // Through wgpu-hal when the device can export dmabufs, so the
    // extensions that need enabling at creation are enabled; through wgpu
    // as usual otherwise, which is the same device without them.
    let (device, queue, dmabuf) = match open_with_dmabuf(instance, &adapter, &descriptor) {
        Some((device, queue, support)) => (device, queue, Some(support)),
        None => {
            let (device, queue) = adapter
                .request_device(&descriptor)
                .await
                .map_err(|error| GpuError(format!("could not create GPU device: {error}")))?;
            (device, queue, None)
        }
    };
    // A validation error -- a surface a nested compositor briefly will not
    // configure, say -- is logged and the frame is skipped, rather than a
    // panic that takes the output's thread with it.
    device.on_uncaptured_error(std::sync::Arc::new(|error: wgpu::Error| {
        eprintln!("morf: gpu: {error}");
    }));
    OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(SharedDevice {
        adapter,
        device,
        queue,
        dmabuf,
        lcd_supported,
    })
}
