use std::sync::Arc;

/// Attempts to initialize a software-rendered or fallback WGPU device.
/// Returns None if wgpu cannot find any suitable adapter.
pub fn try_init_wgpu() -> Option<(Arc<wgpu::Device>, wgpu::BindGroupLayout)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

    // Request a fallback adapter (such as WARP or Lavapipe) for maximum headless compatibility
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: true,
    }))
    .ok()?;

    let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        label: None,
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))
    .ok()?;

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
        label: Some("test_node_bind_group_layout"),
    });

    Some((Arc::new(device), layout))
}
