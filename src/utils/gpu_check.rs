const ACCEPTED_BACKENDS: wgpu::Backends = wgpu::Backends::VULKAN
    .union(wgpu::Backends::METAL)
    .union(wgpu::Backends::DX12);
