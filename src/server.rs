use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, ErrorKind, Write};
use std::net::{Shutdown, TcpStream};
use std::thread::sleep;
use std::time::Duration;
use bincode::de::read::Reader;
use bincode::Encode;
use bincode::error::DecodeError;
use bincode::error::DecodeError::Io;
use crate::structs::{BINCODE_CONFIG, WriterBox, EventType, HandshakeStatus, SourceLocation, UTracyEvent, UTracyHeader, NetworkZoneBegin, NetworkZoneEnd, NetworkZoneColor, NetworkFrameMark, NetworkQuery, NetworkThreadContext, NetworkHeader, QueryResponseType, NetworkMessageSourceLocation, NetworkMessageString, U16SizeString, ServerQueryType, NetworkSourceCode};
use lz4::block::compress;

struct ServerContext<'l> {
    socket: &'l TcpStream,
    reader: BufReader<&'l TcpStream>,
    writer: BufWriter<&'l TcpStream>,
    encoder: WriterBox<'l, Vec<u8>>,
    last_thread_id: u32,
    timestamp: u64,
    locations: &'l Vec<SourceLocation>,
    strings: &'l HashMap<u64, String>,
    events_data: BufReader<File>,
    skip_frames: u64,
    limit_frames: u64,
}

impl ServerContext<'_> {
    fn process_client(&mut self) -> Result<(), String> {
        self.socket.set_nonblocking(true).map_err(|e| format!("{}", e))?;
        let mut read_event = 0;
        let mut frame = 0;
        loop {
            let e1: Result<UTracyEvent, DecodeError> = bincode::decode_from_reader(&mut self.events_data, BINCODE_CONFIG);
            if e1.is_err() {
                println!("Reached end of file");
                break;
            }
            let event = e1.unwrap();
            unsafe {
                match event.event_type {
                    EventType::Begin => {
                        if frame > self.skip_frames {
                            self.check_thread(event.event.begin.thread_id);
                            self.send_message(NetworkZoneBegin {
                                query_type: QueryResponseType::ZoneBegin,
                                timestamp: event.event.begin.timestamp - self.timestamp,
                                source_location: event.event.begin.source_location.into(),
                            })?;
                            self.timestamp = event.event.begin.timestamp;
                        }
                    }
                    EventType::End => {
                        if frame > self.skip_frames {
                            self.check_thread(event.event.begin.thread_id);
                            self.send_message(NetworkZoneEnd {
                                query_type: QueryResponseType::ZoneEnd,
                                timestamp: event.event.end.timestamp - self.timestamp,
                            })?;
                            self.timestamp = event.event.end.timestamp;
                        }
                    }
                    EventType::Color => {
                        if frame > self.skip_frames {
                            self.check_thread(event.event.begin.thread_id);
                            self.send_message(NetworkZoneColor {
                                query_type: QueryResponseType::ZoneColor,
                                color_r: event.event.color.color[0],
                                color_g: event.event.color.color[1],
                                color_b: event.event.color.color[2],
                            })?;
                        }
                    }
                    EventType::Mark => {
                        frame += 1;
                        if frame > self.skip_frames {
                            self.send_message(NetworkFrameMark {
                                query_type: QueryResponseType::FrameMarkMsg,
                                timestamp: event.event.mark.timestamp,
                                name: 0,
                            })?;
                        }
                        if frame > self.skip_frames + self.limit_frames {
                            break;
                        }
                    }
                }
            }
            read_event += 1;
            if read_event > 10000 {
                self.flush_buffer()?;
                self.process_query()?;
                read_event = 0;
            }
        }
        self.flush_buffer()?;
        println!("Sending done, wait 20 seconds to handle queries");
        for _i in 0..2  {
            if !self.process_query()? {
                return Ok(());
            }
            sleep(Duration::from_millis(10));
        }

        return Ok(());
    }

    fn process_query(&mut self) -> Result<bool, String> {
        loop {
            let mut buffer = [0u8; 13];
            let result = self.reader.read(&mut buffer);
            if let Err(Io { inner, additional }) = &result {
                if inner.kind() == ErrorKind::WouldBlock && *additional == 13 {
                    break;
                }
                if inner.kind() == ErrorKind::UnexpectedEof {
                    return Ok(false);
                }
            }
            result.map_err(|e| format!("{}", e))?;
            let request: NetworkQuery = bincode::decode_from_slice(&buffer, BINCODE_CONFIG).unwrap().0;
            match request.query_type {
                ServerQueryType::ServerQueryTerminate => {
                    return Ok(false);
                }
                ServerQueryType::ServerQueryString => {
                    let unkn: String = "Unkn".into();
                    let string = self.strings.get(&request.pointer).unwrap_or(&unkn);

                    self.send_message(NetworkMessageString {
                        query_type: QueryResponseType::StringData,
                        pointer: request.pointer,
                        string: U16SizeString(string),
                    })?;
                }
                ServerQueryType::ServerQueryThreadString => {
                    let main: String = "Main".into();
                    self.send_message(NetworkMessageString {
                        query_type: QueryResponseType::ThreadName,
                        pointer: request.pointer,
                        string: U16SizeString(&main),
                    })?;
                }
                ServerQueryType::ServerQuerySourceLocation => {
                    let source = self.locations.get(request.pointer as usize).unwrap();

                    self.send_message(NetworkMessageSourceLocation {
                        query_type: QueryResponseType::SourceLocation,
                        location: *source,
                    })?;
                }
                ServerQueryType::ServerQuerySymbolCode => {
                    self.send_message(QueryResponseType::AckSymbolCodeNotAvailable)?;
                }
                ServerQueryType::ServerQuerySourceCode => {
                    self.send_message(NetworkSourceCode {
                        query_type: QueryResponseType::AckSourceCodeNotAvailable,
                        id: request.pointer as u32,
                    })?;
                }
                ServerQueryType::ServerQueryDataTransfer | ServerQueryType::ServerQueryDataTransferPart => {
                    self.send_message(QueryResponseType::AckServerQueryNoop)?;
                }
                _ => { println!("Unknown request {:?}", request.query_type) }
            };
        }
        self.flush_buffer()?;
        return Ok(true);
    }

    fn send_message<W: Encode>(&mut self, message: W) -> Result<(), String> {
        if self.encoder.0.len() > 250 * 1024 {
            self.flush_buffer()?
        }
        bincode::encode_into_writer(message, &mut self.encoder, BINCODE_CONFIG).unwrap();
        return Ok(());
    }

    fn flush_buffer(&mut self) -> Result<(), String> {
        if self.encoder.0.is_empty() {
            return Ok(());
        }
        self.socket.set_nonblocking(false).map_err(|e| format!("{}", e))?;
        let result = compress(self.encoder.0.as_slice(), None, false).map_err(|e| format!("{}", e))?;
        self.writer.write(&u32::to_le_bytes(result.len() as u32)).map_err(|e| format!("{}", e))?;
        self.writer.write(result.as_slice()).map_err(|e| format!("{}", e))?;
        self.writer.flush().map_err(|e| format!("{}", e))?;
        self.encoder.0.clear();
        self.socket.set_nonblocking(true).map_err(|e| format!("{}", e))?;
        return Ok(());
    }

    fn check_thread(&mut self, thread_id: u32) {
        if self.last_thread_id != thread_id {
            self.last_thread_id = thread_id;
            self.timestamp = 0;
            bincode::encode_into_writer(NetworkThreadContext {
                query_type: QueryResponseType::ThreadContext,
                thread_id,
            }, &mut self.encoder, BINCODE_CONFIG).unwrap();
        }
    }
}

pub fn handle_client(stream: TcpStream, header: &UTracyHeader, locations: &Vec<SourceLocation>, strings: &HashMap<u64, String>, events_data: BufReader<File>, skip_frames: u32, limit_frames: u32) -> Result<(), String> {
    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    let mut client_name = [0u8; 8];
    reader.read(&mut client_name).map_err(|e| format!("{}", e))?;
    if std::str::from_utf8(&client_name).unwrap() != "TracyPrf" {
        return Err(format!("Invalid client, expected \"TracyPrf\", got {}", std::str::from_utf8(&client_name).unwrap()));
    }
    let version: u32 = bincode::decode_from_reader(&mut reader, BINCODE_CONFIG).map_err(|e| format!("{}", e))?;
    if version != 76 {
        writer.write(&[HandshakeStatus::HandshakeProtocolMismatch as u8]).map_err(|e| format!("{}", e))?;
        return Err(format!("Invalid client version, expected 76, got {}", version));
    }

    writer.write(&[HandshakeStatus::HandshakeWelcome as u8]).map_err(|e| format!("{}", e))?;
    writer.flush().map_err(|e| format!("{}", e))?;
    bincode::encode_into_writer(NetworkHeader {
        multiplier: header.multiplier,
        init_begin: header.init_begin,
        init_end: header.init_end,
        resolution: header.resolution,
        epoch: header.epoch,
        exec_time: header.exec_time,
        process_id: header.process_id,
        sampling_period: header.sampling_period,
        flags: header.flags,
        cpu_arch: header.cpu_arch,
        cpu_manufacturer: header.cpu_manufacturer,
        cpu_id: header.cpu_id,
        program_name: header.program_name,
        host_info: header.host_info,
    }, WriterBox(&mut writer), BINCODE_CONFIG).map_err(|e| format!("{}", e))?;
    writer.flush().map_err(|e| format!("{}", e))?;

    let mut buffer = Vec::new();
    let mut context = ServerContext {
        socket: &stream,
        reader,
        writer,
        encoder: WriterBox(&mut buffer),
        last_thread_id: 0,
        timestamp: 0,
        locations,
        strings,
        events_data,
        skip_frames: skip_frames.into(),
        limit_frames: limit_frames.into(),
    };
    context.process_client()?;
    stream.shutdown(Shutdown::Both).map_err(|e| format!("{}", e))?;

    return Ok(());
}
