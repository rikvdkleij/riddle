use crate::wgpu_ext::*;

/// A standard Renderer WGPU device used with `riddle_platform_winit` windows.
///
/// While this can be used directly, if using [`WGPURenderer::new_from_window`], this
/// type shouldn't need to be used by consumer code.
pub struct WindowWGPUDevice {
    window: WindowHandle,
    window_event_sub: EventSub<PlatformEvent>,

    device: wgpu::Device,
    surface: wgpu::Surface,
    queue: wgpu::Queue,

    swap_chain: Mutex<wgpu::SwapChain>,
    current_frame: Mutex<Option<wgpu::SwapChainFrame>>,
}

impl WindowWGPUDevice {
    pub fn new(window: &Window) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };

        let adapter =
            futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::Default,
                compatible_surface: Some(&surface),
            }))
            .ok_or(RendererError::APIInitError("Failed to get WGPU adapter"))?;

        let (device, queue) = futures::executor::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                shader_validation: true,
                ..Default::default()
            },
            None,
        ))
        .map_err(|_| RendererError::APIInitError("Failed to create WGPU device"))?;

        let (width, height) = window.physical_size();
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width,
            height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        let window_event_sub = EventSub::new();
        window.subscribe_to_events(&window_event_sub);

        Ok(Self {
            window: window.clone_handle(),
            window_event_sub,
            device,
            surface,
            queue,
            swap_chain: Mutex::new(swap_chain),
            current_frame: Mutex::new(None),
        })
    }

    fn handle_window_events(&self) {
        let mut dirty_swap_chain = false;
        for event in self.window_event_sub.collect().iter() {
            match event {
                PlatformEvent::WindowResize(_) => dirty_swap_chain = true,
                _ => (),
            }
        }

        if dirty_swap_chain {
            let (width, height) = self.window.physical_size();
            let sc_desc = wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                width,
                height,
                present_mode: wgpu::PresentMode::Mailbox,
            };

            let swap_chain = self.device.create_swap_chain(&self.surface, &sc_desc);
            *self.swap_chain.lock().unwrap() = swap_chain;
        }
    }

    fn ensure_current_frame(&self) -> Result<()> {
        let mut swap_chain = self.swap_chain.lock().unwrap();
        let mut frame = self.current_frame.lock().unwrap();

        let new_frame = swap_chain
            .get_current_frame()
            .map_err(|_| RendererError::BeginRenderError("Error getting swap chain frame"))?;

        *frame = Some(new_frame);

        Ok(())
    }

    fn present_current_frame(&self) -> () {
        let mut frame = self.current_frame.lock().unwrap();
        *frame = None;
    }
}

impl WGPUDevice for WindowWGPUDevice {
    fn viewport_dimensions(&self) -> Vector2<f32> {
        self.window.logical_size().into()
    }

    fn begin_frame(&self) -> Result<()> {
        self.handle_window_events();
        self.ensure_current_frame()
    }

    fn end_frame(&self) {
        self.present_current_frame()
    }

    #[inline]
    fn with_device_info<R, F>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&WGPUDeviceInfo) -> Result<R>,
    {
        let info = WGPUDeviceInfo {
            device: &self.device,
            queue: &self.queue,
        };
        f(&info)
    }

    #[inline]
    fn with_frame<R, F>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&wgpu::SwapChainFrame) -> Result<R>,
    {
        let frame = self.current_frame.lock().unwrap();
        f(frame.as_ref().unwrap())
    }
}
