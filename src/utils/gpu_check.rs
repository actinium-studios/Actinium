const ACCEPTED_BACKENDS: wgpu::Backends = wgpu::Backends::VULKAN
    .union(wgpu::Backends::METAL) // apple (macs in this case)
    .union(wgpu::Backends::DX12); // every other thing
