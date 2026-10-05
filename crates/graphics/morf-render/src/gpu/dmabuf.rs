//! A texture the compositor can draw into, without a copy.
//!
//! A screen capture used to be shared memory: the compositor rendered the
//! frame on the GPU, copied it out to a `wl_shm` buffer, and this engine
//! uploaded it straight back to the GPU to draw it. Twice across the bus for
//! pixels that never needed to leave. This is the other way: an image is
//! created here, exported as a dmabuf, handed to the compositor as the buffer
//! to capture *into*, and drawn from the same memory when the frame is ready.
//!
//! wgpu has no word for any of that, so this speaks Vulkan directly for the
//! three things it needs -- exporting an image, asking what it looks like in
//! memory, and acquiring it back from a foreign queue -- and hands the result
//! to wgpu as a texture it did not create but will happily draw. Everything
//! is behind the extensions being present; on a device without them the
//! shared-memory path is what runs, as it always did.

use std::ffi::CStr;
use std::os::fd::OwnedFd;

use ash::vk;
use wgpu::hal::api::Vulkan;

mod image;

pub(crate) use image::export_for;

/// `DRM_FORMAT_XRGB8888`: little-endian bytes blue, green, red, padding.
pub const FOURCC_XRGB8888: u32 = 0x3432_5258;
/// `DRM_FORMAT_ARGB8888`: the same with alpha where the padding was.
pub const FOURCC_ARGB8888: u32 = 0x3432_5241;
/// `DRM_FORMAT_MOD_LINEAR`: rows in order, no tiling.
pub const MODIFIER_LINEAR: u64 = 0;

/// Intel's tilings, as DRM names them (`fourcc_mod_code(INTEL, n)`).
const INTEL_X_TILED: u64 = (1 << 56) | 1;
const INTEL_Y_TILED: u64 = (1 << 56) | 2;

/// A modifier's name, for a log line.
fn modifier_name(modifier: u64) -> String {
    match modifier {
        MODIFIER_LINEAR => "linear".to_owned(),
        INTEL_X_TILED => "intel-x".to_owned(),
        INTEL_Y_TILED => "intel-y".to_owned(),
        other => format!("{other:#018x}"),
    }
}

pub(crate) fn modifier_names(modifiers: &[u64]) -> String {
    let names: Vec<String> = modifiers
        .iter()
        .map(|modifier| modifier_name(*modifier))
        .collect();
    format!("[{}]", names.join(", "))
}

/// `MORF_PRESENT_MODIFIER=linear|x|y`: present only through buffers of that
/// layout, for telling a layout the compositor misreads from anything else.
pub(crate) fn modifier_wanted(modifier: u64) -> bool {
    match std::env::var("MORF_PRESENT_MODIFIER").as_deref() {
        Ok("linear") => modifier == MODIFIER_LINEAR,
        Ok("x") => modifier == INTEL_X_TILED,
        Ok("y") => modifier == INTEL_Y_TILED,
        _ => true,
    }
}

/// The device extensions dmabuf export needs, and whether each is required.
///
/// The first four are what makes an exportable image possible at all. The
/// fifth says which DRM node the device is, so a compositor's offer can be
/// checked against it: a buffer allocated on one GPU and captured into by
/// another is a buffer full of noise. Optional, because a device without it
/// can still export -- it just cannot prove it is the right one. The sixth
/// turns a finished frame into a sync file a dmabuf can carry, which is what
/// presenting through buffers of this engine's own needs (`present`); a
/// device without it presents through the swapchain.
pub(crate) const EXTENSIONS: [(&CStr, bool); 6] = [
    (ash::khr::external_memory_fd::NAME, true),
    (ash::ext::external_memory_dma_buf::NAME, true),
    (ash::ext::image_drm_format_modifier::NAME, true),
    (ash::ext::queue_family_foreign::NAME, true),
    (ash::ext::physical_device_drm::NAME, false),
    (ash::khr::external_semaphore_fd::NAME, false),
];

/// What the device turned out to be able to do.
#[derive(Clone, Debug)]
pub struct DmabufSupport {
    /// The render node this device is, as major and minor, when it said.
    pub render_node: Option<(u32, u32)>,
    /// The queue family every wgpu command runs on.
    pub queue_family: u32,
    /// Whether a semaphore can be exported as a sync file, so a frame drawn
    /// into an exported image can say when it is finished.
    pub sync_file: bool,
}

/// One plane of an exported image: the file descriptor and where the pixels
/// sit behind it.
#[derive(Debug)]
pub struct DmabufPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
}

/// An image exported as a dmabuf, and the wgpu texture that reads it.
///
/// The Vulkan image and its memory belong to the texture: wgpu destroys them
/// through the drop callback it was given, after every command that touched
/// the texture has finished. Nothing here needs a destructor of its own.
pub struct DmabufImage {
    pub width: u32,
    pub height: u32,
    pub fourcc: u32,
    pub modifier: u64,
    pub plane: DmabufPlane,
    pub texture: wgpu::Texture,
    pub(crate) raw: vk::Image,
}

/// The extensions this physical device offers, out of [`EXTENSIONS`].
///
/// Returns them only when every required one is there: a device with export
/// but no modifiers cannot say what it exported, and a partial set is no set.
pub(crate) fn supported_extensions(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
) -> Option<Vec<&'static CStr>> {
    let available = unsafe { instance.enumerate_device_extension_properties(physical) }.ok()?;
    let has = |wanted: &CStr| {
        available.iter().any(|extension| {
            extension
                .extension_name_as_c_str()
                .is_ok_and(|name| name == wanted)
        })
    };
    let mut enabled = Vec::new();
    for (name, required) in EXTENSIONS {
        if has(name) {
            enabled.push(name);
        } else if required {
            return None;
        }
    }
    Some(enabled)
}

/// Which DRM render node a physical device is, from `VK_EXT_physical_device_drm`.
pub(crate) fn render_node(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
) -> Option<(u32, u32)> {
    let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut drm);
    unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
    (drm.has_render == vk::TRUE).then_some((drm.render_major as u32, drm.render_minor as u32))
}

/// Splits a Linux `dev_t` the way the kernel packs it.
///
/// Not `major = dev >> 8`: the modern encoding scatters both numbers across
/// the word so that old programs keep working, and a device number read the
/// old way names the wrong node on any machine with more than a few.
pub fn split_dev_t(dev: u64) -> (u32, u32) {
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
    (major as u32, minor as u32)
}

/// The Vulkan format a DRM fourcc is drawn as, for the two a capture offers.
fn vulkan_format(fourcc: u32) -> Option<vk::Format> {
    match fourcc {
        FOURCC_XRGB8888 | FOURCC_ARGB8888 => Some(vk::Format::B8G8R8A8_UNORM),
        _ => None,
    }
}

/// The modifiers this device can export `format` with, single-plane only.
///
/// A modifier with a second memory plane -- Intel's compression modifiers,
/// for one -- would need a second file descriptor, and a capture protocol
/// that takes one buffer with one plane per format cannot carry it. Those are
/// left out rather than half-handled; what remains still includes the tiled
/// layouts, which is where the speed is.
fn exportable_modifiers(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    format: vk::Format,
    wanted: vk::FormatFeatureFlags,
) -> Vec<u64> {
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default();
    let mut properties = vk::FormatProperties2::default().push_next(&mut list);
    unsafe { instance.get_physical_device_format_properties2(physical, format, &mut properties) };
    let count = list.drm_format_modifier_count as usize;
    if count == 0 {
        return Vec::new();
    }
    let mut entries = vec![vk::DrmFormatModifierPropertiesEXT::default(); count];
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default()
        .drm_format_modifier_properties(&mut entries);
    let mut properties = vk::FormatProperties2::default().push_next(&mut list);
    unsafe { instance.get_physical_device_format_properties2(physical, format, &mut properties) };
    entries
        .iter()
        .filter(|entry| entry.drm_format_modifier_plane_count == 1)
        .filter(|entry| entry.drm_format_modifier_tiling_features.contains(wanted))
        .map(|entry| entry.drm_format_modifier)
        .collect()
}

/// The device's exportable modifiers for a fourcc, through wgpu's device.
pub(crate) fn modifiers_for(device: &wgpu::Device, fourcc: u32) -> Vec<u64> {
    modifiers_for_purpose(device, fourcc, Purpose::CAPTURE)
}

/// The device's exportable modifiers for a fourcc, for images made for
/// `purpose`.
pub(crate) fn modifiers_for_purpose(
    device: &wgpu::Device,
    fourcc: u32,
    purpose: Purpose,
) -> Vec<u64> {
    let Some(format) = vulkan_format(fourcc) else {
        return Vec::new();
    };
    let Some(hal) = (unsafe { device.as_hal::<Vulkan>() }) else {
        return Vec::new();
    };
    exportable_modifiers(
        hal.shared_instance().raw_instance(),
        hal.raw_physical_device(),
        format,
        purpose.features,
    )
}

/// What an exported image is for, which decides how it is made and what
/// wgpu is told about it.
#[derive(Clone, Copy)]
pub(crate) struct Purpose {
    label: &'static str,
    /// The format features a modifier must offer to be chosen.
    features: vk::FormatFeatureFlags,
    usage: vk::ImageUsageFlags,
    hal_usage: wgpu::wgt::TextureUses,
    wgpu_usage: wgpu::TextureUsages,
    /// The state wgpu starts tracking the texture in.
    initial: wgpu::wgt::TextureUses,
    /// Other formats it may be viewed as.
    view_formats: &'static [wgpu::TextureFormat],
}

impl Purpose {
    /// Filled by the compositor, then sampled here. `STORAGE_READ_ONLY` as
    /// the starting state is deliberate: it is the one that maps to
    /// `GENERAL`, the layout an image filled from outside is in, and the only
    /// old layout a first barrier may name without being allowed to throw the
    /// contents away.
    pub(crate) const CAPTURE: Self = Self {
        label: "morf dmabuf capture",
        features: vk::FormatFeatureFlags::from_raw(
            vk::FormatFeatureFlags::SAMPLED_IMAGE.as_raw()
                | vk::FormatFeatureFlags::TRANSFER_DST.as_raw()
                | vk::FormatFeatureFlags::TRANSFER_SRC.as_raw(),
        ),
        usage: vk::ImageUsageFlags::from_raw(
            vk::ImageUsageFlags::SAMPLED.as_raw()
                | vk::ImageUsageFlags::TRANSFER_DST.as_raw()
                | vk::ImageUsageFlags::TRANSFER_SRC.as_raw(),
        ),
        hal_usage: wgpu::wgt::TextureUses::from_bits_truncate(
            wgpu::wgt::TextureUses::RESOURCE.bits()
                | wgpu::wgt::TextureUses::COPY_DST.bits()
                | wgpu::wgt::TextureUses::COPY_SRC.bits(),
        ),
        wgpu_usage: wgpu::TextureUsages::from_bits_truncate(
            wgpu::TextureUsages::TEXTURE_BINDING.bits()
                | wgpu::TextureUsages::COPY_DST.bits()
                | wgpu::TextureUsages::COPY_SRC.bits(),
        ),
        initial: wgpu::wgt::TextureUses::STORAGE_READ_ONLY,
        view_formats: &[wgpu::TextureFormat::Bgra8UnormSrgb],
    };

    /// Drawn into here, then shown by the compositor: a surface's buffer.
    /// It starts uninitialised, since its first frame is drawn in full.
    pub(crate) const PRESENT: Self = Self {
        label: "morf dmabuf present",
        features: vk::FormatFeatureFlags::COLOR_ATTACHMENT,
        usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
        hal_usage: wgpu::wgt::TextureUses::COLOR_TARGET,
        wgpu_usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        initial: wgpu::wgt::TextureUses::UNINITIALIZED,
        // Written through its own format only: the composite copies bytes
        // that are already encoded, so no sRGB view (and no mutable-format
        // image, which not every modifier allows) is needed.
        view_formats: &[],
    };
}

/// Creates an image the compositor can capture into, exported as a dmabuf.
///
/// `offered` is the compositor's modifier list for `fourcc`; the first of the
/// device's own that the compositor also accepts is used, in the device's
/// order, because the device lists what it draws fastest first.
pub fn export(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    fourcc: u32,
    offered: &[u64],
) -> Result<DmabufImage, String> {
    export_for(device, (width, height), fourcc, offered, Purpose::CAPTURE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_t_splits_the_way_the_kernel_packs_it() {
        // /dev/dri/renderD128 is 226:128. Packed old-style that is 0xe280;
        // packed the modern way the minor's high byte moves out past bit 20.
        assert_eq!(split_dev_t(0xe280), (226, 128));
        assert_eq!(split_dev_t(0xe281), (226, 129));
        // A minor above 255: 226:300 packs as (300 & 0xff) | (300 >> 8 << 20)
        // with the major in bits 8..20.
        let packed = (226u64 << 8) | (300 & 0xff) | ((300u64 >> 8) << 20);
        assert_eq!(split_dev_t(packed), (226, 300));
    }

    #[test]
    fn only_the_two_capture_formats_have_a_vulkan_face() {
        assert_eq!(
            vulkan_format(FOURCC_XRGB8888),
            Some(vk::Format::B8G8R8A8_UNORM)
        );
        assert_eq!(
            vulkan_format(FOURCC_ARGB8888),
            Some(vk::Format::B8G8R8A8_UNORM)
        );
        assert_eq!(
            vulkan_format(0x3231_3652),
            None,
            "RG16 is not a capture format"
        );
    }
}
