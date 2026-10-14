//! The dmabuf ring, exported and imported for real, against a sink that
//! stands in for the window system.

use std::any::Any;
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use morf_value::present::{BufferSink, Damage, DmabufPlane, SinkBuffer};

use super::dmabuf::{self, FOURCC_ARGB8888, Purpose};
use super::partial_tests::{LOGICAL, desk, frame, largest_difference};
use super::present::BufferRing;
use super::present_link::BufferLink;
use crate::*;

#[derive(Default)]
struct Record {
    /// A duplicate of every plane imported, with its stride and size.
    imported: Vec<(OwnedFd, u32, (u32, u32))>,
    presented: Vec<u32>,
}

struct FakeSink {
    modifiers: Vec<u64>,
    record: Arc<Mutex<Record>>,
}

struct FakeBuffer(u32);

impl SinkBuffer for FakeBuffer {
    fn label(&self) -> u32 {
        self.0
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BufferSink for FakeSink {
    fn modifiers(&self, _fourcc: u32) -> Vec<u64> {
        self.modifiers.clone()
    }

    fn import(
        &mut self,
        plane: DmabufPlane<'_>,
        size: (u32, u32),
        _fourcc: u32,
        _busy: Arc<AtomicBool>,
    ) -> Result<Box<dyn SinkBuffer>, String> {
        let fd = plane.fd.try_clone_to_owned().map_err(|e| e.to_string())?;
        let mut record = self.record.lock().unwrap();
        record.imported.push((fd, plane.stride, size));
        Ok(Box::new(FakeBuffer(record.imported.len() as u32)))
    }

    fn present(&mut self, buffer: &dyn SinkBuffer, _damage: &[Damage]) {
        self.record.lock().unwrap().presented.push(buffer.label());
    }

    fn commit(&mut self) {}

    fn dispatch(&mut self) {}

    fn wait(&mut self, _deadline: Instant) -> bool {
        false
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_frame_goes_out_through_an_exported_dmabuf() {
    require_gpu!();
    let physical = LOGICAL as u32;
    let mut backend = pollster::block_on(WgpuBackend::new(physical, physical)).unwrap();
    if !backend
        .dmabuf
        .as_ref()
        .is_some_and(|support| support.sync_file)
    {
        eprintln!("skipped: this device cannot export dmabufs with sync files");
        return;
    }
    // Linear only, so the exported memory can be read back as rows.
    let ours = dmabuf::modifiers_for_purpose(&backend.device, FOURCC_ARGB8888, Purpose::PRESENT);
    if !ours.contains(&dmabuf::MODIFIER_LINEAR) {
        eprintln!("skipped: this device cannot present through linear dmabufs");
        return;
    }
    let record = Arc::new(Mutex::new(Record::default()));
    let sink = FakeSink {
        modifiers: vec![dmabuf::MODIFIER_LINEAR],
        record: record.clone(),
    };
    let link = BufferLink::connect(&backend.device, backend.dmabuf.as_ref(), Box::new(sink))
        .expect("the device and the sink agree on a modifier");
    backend.buffers = Some(BufferRing::new(
        &backend.device,
        &backend.texture,
        3,
        Some(link),
        Default::default(),
    ));
    let mut engine = RenderEngine::new(backend);
    let desk = desk();
    frame(&mut engine, &desk.scene, desk.root, 120);
    let target = engine.backend_mut().read_pixels();

    let record = record.lock().unwrap();
    let label = *record.presented.last().expect("the frame was presented");
    let (fd, stride, (width, height)) = &record.imported[label as usize - 1];
    assert_eq!((*width, *height), (physical, physical));
    let shown = read_linear(fd, *stride, *height);
    // ARGB8888 is B, G, R, A in memory; the target reads back as RGBA.
    let mut rows = Vec::with_capacity(target.len());
    for row in shown.chunks(*stride as usize) {
        for pixel in row[..*width as usize * 4].chunks(4) {
            rows.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    let (difference, x, y) = largest_difference(&rows, &target, physical);
    assert_eq!(
        difference, 0,
        "the dmabuf differs from the frame at ({x}, {y})"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_deep_frame_goes_out_through_a_ten_bit_dmabuf() {
    use super::depth::{OutputDepth, PresentFormat};
    use super::depth_tests::{STEPS, columns, ten_bit_reds};
    require_gpu!();
    let mut backend = pollster::block_on(WgpuBackend::new(4, 4)).unwrap();
    let ours = dmabuf::modifiers_for_purpose(
        &backend.device,
        dmabuf::FOURCC_XBGR2101010,
        Purpose::PRESENT,
    );
    if !backend.dmabuf.as_ref().is_some_and(|s| s.sync_file)
        || !ours.contains(&dmabuf::MODIFIER_LINEAR)
    {
        eprintln!("skipped: this device cannot present through linear 10-bit dmabufs");
        return;
    }
    let record = Arc::new(Mutex::new(Record::default()));
    let sink = FakeSink {
        modifiers: vec![dmabuf::MODIFIER_LINEAR],
        record: record.clone(),
    };
    backend.set_blend(BlendSpace::Srgb);
    backend.set_opaque_surface(true);
    backend.attach_sink(Box::new(sink)).expect("a link");
    assert!(backend.set_output_depth(OutputDepth::Ten));
    assert_eq!(backend.present, PresentFormat::Ten);
    let mut scene = morf_scene::Scene::new();
    let whole = [DamageRect {
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    }];
    backend.render(&columns(&mut scene), &whole, 120).unwrap();

    let record = record.lock().unwrap();
    let label = *record.presented.last().expect("the frame was presented");
    let (fd, stride, _) = &record.imported[label as usize - 1];
    let shown = read_linear(fd, *stride, 4);
    let reds = ten_bit_reds(&shown[..16]);
    for (red, step) in reds.iter().zip(STEPS) {
        assert!(red.abs_diff(step) <= 1, "{reds:?} for {STEPS:?}");
    }
}

/// The bytes of a linear dmabuf, mapped and copied out.
fn read_linear(fd: &OwnedFd, stride: u32, height: u32) -> Vec<u8> {
    let length = stride as usize * height as usize;
    let mapped = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            length,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            0,
        )
    };
    assert_ne!(mapped, libc::MAP_FAILED, "the dmabuf maps");
    // The fences the frame attached are waited for before reading.
    let mut poll = [libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    }];
    unsafe { libc::poll(poll.as_mut_ptr(), 1, 1000) };
    let bytes = unsafe { std::slice::from_raw_parts(mapped as *const u8, length) }.to_vec();
    unsafe { libc::munmap(mapped, length) };
    bytes
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_capture_exported_and_acquired_back_reads_as_written() {
    require_gpu!();
    let mut backend = pollster::block_on(WgpuBackend::new(16, 16)).unwrap();
    let modifiers = backend.capture_modifiers(dmabuf::FOURCC_XRGB8888);
    if modifiers.is_empty() {
        eprintln!("skipped: this device cannot export capture dmabufs");
        return;
    }
    let image = backend
        .export_capture(8, 8, dmabuf::FOURCC_XRGB8888, &modifiers)
        .expect("an exported capture");
    backend
        .publish_texture("gpu:test", image)
        .expect("the capture is acquired back");
    assert_eq!(backend.published_texture_size("gpu:test"), Some((8, 8)));
    assert!(backend.texture_pixels("gpu:test").is_some());
    assert!(backend.forget_texture("gpu:test"));
}
