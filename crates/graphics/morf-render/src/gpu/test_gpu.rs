//! Skipping the GPU tests on a machine with no adapter.
//!
//! A test that needs a device starts with `require_gpu!()`. Without an
//! adapter it says so and passes, rather than failing on an unwrap the reader
//! has to trace back to a missing driver.

/// Whether a GPU adapter can be opened, probed once per test process.
pub(crate) fn adapter() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        let found = pollster::block_on(super::WgpuBackend::new(1, 1));
        if let Err(error) = &found {
            eprintln!(
                "morf-render: GPU tests skipped, no adapter ({error}); \
                 run them under nixVulkanIntel"
            );
        }
        found.is_ok()
    })
}

/// Returns from the enclosing test when there is no GPU adapter.
macro_rules! require_gpu {
    () => {
        if !$crate::gpu::test_gpu::adapter() {
            eprintln!("skipped: no GPU adapter");
            return;
        }
    };
}
