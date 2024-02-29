use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use bincode::enc::Encoder;
use bincode::enc::write::Writer;
use bincode::{Decode, Encode};
use bincode::config::{Configuration, Fixint, LittleEndian};
use bincode::de::Decoder;
use bincode::de::read::Reader;
use bincode::error::{DecodeError, EncodeError};
use bincode::error::AllowedEnumVariants::{Allowed, Range};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};

pub const BINCODE_CONFIG: Configuration<LittleEndian, Fixint> = bincode::config::standard().with_little_endian().with_fixed_int_encoding();

#[derive(Debug)]
pub struct U16SizeString<'l>(pub &'l String);

impl Encode for U16SizeString<'_> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        (self.0.len() as u16).encode(encoder)?;
        encoder.writer().write(self.0.as_bytes())?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct U32SizeString(pub String);

impl U32SizeString {
    pub fn get_hash(&self) -> u64 {
        if self.0.len() == 0 {
            return 0;
        }
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        return hasher.finish();
    }
}

impl Encode for U32SizeString {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        (self.0.len() as u32).encode(encoder)?;
        encoder.writer().write(self.0.as_bytes())?;
        Ok(())
    }
}

bincode::impl_borrow_decode!(U32SizeString);

impl Decode for U32SizeString {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        let len = u32::decode(decoder)?;
        decoder.claim_container_read::<u8>(len as usize)?;
        let mut vec = Vec::new();
        vec.resize(len as usize, 0u8);
        decoder.reader().read(&mut vec)?;
        return match String::from_utf8(vec) {
            Ok(result) => Ok(U32SizeString(result)),
            Err(e) => Err(DecodeError::Utf8 {
                inner: e.utf8_error(),
            })
        };
    }
}

#[derive(Decode, Debug)]
pub struct UTracyHeader {
    pub signature: u64,
    pub version: u32,
    _padding0: u32,
    pub multiplier: f64,
    pub init_begin: u64,
    pub init_end: u64,
    pub delay: u64,
    pub resolution: u64,
    pub epoch: u64,
    pub exec_time: u64,
    pub process_id: u64,
    pub sampling_period: u64,
    pub flags: u8,
    pub cpu_arch: u8,
    pub cpu_manufacturer: [u8; 12],
    _padding1: [u8; 2],
    pub cpu_id: u32,
    pub program_name: [u8; 64],
    pub host_info: [u8; 1024],
    _padding2: [u8; 4],
}

#[derive(Decode, Debug)]
pub struct UTracySourceLocation {
    pub name: U32SizeString,
    pub function: U32SizeString,
    pub file: U32SizeString,
    pub line: u32,
    pub color: [u8; 4],
}

#[derive(Encode, Copy, Clone, Debug)]
pub struct SourceLocation {
    pub name: u64,
    pub function: u64,
    pub file: u64,
    pub line: u32,
    pub color_r: u8,
    pub color_g: u8,
    pub color_b: u8,
}

#[derive(FromPrimitive, ToPrimitive, Copy, Clone, Debug)]
pub enum EventType {
    Begin = 15,
    End = 17,
    Color = 62,
    Mark = 64,
}

bincode::impl_borrow_decode!(EventType);

impl Decode for EventType {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        let value = u8::decode(decoder)?;
        return match EventType::from_u8(value) {
            None => Err(DecodeError::UnexpectedVariant {
                type_name: "event_type",
                allowed: &Allowed(&[15, 17, 62, 64]),
                found: value.into(),
            }),
            Some(v) => Ok(v)
        };
    }
}

#[derive(Decode, Copy, Clone, Debug)]
pub struct EventZoneBegin {
    pub thread_id: u32,
    pub source_location: u32,
    pub timestamp: u64,
}

#[derive(Decode, Copy, Clone, Debug)]
pub struct EventZoneEnd {
    pub thread_id: u32,
    _padding: u32,
    pub timestamp: u64,
}

#[derive(Decode, Copy, Clone, Debug)]
pub struct EventZoneColor {
    pub thread_id: u32,
    pub color: [u8; 4],
    pub padding: u64,
}

#[derive(Decode, Copy, Clone, Debug)]
pub struct EventFrameMark {
    pub name: u32,
    _padding: u32,
    pub timestamp: u64,
}

pub union Event {
    pub begin: EventZoneBegin,
    pub end: EventZoneEnd,
    pub color: EventZoneColor,
    pub mark: EventFrameMark,
}

bincode::impl_borrow_decode!(Event);

impl Decode for Event {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        EventZoneBegin::decode(decoder).map(|t| Event { begin: t })
    }
}

#[derive(Decode)]
pub struct UTracyEvent {
    pub event_type: EventType,
    _padding: [u8; 7],
    pub event: Event,
}

#[derive(FromPrimitive, ToPrimitive, Copy, Clone, Debug)]
#[repr(u8)]
pub enum HandshakeStatus {
    HandshakePending,
    HandshakeWelcome,
    HandshakeProtocolMismatch,
    HandshakeNotAvailable,
    HandshakeDropped,
}

impl Encode for HandshakeStatus {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        return self.to_u8().unwrap().encode(encoder);
    }
}

pub struct WriterBox<'l, W: Write>(pub &'l mut W);

impl<W: Write> Writer for WriterBox<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.0.write(bytes).map_err(|e| EncodeError::Io { inner: e, index: 0 }).map(|_| ())
    }
}

#[derive(Encode, Decode, Debug)]
pub struct NetworkHeader {
    pub multiplier: f64,
    pub init_begin: u64,
    pub init_end: u64,
    pub delay: u64,
    pub resolution: u64,
    pub epoch: u64,
    pub exec_time: u64,
    pub process_id: u64,
    pub sampling_period: u64,
    pub flags: u8,
    pub cpu_arch: u8,
    pub cpu_manufacturer: [u8; 12],
    pub cpu_id: u32,
    pub program_name: [u8; 64],
    pub host_info: [u8; 1024],
}

#[derive(Decode, Debug)]
pub struct NetworkQuery {
    pub query_type: ServerQueryType,
    pub pointer: u64,
    pub extra: u32,
}

#[derive(Encode, Debug)]
pub struct NetworkZoneBegin {
    pub query_type: QueryResponseType,
    pub timestamp: u64,
    pub source_location: u64,
}

#[derive(Encode, Debug)]
pub struct NetworkZoneEnd {
    pub query_type: QueryResponseType,
    pub timestamp: u64,
}

#[derive(Encode, Debug)]
pub struct NetworkZoneColor {
    pub query_type: QueryResponseType,
    pub color_r: u8,
    pub color_g: u8,
    pub color_b: u8,
}

#[derive(Encode, Debug)]
pub struct NetworkFrameMark {
    pub query_type: QueryResponseType,
    pub timestamp: u64,
    pub name: u64,
}

#[derive(Encode, Debug)]
pub struct NetworkThreadContext {
    pub query_type: QueryResponseType,
    pub thread_id: u32,
}

#[derive(Encode, Debug)]
pub struct NetworkSourceCode {
    pub query_type: QueryResponseType,
    pub id: u32,
}

#[derive(Encode, Debug)]
pub struct NetworkMessageSourceLocation {
    pub query_type: QueryResponseType,
    pub location: SourceLocation,
}

#[derive(Encode, Debug)]
pub struct NetworkMessageString<'l> {
    pub query_type: QueryResponseType,
    pub pointer: u64,
    pub string: U16SizeString<'l>,
}

#[derive(FromPrimitive, ToPrimitive, Debug)]
pub enum ServerQueryType {
    ServerQueryTerminate = 0,
    ServerQueryString,
    ServerQueryThreadString,
    ServerQuerySourceLocation,
    ServerQueryPlotName,
    ServerQueryFrameName,
    ServerQueryParameter,
    ServerQueryFiberName,
    // Items above are high priority. Split order must be preserved. See IsQueryPrio().
    ServerQueryDisconnect,
    ServerQueryCallstackFrame,
    ServerQueryExternalName,
    ServerQuerySymbol,
    ServerQuerySymbolCode,
    ServerQuerySourceCode,
    ServerQueryDataTransfer,
    ServerQueryDataTransferPart,
}

bincode::impl_borrow_decode!(ServerQueryType);
impl Decode for ServerQueryType {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        let byte = u8::decode(decoder)?;
        match ServerQueryType::from_u8(byte) {
            None => {
                const MAX_VALUE: u32 = ServerQueryType::ServerQueryDataTransferPart as u32;
                Err(DecodeError::UnexpectedVariant {
                    type_name: "ServerQueryType",
                    allowed: &Range { min: 0, max: MAX_VALUE },
                    found: byte.into(),
                })
            }
            Some(val) => {
                Ok(val)
            }
        }
    }
}

#[derive(FromPrimitive, ToPrimitive, Debug)]
pub enum QueryResponseType {
    ZoneText = 0,
    ZoneName,
    Message,
    MessageColor,
    MessageCallstack,
    MessageColorCallstack,
    MessageAppInfo,
    ZoneBeginAllocSrcLoc,
    ZoneBeginAllocSrcLocCallstack,
    CallstackSerial,
    Callstack,
    CallstackAlloc,
    CallstackSample,
    CallstackSampleContextSwitch,
    FrameImage,
    ZoneBegin,
    ZoneBeginCallstack,
    ZoneEnd,
    LockWait,
    LockObtain,
    LockRelease,
    LockSharedWait,
    LockSharedObtain,
    LockSharedRelease,
    LockName,
    MemAlloc,
    MemAllocNamed,
    MemFree,
    MemFreeNamed,
    MemAllocCallstack,
    MemAllocCallstackNamed,
    MemFreeCallstack,
    MemFreeCallstackNamed,
    GpuZoneBegin,
    GpuZoneBeginCallstack,
    GpuZoneBeginAllocSrcLoc,
    GpuZoneBeginAllocSrcLocCallstack,
    GpuZoneEnd,
    GpuZoneBeginSerial,
    GpuZoneBeginCallstackSerial,
    GpuZoneBeginAllocSrcLocSerial,
    GpuZoneBeginAllocSrcLocCallstackSerial,
    GpuZoneEndSerial,
    PlotDataInt,
    PlotDataFloat,
    PlotDataDouble,
    ContextSwitch,
    ThreadWakeup,
    GpuTime,
    GpuContextName,
    CallstackFrameSize,
    SymbolInformation,
    ExternalNameMetadata,
    SymbolCodeMetadata,
    SourceCodeMetadata,
    FiberEnter,
    FiberLeave,
    Terminate,
    KeepAlive,
    ThreadContext,
    GpuCalibration,
    Crash,
    CrashReport,
    ZoneValidation,
    ZoneColor,
    ZoneValue,
    FrameMarkMsg,
    FrameMarkMsgStart,
    FrameMarkMsgEnd,
    FrameVsync,
    SourceLocation,
    LockAnnounce,
    LockTerminate,
    LockMark,
    MessageLiteral,
    MessageLiteralColor,
    MessageLiteralCallstack,
    MessageLiteralColorCallstack,
    GpuNewContext,
    CallstackFrame,
    SysTimeReport,
    SysPowerReport,
    TidToPid,
    HwSampleCpuCycle,
    HwSampleInstructionRetired,
    HwSampleCacheReference,
    HwSampleCacheMiss,
    HwSampleBranchRetired,
    HwSampleBranchMiss,
    PlotConfig,
    ParamSetup,
    AckServerQueryNoop,
    AckSourceCodeNotAvailable,
    AckSymbolCodeNotAvailable,
    CpuTopology,
    SingleStringData,
    SecondStringData,
    MemNamePayload,
    StringData,
    ThreadName,
    PlotName,
    SourceLocationPayload,
    CallstackPayload,
    CallstackAllocPayload,
    FrameName,
    FrameImageData,
    ExternalName,
    ExternalThreadName,
    SymbolCode,
    SourceCode,
    FiberName,
    NumTypes,
}

impl Encode for QueryResponseType {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        return self.to_u8().unwrap().encode(encoder);
    }
}