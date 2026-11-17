//! Windows hardware video encoding through Media Foundation: the GPU's own encoder (NVIDIA NVENC,
//! Intel Quick Sync, AMD AMF) as an asynchronous MFT. Frames go in as NV12 from system memory;
//! output is Annex B with parameter sets on key frames. Any failure marks the codec broken for
//! this run, so the stream falls back to software H.264 instead of retrying every frame.
use std::{
    collections::VecDeque,
    mem::ManuallyDrop,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{HMODULE, LUID},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN},
            Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11Multithread},
            Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory4},
        },
        Media::MediaFoundation::*,
        System::{
            Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree},
            Variant::VARIANT,
        },
    },
    core::{GUID, Interface},
};

use super::{Codec, Encoded, Frame, VideoEncoder, avcc_to_annexb, nal_types, nv12, to_bgra};

static BROKEN_H264: AtomicBool = AtomicBool::new(false);
static BROKEN_HEVC: AtomicBool = AtomicBool::new(false);

fn broken(codec: Codec) -> &'static AtomicBool {
    match codec {
        Codec::Hevc => &BROKEN_HEVC,
        _ => &BROKEN_H264,
    }
}

/// This codec's hardware encoder failed earlier in this run.
pub fn is_broken(codec: Codec) -> bool {
    broken(codec).load(Ordering::Relaxed)
}

fn subtype(codec: Codec) -> Option<GUID> {
    match codec {
        Codec::H264 => Some(MFVideoFormat_H264),
        Codec::Hevc => Some(MFVideoFormat_HEVC),
        Codec::Av1 => None,
    }
}

fn e(what: &str) -> impl Fn(windows::core::Error) -> String + '_ {
    move |err| format!("{what}: {err}")
}

/// COM and Media Foundation for the calling thread (MF's own startup is reference counted).
fn startup() -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(e("MFStartup"))
    }
}

/// Hardware encoders for `codec`, best first.
unsafe fn find(codec: Codec) -> Result<Vec<IMFActivate>, String> {
    let sub = subtype(codec).ok_or("no Media Foundation codec")?;
    let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
    let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: sub };
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut n = 0u32;
    let flags = MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0);
    unsafe {
        MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, flags, Some(&input), Some(&output), &mut list, &mut n).map_err(e("MFTEnumEx"))?;
        if list.is_null() {
            return Ok(Vec::new());
        }
        let found = std::slice::from_raw_parts_mut(list, n as usize).iter_mut().filter_map(|a| a.take()).collect();
        CoTaskMemFree(Some(list as *const _));
        Ok(found)
    }
}

/// A D3D11 device on the encoder's own GPU (laptops have two), shared with the MFT. Optional:
/// most encoders also run without one.
unsafe fn device_manager(act: &IMFActivate) -> Option<IMFDXGIDeviceManager> {
    unsafe {
        let mut luid = [0u8; 8];
        let adapter: Option<IDXGIAdapter> = act.GetBlob(&MFT_ENUM_ADAPTER_LUID, &mut luid, None).ok().and_then(|_| {
            let id = LUID { LowPart: u32::from_le_bytes(luid[..4].try_into().ok()?), HighPart: i32::from_le_bytes(luid[4..].try_into().ok()?) };
            CreateDXGIFactory1::<IDXGIFactory4>().ok()?.EnumAdapterByLuid::<IDXGIAdapter>(id).ok()
        });
        let mut dev: Option<ID3D11Device> = None;
        let driver = if adapter.is_some() { D3D_DRIVER_TYPE_UNKNOWN } else { D3D_DRIVER_TYPE_HARDWARE };
        D3D11CreateDevice(
            adapter.as_ref(),
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut dev),
            None,
            None,
        )
        .ok()?;
        let dev = dev?;
        if let Ok(mt) = dev.cast::<ID3D11Multithread>() {
            let _ = mt.SetMultithreadProtected(true);
        }
        let mut token = 0u32;
        let mut mgr: Option<IMFDXGIDeviceManager> = None;
        MFCreateDXGIDeviceManager(&mut token, &mut mgr).ok()?;
        let mgr = mgr?;
        mgr.ResetDevice(&dev, token).ok()?;
        Some(mgr)
    }
}

pub struct MfEncoder {
    t: IMFTransform,
    events: IMFMediaEventGenerator,
    api: Option<ICodecAPI>,
    codec: Codec,
    width: u32,
    height: u32,
    kbps: u32,
    fps: u32,
    /// The encoder asked for this many more input frames.
    need: u32,
    out: VecDeque<Encoded>,
    /// Parameter sets from the output type, for key frames that come without them.
    header: Vec<u8>,
    started: Instant,
    /// A key frame was asked for but the frame was dropped: ask again with the next one.
    key_pending: bool,
    nv12: Vec<u8>,
    _mgr: Option<IMFDXGIDeviceManager>,
}

// The MFT lives in the multithreaded apartment and is only used by one thread at a time.
unsafe impl Send for MfEncoder {}

impl MfEncoder {
    pub fn new(codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self, String> {
        if is_broken(codec) {
            return Err(format!("hardware {} encoder failed earlier", codec.name()));
        }
        if !width.is_multiple_of(2) || !height.is_multiple_of(2) || width == 0 || height == 0 {
            return Err("frame size must be even".into());
        }
        startup()?;
        let list = unsafe { find(codec)? };
        if list.is_empty() {
            return Err(format!("no hardware {} encoder", codec.name()));
        }
        let mut last = String::new();
        for act in list {
            match unsafe { Self::from_activate(&act, codec, width, height, fps, kbps) } {
                Ok(enc) => {
                    let name = unsafe { friendly_name(&act) };
                    tracing::info!("stream: hardware {} encoder: {name}", codec.name());
                    return Ok(enc);
                }
                Err(err) => {
                    tracing::warn!("stream: hardware {} encoder did not start: {err}", codec.name());
                    unsafe {
                        let _ = act.ShutdownObject();
                    }
                    last = err;
                }
            }
        }
        Err(last)
    }

    unsafe fn from_activate(act: &IMFActivate, codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self, String> {
        unsafe {
            let t: IMFTransform = act.ActivateObject().map_err(e("activate"))?;
            let attrs = t.GetAttributes().map_err(e("attributes"))?;
            attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1).map_err(e("async unlock"))?;
            let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);
            let mgr = device_manager(act);
            if let Some(m) = &mgr
                && t.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, m.as_raw() as usize).is_err()
            {
                tracing::info!("stream: encoder runs without a shared D3D11 device");
            }
            let api = t.cast::<ICodecAPI>().ok();
            let fps = fps.max(1);
            if let Some(api) = &api {
                configure(api, fps, kbps);
            }

            let sub = subtype(codec).ok_or("no Media Foundation codec")?;
            let out = MFCreateMediaType().map_err(e("media type"))?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(e("out major"))?;
            out.SetGUID(&MF_MT_SUBTYPE, &sub).map_err(e("out subtype"))?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, kbps.saturating_mul(1000)).map_err(e("out bitrate"))?;
            video_format(&out, width, height, fps)?;
            let profile = match codec {
                Codec::Hevc => eAVEncH265VProfile_Main_420_8.0 as u32,
                _ => eAVEncH264VProfile_Main.0 as u32,
            };
            let _ = out.SetUINT32(&MF_MT_MPEG2_PROFILE, profile);
            t.SetOutputType(0, &out, 0).map_err(e("output type"))?;

            let inp = MFCreateMediaType().map_err(e("media type"))?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(e("in major"))?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(e("in subtype"))?;
            video_format(&inp, width, height, fps)?;
            t.SetInputType(0, &inp, 0).map_err(e("input type"))?;

            // Some encoders only take these once the types are set.
            if let Some(api) = &api {
                configure(api, fps, kbps);
            }
            let events: IMFMediaEventGenerator = t.cast().map_err(e("event generator"))?;
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0).map_err(e("begin streaming"))?;
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0).map_err(e("start of stream"))?;
            let mut enc = MfEncoder {
                t,
                events,
                api,
                codec,
                width,
                height,
                kbps,
                fps,
                need: 0,
                out: VecDeque::new(),
                header: Vec::new(),
                started: Instant::now(),
                key_pending: false,
                nv12: vec![0; nv12::nv12_len(width, height)],
                _mgr: mgr,
            };
            enc.load_header();
            Ok(enc)
        }
    }

    fn load_header(&mut self) {
        unsafe {
            let Ok(ty) = self.t.GetOutputCurrentType(0) else { return };
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut n = 0u32;
            if ty.GetAllocatedBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut p, &mut n).is_ok() && !p.is_null() {
                self.header = annexb(std::slice::from_raw_parts(p, n as usize));
                CoTaskMemFree(Some(p as *const _));
            }
        }
    }

    /// Handles queued encoder events until `want_input` is met (or output arrived when it's false),
    /// or `deadline` passes.
    fn pump(&mut self, want_input: bool, deadline: Instant) -> Result<(), String> {
        loop {
            match unsafe { self.events.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                Ok(ev) => {
                    let ty = unsafe { ev.GetType() }.map_err(e("event type"))?;
                    if ty == METransformNeedInput.0 as u32 {
                        self.need += 1;
                    } else if ty == METransformHaveOutput.0 as u32 {
                        self.take_output()?;
                    }
                }
                Err(err) if err.code() == MF_E_NO_EVENTS_AVAILABLE => {
                    let done = if want_input { self.need > 0 } else { !self.out.is_empty() };
                    if done || Instant::now() >= deadline {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_micros(500));
                }
                Err(err) => return Err(format!("encoder event: {err}")),
            }
        }
    }

    fn take_output(&mut self) -> Result<(), String> {
        unsafe {
            let info = self.t.GetOutputStreamInfo(0).map_err(e("output info"))?;
            let provides = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32) != 0;
            let mine = if provides {
                None
            } else {
                let s = MFCreateSample().map_err(e("sample"))?;
                let size = info.cbSize.max(self.width * self.height * 3 / 2);
                let b = MFCreateMemoryBuffer(size).map_err(e("buffer"))?;
                s.AddBuffer(&b).map_err(e("add buffer"))?;
                Some(s)
            };
            let mut bufs = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(mine), dwStatus: 0, pEvents: ManuallyDrop::new(None) }];
            let mut status = 0u32;
            let r = self.t.ProcessOutput(0, &mut bufs, &mut status);
            let sample = ManuallyDrop::take(&mut bufs[0].pSample);
            drop(ManuallyDrop::take(&mut bufs[0].pEvents));
            match r {
                Ok(()) => {}
                Err(err) if err.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    let ty = self.t.GetOutputAvailableType(0, 0).map_err(e("new output type"))?;
                    self.t.SetOutputType(0, &ty, 0).map_err(e("set new output type"))?;
                    self.load_header();
                    return Ok(());
                }
                Err(err) if err.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(err) => return Err(format!("encoder output: {err}")),
            }
            let Some(sample) = sample else { return Ok(()) };
            let key = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
            let buf = sample.ConvertToContiguousBuffer().map_err(e("output buffer"))?;
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut p, None, Some(&mut len)).map_err(e("lock output"))?;
            let raw = std::slice::from_raw_parts(p, len as usize).to_vec();
            let _ = buf.Unlock();
            let mut data = annexb(&raw);
            if key && !has_parameter_sets(self.codec, &data) && !self.header.is_empty() {
                let mut with = self.header.clone();
                with.extend_from_slice(&data);
                data = with;
            }
            if !data.is_empty() {
                self.out.push_back(Encoded { data, key });
            }
            Ok(())
        }
    }

    fn encode_inner(&mut self, f: &Frame, force_key: bool) -> Result<Option<Encoded>, String> {
        let (w, h, bgra) = to_bgra(f)?;
        if (w, h) != (self.width, self.height) {
            return Err(format!("frame is {w}×{h}, encoder is {}×{}", self.width, self.height));
        }
        let key = force_key || self.key_pending;
        // Earlier frames' output first, then room for this one.
        self.pump(true, Instant::now() + Duration::from_millis(40))?;
        if self.need == 0 {
            // The GPU is behind: skip this frame rather than queue latency.
            self.key_pending = key;
            return Ok(self.out.pop_front());
        }
        nv12::bgra_to_nv12(&bgra, w, h, &mut self.nv12);
        unsafe {
            let len = self.nv12.len() as u32;
            let buf = MFCreateMemoryBuffer(len).map_err(e("input buffer"))?;
            let mut p: *mut u8 = std::ptr::null_mut();
            buf.Lock(&mut p, None, None).map_err(e("lock input"))?;
            std::ptr::copy_nonoverlapping(self.nv12.as_ptr(), p, self.nv12.len());
            let _ = buf.Unlock();
            buf.SetCurrentLength(len).map_err(e("input length"))?;
            let sample = MFCreateSample().map_err(e("input sample"))?;
            sample.AddBuffer(&buf).map_err(e("add input"))?;
            let t100ns = (self.started.elapsed().as_nanos() / 100) as i64;
            sample.SetSampleTime(t100ns).map_err(e("sample time"))?;
            sample.SetSampleDuration(10_000_000 / self.fps as i64).map_err(e("sample duration"))?;
            if key && let Some(api) = &self.api {
                let _ = api.SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &VARIANT::from(1u32));
            }
            self.t.ProcessInput(0, &sample, 0).map_err(e("encoder input"))?;
        }
        self.key_pending = false;
        self.need -= 1;
        // Low-latency encoders hand this frame back within a few milliseconds.
        self.pump(false, Instant::now() + Duration::from_millis(25))?;
        Ok(self.out.pop_front())
    }
}

impl VideoEncoder for MfEncoder {
    fn encode(&mut self, f: &Frame, force_key: bool) -> Result<Option<Encoded>, String> {
        let r = self.encode_inner(f, force_key);
        if let Err(err) = &r {
            tracing::warn!("stream: hardware {} encoder failed, using software from now on: {err}", self.codec.name());
            broken(self.codec).store(true, Ordering::Relaxed);
        }
        r
    }

    fn set_bitrate(&mut self, kbps: u32) {
        if kbps == self.kbps {
            return;
        }
        self.kbps = kbps;
        if let Some(api) = &self.api {
            unsafe {
                let _ = api.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &VARIANT::from(kbps.saturating_mul(1000)));
            }
        }
    }

    fn codec(&self) -> Codec {
        self.codec
    }

    fn hardware(&self) -> bool {
        true
    }
}

impl Drop for MfEncoder {
    fn drop(&mut self) {
        unsafe {
            let _ = self.t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            if let Ok(s) = self.t.cast::<IMFShutdown>() {
                let _ = s.Shutdown();
            }
        }
    }
}

/// Constant bitrate, low latency, no B-frames, a key frame every 4 s.
unsafe fn configure(api: &ICodecAPI, fps: u32, kbps: u32) {
    unsafe {
        let _ = api.SetValue(&CODECAPI_AVEncCommonRateControlMode, &VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32));
        let _ = api.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &VARIANT::from(kbps.saturating_mul(1000)));
        let _ = api.SetValue(&CODECAPI_AVLowLatencyMode, &VARIANT::from(true));
        let _ = api.SetValue(&CODECAPI_AVEncMPVDefaultBPictureCount, &VARIANT::from(0u32));
        let _ = api.SetValue(&CODECAPI_AVEncMPVGOPSize, &VARIANT::from(fps * 4));
    }
}

/// Size, rate, square pixels, progressive, BT.709 limited range (what `nv12` produces).
unsafe fn video_format(t: &IMFMediaType, w: u32, h: u32, fps: u32) -> Result<(), String> {
    unsafe {
        t.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64).map_err(e("frame size"))?;
        t.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1).map_err(e("frame rate"))?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1).map_err(e("pixel aspect"))?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(e("interlace"))?;
        let _ = t.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32);
        let _ = t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32);
        let _ = t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32);
        let _ = t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32);
        Ok(())
    }
}

unsafe fn friendly_name(act: &IMFActivate) -> String {
    unsafe {
        let mut p = windows::core::PWSTR::null();
        let mut n = 0u32;
        if act.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut p, &mut n).is_ok() && !p.is_null() {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const _));
            return s;
        }
        "unknown".into()
    }
}

/// Encoders normally emit Annex B; a length-prefixed stream is converted.
fn annexb(raw: &[u8]) -> Vec<u8> {
    if raw.starts_with(&[0, 0, 0, 1]) || raw.starts_with(&[0, 0, 1]) {
        return raw.to_vec();
    }
    let mut out = Vec::with_capacity(raw.len() + 16);
    avcc_to_annexb(raw, 4, &mut out);
    out
}

fn has_parameter_sets(codec: Codec, annexb: &[u8]) -> bool {
    let types = nal_types(codec, annexb);
    match codec {
        Codec::Hevc => types.contains(&33), // SPS
        _ => types.contains(&7),
    }
}
