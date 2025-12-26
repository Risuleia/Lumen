use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use anyhow::Result;
use wgpu::Texture;
use windows::Win32::{
    Foundation::{GENERIC_ALL, HANDLE},
    Graphics::{
        Direct3D11::ID3D11Texture2D,
        Direct3D12::{ID3D12Device, ID3D12Resource},
        Dxgi::IDXGIResource1,
    },
};
use windows_core::Interface;

pub fn import_d3d11_texture_to_wgpu(
    device: &wgpu::Device,
    tex: &ID3D11Texture2D
) -> Result<Texture> {
    use wgpu::hal::api::Dx12;

    let handle = get_shared_handle(tex)?;
    let handle = HANDLE(handle.as_raw_handle() as _);

    let hal_device = unsafe { device.as_hal::<Dx12>() }
        .ok_or_else(|| anyhow::anyhow!("DX12 backend required"))?;

    let raw_d3d12_device: ID3D12Device = unsafe { hal_device.raw_device().cast()? };

    let d3d12_resource: ID3D12Resource = unsafe {
        let mut resource: Option<ID3D12Resource> = None;
        raw_d3d12_device.OpenSharedHandle(handle, &mut resource)?;
        resource.ok_or_else(|| anyhow::anyhow!("Failed to open shared handle as ID3D12Resource"))?
    };

    let desc = unsafe {
        let mut d = std::mem::zeroed();
        tex.GetDesc(&mut d);
        d
    };

    let width = desc.Width;
    let height = desc.Height;

    let imported = unsafe {
        wgpu::hal::dx12::Device::texture_from_raw(
            d3d12_resource,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureDimension::D2,
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            1,
            1,
        )
    };

    let wgpu_tex = unsafe {
        device.create_texture_from_hal::<Dx12>(
            imported,
            &wgpu::TextureDescriptor {
                label: Some("WGPU Imported Texture"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
        )
    };

    Ok(wgpu_tex)
}

pub fn get_shared_handle(tex: &ID3D11Texture2D) -> Result<OwnedHandle> {
    let dxgi = tex.cast::<IDXGIResource1>()?;

    let handle = unsafe { dxgi.CreateSharedHandle(None, GENERIC_ALL.0, None)? };

    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0 as _) })
}

pub fn align_to(value: u32, align: u32) -> u32 {
    ((value + align - 1) / align) * align
}
