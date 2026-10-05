//! The exported image itself: created with a modifier both sides accept,
//! its memory bound and exported, and handed to wgpu as a texture.

use std::os::fd::{FromRawFd, OwnedFd};

use ash::vk;
use wgpu::hal::api::Vulkan;

use super::{DmabufImage, DmabufPlane, Purpose, exportable_modifiers, vulkan_format};

/// Creates an image exported as a dmabuf, made for `purpose`.
pub(crate) fn export_for(
    device: &wgpu::Device,
    (width, height): (u32, u32),
    fourcc: u32,
    offered: &[u64],
    purpose: Purpose,
) -> Result<DmabufImage, String> {
    let format =
        vulkan_format(fourcc).ok_or_else(|| format!("no Vulkan format for fourcc {fourcc:#x}"))?;
    let hal = unsafe { device.as_hal::<Vulkan>() }.ok_or("the device is not Vulkan")?;
    let raw = hal.raw_device();
    let instance = hal.shared_instance().raw_instance();
    let physical = hal.raw_physical_device();
    let queue_family = hal.queue_family_index();

    let candidates = exportable_modifiers(instance, physical, format, purpose.features);
    let modifiers = candidates
        .iter()
        .copied()
        .filter(|modifier| offered.contains(modifier))
        .collect::<Vec<_>>();
    if modifiers.is_empty() {
        return Err("the compositor and the GPU agree on no modifier".to_owned());
    }

    // The image: tiled by whichever modifier the driver picks from the list,
    // and marked from birth as one that will be exported.
    let mut modifier_list =
        vk::ImageDrmFormatModifierListCreateInfoEXT::default().drm_format_modifiers(&modifiers);
    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(purpose.usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut modifier_list)
        .push_next(&mut external);
    let image = unsafe { raw.create_image(&image_info, None) }
        .map_err(|error| format!("could not create an exportable image: {error}"))?;

    // Its memory: dedicated, because an exported allocation is one the driver
    // wants to own outright, and exportable as a dmabuf.
    let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
    let mut requirements =
        vk::MemoryRequirements2::default().push_next(&mut dedicated_requirements);
    unsafe {
        raw.get_image_memory_requirements2(
            &vk::ImageMemoryRequirementsInfo2::default().image(image),
            &mut requirements,
        )
    };
    let requirements = requirements.memory_requirements;
    let memory_properties = unsafe { instance.get_physical_device_memory_properties(physical) };
    let memory_type = (0..memory_properties.memory_type_count)
        .find(|&index| {
            requirements.memory_type_bits & (1 << index) != 0
                && memory_properties.memory_types[index as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .or_else(|| {
            (0..memory_properties.memory_type_count)
                .find(|&index| requirements.memory_type_bits & (1 << index) != 0)
        });
    let Some(memory_type) = memory_type else {
        unsafe { raw.destroy_image(image, None) };
        return Err("no memory type can back an exportable image".to_owned());
    };
    let mut export_info = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type)
        .push_next(&mut export_info)
        .push_next(&mut dedicated);
    let memory = match unsafe { raw.allocate_memory(&allocate_info, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe { raw.destroy_image(image, None) };
            return Err(format!("could not allocate exportable memory: {error}"));
        }
    };
    if let Err(error) = unsafe { raw.bind_image_memory(image, memory, 0) } {
        unsafe {
            raw.free_memory(memory, None);
            raw.destroy_image(image, None);
        }
        return Err(format!("could not bind exportable memory: {error}"));
    }

    // What it looks like from outside: which modifier the driver chose, and
    // the plane's offset and stride, which is what a wl_buffer is made of.
    let modifier_device = ash::ext::image_drm_format_modifier::Device::new(instance, raw);
    let mut chosen = vk::ImageDrmFormatModifierPropertiesEXT::default();
    let modifier = match unsafe {
        modifier_device.get_image_drm_format_modifier_properties(image, &mut chosen)
    } {
        Ok(()) => {
            if std::env::var_os("MORF_GPU_LOG").is_some() {
                eprintln!(
                    "morf: gpu: {} buffer {width}x{height}: modifier {}",
                    purpose.label,
                    super::modifier_names(&[chosen.drm_format_modifier])
                );
            }
            chosen.drm_format_modifier
        }
        Err(error) => {
            unsafe {
                raw.free_memory(memory, None);
                raw.destroy_image(image, None);
            }
            return Err(format!(
                "the driver would not say which modifier it used: {error}"
            ));
        }
    };
    let layout = unsafe {
        raw.get_image_subresource_layout(
            image,
            vk::ImageSubresource::default()
                .aspect_mask(vk::ImageAspectFlags::MEMORY_PLANE_0_EXT)
                .mip_level(0)
                .array_layer(0),
        )
    };
    let fd_device = ash::khr::external_memory_fd::Device::new(instance, raw);
    let fd = match unsafe {
        fd_device.get_memory_fd(
            &vk::MemoryGetFdInfoKHR::default()
                .memory(memory)
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
        )
    } {
        Ok(fd) => unsafe { OwnedFd::from_raw_fd(fd) },
        Err(error) => {
            unsafe {
                raw.free_memory(memory, None);
                raw.destroy_image(image, None);
            }
            return Err(format!("could not export the image as a dmabuf: {error}"));
        }
    };

    // And as a wgpu texture. The Vulkan objects go to wgpu with a callback
    // that destroys them, so their life is the texture's and ends after the
    // last command that read it. The state it starts in is the purpose's.
    let raw_for_drop = raw.clone();
    let hal_texture = unsafe {
        hal.texture_from_raw(
            image,
            &wgpu::hal::TextureDescriptor {
                label: Some(purpose.label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: purpose.hal_usage,
                memory_flags: wgpu::hal::MemoryFlags::empty(),
                view_formats: purpose.view_formats.to_vec(),
            },
            // Called by wgpu after the last command that used the texture
            // has completed, and the handles are ours: inside the enclosing
            // unsafe block, which is what makes the calls permitted.
            Some(Box::new(move || {
                raw_for_drop.free_memory(memory, None);
                raw_for_drop.destroy_image(image, None);
            })),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };
    drop(hal);
    let texture = unsafe {
        device.create_texture_from_hal::<Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(purpose.label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: purpose.wgpu_usage,
                view_formats: purpose.view_formats,
            },
            purpose.initial,
        )
    };
    let _ = queue_family;
    Ok(DmabufImage {
        width,
        height,
        fourcc,
        modifier,
        plane: DmabufPlane {
            fd,
            offset: layout.offset as u32,
            stride: layout.row_pitch as u32,
        },
        texture,
        raw: image,
    })
}
