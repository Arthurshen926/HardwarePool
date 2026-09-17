//! Windows Graphics Capture and D3D11 projection.
//!
//! Unsafe code is restricted to documented Windows ABI calls and mapped-row
//! reads. WGC callbacks only update atomics; all GPU/CPU work happens in
//! `MirrorCaptureBackend::poll` on the caller-owned worker thread.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use capyio_display::{DisplayCursorMode, DisplayStreamSpec};
use capyio_video::{VideoColorimetry, VideoPixelFormat};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCapturePicker, GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{E_POINTER, HMODULE, HWND},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                ID3D11Texture2D,
            },
            Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, IDXGIAdapter, IDXGIDevice},
        },
        System::WinRT::Direct3D11::{
            CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
        },
        UI::Shell::IInitializeWithWindow,
    },
    core::{Error as WindowsError, Interface},
};
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus};

use crate::{
    CaptureBackendPoll, CaptureSize, CapturedBgraFrame, MirrorCaptureBackend, WindowsDisplayError,
};

const FRAME_POOL_BUFFERS: i32 = 2;

/// Opaque proof that the production target came from the Windows system picker.
///
/// There is intentionally no public constructor from HWND, HMONITOR,
/// `GraphicsCaptureItem`, WindowId or DisplayId.
pub struct PickerSelectedTarget {
    item: GraphicsCaptureItem,
}

impl PickerSelectedTarget {
    pub fn display_name(&self) -> Result<String, WindowsDisplayError> {
        self.item
            .DisplayName()
            .map(|name| name.to_string())
            .map_err(|error| windows_error("read picker target name", error))
    }

    pub fn size(&self) -> Result<CaptureSize, WindowsDisplayError> {
        capture_size(
            self.item
                .Size()
                .map_err(|error| windows_error("read picker target size", error))?,
        )
    }
}

/// Opens the consent-visible Windows Graphics Capture picker for `owner`.
///
/// # Safety
///
/// `owner` must be a live top-level HWND owned by the calling desktop process,
/// and the call must run from the window's appropriately initialized UI thread.
/// The returned future must be driven without blocking that UI thread.
pub async unsafe fn pick_target_for_window(
    owner: HWND,
) -> Result<PickerSelectedTarget, WindowsDisplayError> {
    if owner.0.is_null() {
        return Err(WindowsDisplayError::InvalidStream(
            "capture picker owner HWND cannot be null".to_owned(),
        ));
    }
    let picker = GraphicsCapturePicker::new()
        .map_err(|error| windows_error("create graphics capture picker", error))?;
    let initializer: IInitializeWithWindow = picker
        .cast()
        .map_err(|error| windows_error("query picker owner-window interface", error))?;
    // SAFETY: upheld by the function's live, process-owned HWND contract.
    unsafe { initializer.Initialize(owner) }
        .map_err(|error| windows_error("initialize capture picker owner", error))?;
    let operation = picker
        .PickSingleItemAsync()
        .map_err(|error| windows_error("open graphics capture picker", error))?;
    match operation.await {
        Ok(item) => Ok(PickerSelectedTarget { item }),
        Err(error) if error.code() == E_POINTER => Err(WindowsDisplayError::PickerCancelled),
        Err(error) => Err(windows_error("select graphics capture target", error)),
    }
}

/// Opens the consent-visible picker and reports completion without requiring
/// the caller's Future to be `Send`.
///
/// This is intended for UI frameworks that require picker creation on their
/// main thread but dispatch asynchronous commands on a `Send` executor.
///
/// # Safety
///
/// The same live, process-owned top-level HWND and UI-thread requirements as
/// [`pick_target_for_window`] apply to the call that starts this operation.
pub unsafe fn begin_pick_target_for_window<F>(
    owner: HWND,
    on_complete: F,
) -> Result<(), WindowsDisplayError>
where
    F: FnOnce(Result<PickerSelectedTarget, WindowsDisplayError>) + Send + 'static,
{
    if owner.0.is_null() {
        return Err(WindowsDisplayError::InvalidStream(
            "capture picker owner HWND cannot be null".to_owned(),
        ));
    }
    let picker = GraphicsCapturePicker::new()
        .map_err(|error| windows_error("create graphics capture picker", error))?;
    let initializer: IInitializeWithWindow = picker
        .cast()
        .map_err(|error| windows_error("query picker owner-window interface", error))?;
    // SAFETY: upheld by the function's live, process-owned HWND contract.
    unsafe { initializer.Initialize(owner) }
        .map_err(|error| windows_error("initialize capture picker owner", error))?;
    let operation = picker
        .PickSingleItemAsync()
        .map_err(|error| windows_error("open graphics capture picker", error))?;
    let mut callback = Some(on_complete);
    operation
        .SetCompleted(&AsyncOperationCompletedHandler::new(
            move |operation, status| {
                let result = match status {
                    AsyncStatus::Canceled => Err(WindowsDisplayError::PickerCancelled),
                    AsyncStatus::Completed => operation
                        .ok()
                        .and_then(windows_future::IAsyncOperation::GetResults)
                        .map(|item| PickerSelectedTarget { item })
                        .map_err(|error| {
                            if error.code() == E_POINTER {
                                WindowsDisplayError::PickerCancelled
                            } else {
                                windows_error("select graphics capture target", error)
                            }
                        }),
                    AsyncStatus::Error => operation
                        .ok()
                        .and_then(windows_future::IAsyncOperation::GetResults)
                        .map(|item| PickerSelectedTarget { item })
                        .map_err(|error| windows_error("select graphics capture target", error)),
                    _ => Err(WindowsDisplayError::Backend(
                        "capture picker completed with an invalid status".to_owned(),
                    )),
                };
                if let Some(callback) = callback.take() {
                    callback(result);
                }
                Ok(())
            },
        ))
        .map_err(|error| windows_error("register capture picker completion", error))?;
    Ok(())
}

struct StagingTexture {
    size: CaptureSize,
    texture: ID3D11Texture2D,
}

/// WGC backend that can only be constructed from a system-picker target.
pub struct WgcCaptureBackend {
    selected_size: CaptureSize,
    item: GraphicsCaptureItem,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
    frame_pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    staging: Option<StagingTexture>,
    frame_ready: Arc<AtomicBool>,
    target_closed: Arc<AtomicBool>,
    frame_arrived_token: Option<i64>,
    target_closed_token: Option<i64>,
    started: bool,
    closed: bool,
}

impl WgcCaptureBackend {
    pub fn is_supported() -> Result<bool, WindowsDisplayError> {
        GraphicsCaptureSession::IsSupported()
            .map_err(|error| windows_error("query Graphics Capture support", error))
    }

    pub fn new(
        target: PickerSelectedTarget,
        selected: &DisplayStreamSpec,
    ) -> Result<Self, WindowsDisplayError> {
        validate_selected(selected)?;
        if !Self::is_supported()? {
            return Err(WindowsDisplayError::CaptureUnsupported);
        }
        let selected_size = CaptureSize {
            width: selected.width,
            height: selected.height,
        };
        let item_size = capture_size(
            target
                .item
                .Size()
                .map_err(|error| windows_error("read initial capture target size", error))?,
        )?;
        if item_size != selected_size {
            return Err(WindowsDisplayError::ReconfigureRequired {
                width: item_size.width,
                height: item_size.height,
            });
        }

        let (device, context, winrt_device) = create_d3d_device()?;
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            FRAME_POOL_BUFFERS,
            size_int32(selected_size)?,
        )
        .map_err(|error| windows_error("create free-threaded WGC frame pool", error))?;
        let session = frame_pool
            .CreateCaptureSession(&target.item)
            .map_err(|error| windows_error("create WGC capture session", error))?;
        session
            .SetIsBorderRequired(true)
            .map_err(|error| windows_error("require the Windows capture border", error))?;
        session
            .SetIsCursorCaptureEnabled(selected.cursor_mode == DisplayCursorMode::Composited)
            .map_err(|error| windows_error("configure cursor capture", error))?;

        let frame_ready = Arc::new(AtomicBool::new(false));
        let callback_ready = Arc::clone(&frame_ready);
        let frame_handler = TypedEventHandler::<
            Direct3D11CaptureFramePool,
            windows::core::IInspectable,
        >::new(move |_, _| {
            callback_ready.store(true, Ordering::Release);
            Ok(())
        });
        let frame_arrived_token = frame_pool
            .FrameArrived(&frame_handler)
            .map_err(|error| windows_error("register WGC frame-arrival callback", error))?;

        let target_closed = Arc::new(AtomicBool::new(false));
        let callback_closed = Arc::clone(&target_closed);
        let close_handler =
            TypedEventHandler::<GraphicsCaptureItem, windows::core::IInspectable>::new(
                move |_, _| {
                    callback_closed.store(true, Ordering::Release);
                    Ok(())
                },
            );
        let target_closed_token = target
            .item
            .Closed(&close_handler)
            .map_err(|error| windows_error("register capture-target close callback", error))?;

        Ok(Self {
            selected_size,
            item: target.item,
            device,
            context,
            winrt_device,
            frame_pool,
            session,
            staging: None,
            frame_ready,
            target_closed,
            frame_arrived_token: Some(frame_arrived_token),
            target_closed_token: Some(target_closed_token),
            started: false,
            closed: false,
        })
    }

    fn read_frame(
        &mut self,
        frame: &Direct3D11CaptureFrame,
    ) -> Result<CaptureBackendPoll, WindowsDisplayError> {
        let content_size = capture_size(
            frame
                .ContentSize()
                .map_err(|error| windows_error("read captured content size", error))?,
        )?;
        if content_size != self.selected_size {
            return Ok(CaptureBackendPoll::SizeChanged(content_size));
        }
        let timestamp = frame
            .SystemRelativeTime()
            .map_err(|error| windows_error("read capture QPC timestamp", error))?
            .Duration;
        let source_timestamp_nanos = u64::try_from(timestamp)
            .ok()
            .and_then(|ticks| ticks.checked_mul(100))
            .ok_or_else(|| {
                WindowsDisplayError::InvalidFrame(
                    "capture timestamp is negative or overflows nanoseconds".to_owned(),
                )
            })?;
        let surface = frame
            .Surface()
            .map_err(|error| windows_error("read captured D3D surface", error))?;
        let access: IDirect3DDxgiInterfaceAccess = surface
            .cast()
            .map_err(|error| windows_error("query captured DXGI surface access", error))?;
        // SAFETY: the WGC IDirect3DSurface is alive for `frame` and the queried
        // IID is the generated ID3D11Texture2D interface identifier.
        let source: ID3D11Texture2D = unsafe { access.GetInterface() }
            .map_err(|error| windows_error("query captured D3D11 texture", error))?;
        let payload = self.copy_bgra_to_packed(&source, content_size)?;
        Ok(CaptureBackendPoll::Frame(CapturedBgraFrame {
            size: content_size,
            source_timestamp_nanos,
            payload,
        }))
    }

    fn copy_bgra_to_packed(
        &mut self,
        source: &ID3D11Texture2D,
        size: CaptureSize,
    ) -> Result<Vec<u8>, WindowsDisplayError> {
        let mut source_desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: `source_desc` is a valid writable descriptor and `source`
        // remains alive for this call.
        unsafe { source.GetDesc(&mut source_desc) };
        if source_desc.Width != size.width
            || source_desc.Height != size.height
            || source_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        {
            return Err(WindowsDisplayError::InvalidFrame(
                "WGC texture does not match the exact BGRA8 content mode".to_owned(),
            ));
        }
        let needs_staging = self
            .staging
            .as_ref()
            .is_none_or(|staging| staging.size != size);
        if needs_staging {
            let mut staging_desc = source_desc;
            staging_desc.Usage = D3D11_USAGE_STAGING;
            staging_desc.BindFlags = 0;
            staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            staging_desc.MiscFlags = 0;
            let mut texture = None;
            // SAFETY: the descriptor is initialized, no initial-data pointer is
            // supplied, and `texture` is a valid out parameter.
            unsafe {
                self.device
                    .CreateTexture2D(&staging_desc, None, Some(&mut texture))
            }
            .map_err(|error| windows_error("create D3D11 staging texture", error))?;
            self.staging = Some(StagingTexture {
                size,
                texture: texture.ok_or_else(|| {
                    WindowsDisplayError::Backend("D3D11 returned a null staging texture".to_owned())
                })?,
            });
        }
        let staging = self.staging.as_ref().ok_or_else(|| {
            WindowsDisplayError::Backend("D3D11 staging texture is unavailable".to_owned())
        })?;
        // SAFETY: source and destination are live textures with identical
        // descriptors; the immediate context is used only on this polling
        // worker.
        unsafe { self.context.CopyResource(&staging.texture, source) };

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: `mapped` is a valid out parameter. The staging texture was
        // created with CPU read access and is not mapped elsewhere.
        unsafe {
            self.context
                .Map(&staging.texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|error| windows_error("map D3D11 staging texture", error))?;

        let copied = copy_mapped_rows(mapped, size);
        // SAFETY: the successful Map above owns subresource zero until this
        // matching Unmap on the same immediate context.
        unsafe { self.context.Unmap(&staging.texture, 0) };
        copied
    }

    fn close_inner(&mut self) -> Result<(), WindowsDisplayError> {
        if self.closed {
            return Ok(());
        }
        let mut first_error = None;
        if let Some(token) = self.frame_arrived_token.take()
            && let Err(error) = self.frame_pool.RemoveFrameArrived(token)
        {
            first_error.get_or_insert_with(|| windows_error("remove frame callback", error));
        }
        if let Some(token) = self.target_closed_token.take()
            && let Err(error) = self.item.RemoveClosed(token)
        {
            first_error.get_or_insert_with(|| windows_error("remove target callback", error));
        }
        if let Err(error) = self.session.Close() {
            first_error.get_or_insert_with(|| windows_error("close WGC session", error));
        }
        if let Err(error) = self.frame_pool.Close() {
            first_error.get_or_insert_with(|| windows_error("close WGC frame pool", error));
        }
        self.staging = None;
        self.started = false;
        self.closed = true;
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl MirrorCaptureBackend for WgcCaptureBackend {
    fn start(&mut self) -> Result<(), WindowsDisplayError> {
        if self.started || self.closed {
            return Err(WindowsDisplayError::Backend(
                "WGC backend cannot be started in its current state".to_owned(),
            ));
        }
        self.session
            .StartCapture()
            .map_err(|error| windows_error("start WGC session", error))?;
        self.started = true;
        Ok(())
    }

    fn poll(&mut self) -> Result<CaptureBackendPoll, WindowsDisplayError> {
        if !self.started || self.closed {
            return Err(WindowsDisplayError::Backend(
                "WGC backend is not running".to_owned(),
            ));
        }
        if self.target_closed.swap(false, Ordering::AcqRel) {
            return Ok(CaptureBackendPoll::TargetClosed);
        }
        if !self.frame_ready.swap(false, Ordering::AcqRel) {
            return Ok(CaptureBackendPoll::Idle);
        }
        let mut latest = None;
        loop {
            match self.frame_pool.TryGetNextFrame() {
                Ok(frame) => {
                    if let Some(previous) = latest.replace(frame) {
                        previous.Close().map_err(|error| {
                            windows_error("return superseded WGC frame to pool", error)
                        })?;
                    }
                }
                // TryGetNextFrame has a nullable WinRT return: an empty pool is
                // reported as S_OK plus a null interface. windows-rs projects
                // that null interface as Error::empty() (HRESULT 0), so it is
                // an expected end-of-drain condition rather than a backend
                // failure. E_POINTER is retained for picker/capture API
                // variants that surface a null result that way.
                Err(error) if empty_frame_queue(&error) => break,
                Err(error) => return Err(windows_error("dequeue WGC frame", error)),
            }
        }
        let Some(frame) = latest else {
            return Ok(CaptureBackendPoll::Idle);
        };
        let result = self.read_frame(&frame);
        let close_result = frame
            .Close()
            .map_err(|error| windows_error("return WGC frame to pool", error));
        match (result, close_result) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(polled), Ok(())) => Ok(polled),
        }
    }

    fn reconfigure(&mut self, size: CaptureSize) -> Result<(), WindowsDisplayError> {
        if !self.started || self.closed {
            return Err(WindowsDisplayError::Backend(
                "WGC backend is not running".to_owned(),
            ));
        }
        size.validate()?;
        self.frame_pool
            .Recreate(
                &self.winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                FRAME_POOL_BUFFERS,
                size_int32(size)?,
            )
            .map_err(|error| windows_error("recreate WGC frame pool", error))?;
        self.selected_size = size;
        self.staging = None;
        self.frame_ready.store(false, Ordering::Release);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), WindowsDisplayError> {
        self.close_inner()
    }
}

impl Drop for WgcCaptureBackend {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}

fn validate_selected(selected: &DisplayStreamSpec) -> Result<(), WindowsDisplayError> {
    selected.validate()?;
    if selected.pixel_format != VideoPixelFormat::Bgra8
        || selected.colorimetry != VideoColorimetry::SrgbFull
    {
        return Err(WindowsDisplayError::InvalidStream(
            "WGC mirror v1 requires exact BGRA8/sRGB output".to_owned(),
        ));
    }
    Ok(())
}

fn capture_size(size: SizeInt32) -> Result<CaptureSize, WindowsDisplayError> {
    let width = u32::try_from(size.Width).map_err(|_| {
        WindowsDisplayError::InvalidFrame("capture width must be positive".to_owned())
    })?;
    let height = u32::try_from(size.Height).map_err(|_| {
        WindowsDisplayError::InvalidFrame("capture height must be positive".to_owned())
    })?;
    let size = CaptureSize { width, height };
    size.validate()?;
    Ok(size)
}

fn size_int32(size: CaptureSize) -> Result<SizeInt32, WindowsDisplayError> {
    size.validate()?;
    Ok(SizeInt32 {
        Width: i32::try_from(size.width).map_err(|_| {
            WindowsDisplayError::InvalidStream("capture width exceeds i32".to_owned())
        })?,
        Height: i32::try_from(size.height).map_err(|_| {
            WindowsDisplayError::InvalidStream("capture height exceeds i32".to_owned())
        })?,
    })
}

fn create_d3d_device()
-> Result<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice), WindowsDisplayError> {
    let mut device = None;
    let mut context = None;
    // SAFETY: all optional input pointers are null, the SDK version and flags
    // are fixed Windows constants, and both out parameters are valid Options.
    unsafe {
        D3D11CreateDevice(
            None::<&IDXGIAdapter>,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|error| windows_error("create D3D11 capture device", error))?;
    let device = device
        .ok_or_else(|| WindowsDisplayError::Backend("D3D11 returned a null device".to_owned()))?;
    let context = context.ok_or_else(|| {
        WindowsDisplayError::Backend("D3D11 returned a null immediate context".to_owned())
    })?;
    let dxgi_device: IDXGIDevice = device
        .cast()
        .map_err(|error| windows_error("query capture DXGI device", error))?;
    // SAFETY: `dxgi_device` is a live generated IDXGIDevice interface and the
    // API returns an owned WinRT inspectable on success.
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }
        .map_err(|error| windows_error("create WinRT D3D11 device", error))?;
    let winrt_device: IDirect3DDevice = inspectable
        .cast()
        .map_err(|error| windows_error("query WinRT D3D11 device", error))?;
    Ok((device, context, winrt_device))
}

fn copy_mapped_rows(
    mapped: D3D11_MAPPED_SUBRESOURCE,
    size: CaptureSize,
) -> Result<Vec<u8>, WindowsDisplayError> {
    if mapped.pData.is_null() {
        return Err(WindowsDisplayError::InvalidFrame(
            "mapped D3D11 texture returned a null pointer".to_owned(),
        ));
    }
    let width =
        usize::try_from(size.width).map_err(|_| WindowsDisplayError::ArithmeticExhausted)?;
    let height =
        usize::try_from(size.height).map_err(|_| WindowsDisplayError::ArithmeticExhausted)?;
    let row_bytes = width
        .checked_mul(4)
        .ok_or(WindowsDisplayError::ArithmeticExhausted)?;
    let row_pitch =
        usize::try_from(mapped.RowPitch).map_err(|_| WindowsDisplayError::ArithmeticExhausted)?;
    if row_pitch < row_bytes {
        return Err(WindowsDisplayError::InvalidFrame(
            "mapped D3D11 row pitch is shorter than packed BGRA".to_owned(),
        ));
    }
    let payload_bytes = row_bytes
        .checked_mul(height)
        .ok_or(WindowsDisplayError::ArithmeticExhausted)?;
    let mut payload = vec![0_u8; payload_bytes];
    for row in 0..height {
        let source_offset = row
            .checked_mul(row_pitch)
            .ok_or(WindowsDisplayError::ArithmeticExhausted)?;
        let destination_offset = row
            .checked_mul(row_bytes)
            .ok_or(WindowsDisplayError::ArithmeticExhausted)?;
        // SAFETY: Map guarantees at least RowPitch bytes for every texture row
        // until Unmap. The validated row width is no larger than RowPitch, and
        // both offsets are checked before forming the bounded slices.
        let source = unsafe {
            std::slice::from_raw_parts(mapped.pData.cast::<u8>().add(source_offset), row_bytes)
        };
        payload[destination_offset..destination_offset + row_bytes].copy_from_slice(source);
    }
    Ok(payload)
}

fn windows_error(context: &str, error: WindowsError) -> WindowsDisplayError {
    WindowsDisplayError::Backend(format!("{context}: {error}"))
}

fn empty_frame_queue(error: &WindowsError) -> bool {
    error.code().is_ok() || error.code() == E_POINTER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_rs_null_interface_is_an_empty_frame_queue() {
        assert!(empty_frame_queue(&WindowsError::empty()));
        assert!(!empty_frame_queue(&WindowsError::from_hresult(
            windows::core::HRESULT(0x8000_4005_u32 as i32)
        )));
    }

    #[test]
    fn mapped_rows_are_packed_without_d3d_padding() {
        let mut mapped_bytes: Vec<u8> = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, 9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97,
        ];
        let mapped = D3D11_MAPPED_SUBRESOURCE {
            pData: mapped_bytes.as_mut_ptr().cast(),
            RowPitch: 12,
            DepthPitch: 24,
        };
        assert_eq!(
            copy_mapped_rows(
                mapped,
                CaptureSize {
                    width: 2,
                    height: 2
                }
            )
            .unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn mapped_rows_reject_short_pitch_before_reading() {
        let mut mapped_bytes = [0_u8; 8];
        let mapped = D3D11_MAPPED_SUBRESOURCE {
            pData: mapped_bytes.as_mut_ptr().cast(),
            RowPitch: 4,
            DepthPitch: 8,
        };
        assert!(matches!(
            copy_mapped_rows(
                mapped,
                CaptureSize {
                    width: 2,
                    height: 1
                }
            ),
            Err(WindowsDisplayError::InvalidFrame(_))
        ));
    }
}
