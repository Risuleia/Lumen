use std::sync::{Arc, Mutex};

use anyhow::Result;
use wgpu::{TextureView, TextureViewDescriptor};
use windows::{
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::Direct3D11::{IDirect3DDevice, IDirect3DSurface},
    },
    Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D
            },
            Dxgi::IDXGIDevice,
            Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint},
        },
        System::{
            Com::{COINIT_MULTITHREADED, CoInitializeEx},
            WinRT::{
                Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
                Graphics::Capture::IGraphicsCaptureItemInterop,
            },
        }, UI::WindowsAndMessaging::GetWindowRect,
    },
};
use windows_core::{IInspectable, Interface};

use crate::utils::import_d3d11_texture_to_wgpu;

pub struct CaptureState {
    pub d3d_device: ID3D11Device,
    pub winrt_device: IDirect3DDevice,
    pub item: GraphicsCaptureItem,
    pub frame_pool: Direct3D11CaptureFramePool,
    pub session: GraphicsCaptureSession,
    pub latest_frame: Arc<Mutex<Option<ID3D11Texture2D>>>,
}

impl CaptureState {
    pub fn new_primary_monitor() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        let mut d3d_device: Option<ID3D11Device> = None;
        let mut _context: Option<ID3D11DeviceContext> = None;
        let mut feature_level = windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL(0);

        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_device as *mut _),
                Some(&mut feature_level as *mut _),
                Some(&mut _context as *mut _),
            )?;
        }

        let d3d_device = d3d_device.expect("D3D11 device must exist");
        let dxgi = d3d_device.cast::<IDXGIDevice>()?;

        let winrt_device = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? };
        let winrt_device = winrt_device.cast::<IDirect3DDevice>()?;

        let item = Self::primary_monitor_item()?;

        let latest_frame = Arc::new(Mutex::new(None));

        let size = item.Size()?;

        let frame_pool = Direct3D11CaptureFramePool::Create(
            &winrt_device,
            windows::Graphics::DirectX::DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )?;

        let session = frame_pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsBorderRequired(false);
        let _ = session.SetIsCursorCaptureEnabled(false);

        let latest = latest_frame.clone();

        frame_pool.FrameArrived(&windows::Foundation::TypedEventHandler::new({
            move |pool: windows_core::Ref<'_, Direct3D11CaptureFramePool>,
                  _args: windows_core::Ref<'_, IInspectable>| {
                if let Some(p) = pool.as_ref() {
                    if let Ok(frame) = p.TryGetNextFrame() {
                        if let Ok(surface) = frame.Surface() {
                            if let Ok(tex) = Self::surface_to_d3dtex(&surface) {
                                let mut guard = latest.lock().unwrap();
                                *guard = Some(tex);
                            }
                        }
                    }
                }

                Ok(())
            }
        }))?;

        session.StartCapture()?;

        Ok(Self {
            d3d_device,
            winrt_device,
            item,
            frame_pool,
            session,
            latest_frame,
        })
    }

    pub fn to_wgpu_view(&self, device: &wgpu::Device, tex: &ID3D11Texture2D) -> TextureView {
        let wgpu_tex = import_d3d11_texture_to_wgpu(device, tex).expect("Failed to import texture");

        wgpu_tex.create_view(&TextureViewDescriptor::default())
    }

    fn primary_monitor_item() -> Result<GraphicsCaptureItem> {
        let monitor_handle =
            unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(monitor_handle)? };
        Ok(item)
    }

    fn surface_to_d3dtex(surface: &IDirect3DSurface) -> Result<ID3D11Texture2D> {
        let access = surface.cast::<IDirect3DDxgiInterfaceAccess>()?;
        let texture = unsafe { access.GetInterface::<ID3D11Texture2D>()? };
        Ok(texture)
    }

    fn crop_texture_to_window(
        d3d_device: &ID3D11Device,
        src_tex: &ID3D11Texture2D,
        hwnd: HWND,
    ) -> Result<ID3D11Texture2D> {
        unsafe {
            let context = d3d_device.GetImmediateContext()?;

            // 1. Get current window position
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect)?;

            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;

            // 2. Create a destination texture of the window's size
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            src_tex.GetDesc(&mut desc); // Copy monitor format (BGRA8, etc)
            desc.Width = width;
            desc.Height = height;
            desc.BindFlags = D3D11_BIND_SHADER_RESOURCE.0 as u32 | D3D11_BIND_RENDER_TARGET.0 as u32;

            let mut dst_tex = None;
            d3d_device.CreateTexture2D(&desc, None, Some(&mut dst_tex))?;
            let dst_tex = dst_tex.unwrap();

            // 3. Define the region to copy (the "hole" where your window is)
            // Note: Coordinates must be clamped to monitor bounds to avoid crashes
            let region = D3D11_BOX {
                left: rect.left.max(0) as u32,
                top: rect.top.max(0) as u32,
                front: 0,
                right: rect.right.max(0) as u32,
                bottom: rect.bottom.max(0) as u32,
                back: 1,
            };

            // 4. Perform the crop
            context.CopySubresourceRegion(&dst_tex, 0, 0, 0, 0, src_tex, 0, Some(&region));

            Ok(dst_tex)
        }
    }
}
