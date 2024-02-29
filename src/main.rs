mod structs;
mod server;

use std::{str, thread};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::io::SeekFrom;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::env;
use crate::server::handle_client;
use crate::structs::{BINCODE_CONFIG, SourceLocation, UTracyHeader, UTracySourceLocation};


const FILE_SIGNATURE: u64 = 0x6D64796361727475;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("No input file supplied, exiting");
        println!("Use: <file> [-p port] [-s skip_frames] [-l limit_frames]");
        return;
    }

    let mut port = 8086;
    let mut skip_frames = 0u32;
    let mut limit_frames = u32::MAX;

    if args.len() > 2 {
        if args.len() == 3 {
            println!("Wrong option");
            println!("Available options: -p port, -s skip_frames, -l limit_frames");
        } else {
            for i in 0..(args.len() - 2) / 2 {
                match args[i * 2 + 2].as_str() {
                    "-p" => {
                        port = args[i * 2 + 3].parse().expect("Wrong input: -p");
                    }
                    "-s" => {
                        skip_frames = args[i * 2 + 3].parse().expect("Wrong input: -s");
                    }
                    "-l" => {
                        limit_frames = args[i * 2 + 3].parse().expect("Wrong input: -l");
                    }
                    _ => {
                        println!("Wrong option {}", args[i * 2 + 3].as_str());
                        println!("Available options: -p port, -s skip_frames, -l limit_frames");
                    }
                };
            }
        }
    }

    let mut file_reader = BufReader::new(File::open(&args[1]).expect("Error opening file"));

    let header: UTracyHeader = bincode::decode_from_reader(&mut file_reader, BINCODE_CONFIG).unwrap();

    if header.signature != FILE_SIGNATURE {
        println!("Wrong utracy file signature, expected \"{FILE_SIGNATURE}\" got \"{}\"", header.signature);
        return;
    }

    if header.version != 2 {
        println!("Wrong utracy file version, expected 2 got {}", header.version);
        return;
    }

    let location_count: u32 = bincode::decode_from_reader(&mut file_reader, BINCODE_CONFIG).unwrap();
    println!("Captured process: {}", str::from_utf8(&header.program_name).unwrap());

    println!("Found {} source locations", location_count);
    let mut locations = Vec::<SourceLocation>::with_capacity(location_count as usize);
    let mut strings = HashMap::<u64, String>::new();
    strings.insert(0, "".into());

    for i in 0..location_count {
        let location: UTracySourceLocation = bincode::decode_from_reader(&mut file_reader, BINCODE_CONFIG).unwrap();

        let mut name_string = location.name.get_hash();
        while strings.get(&name_string).is_some_and(|t| t != &location.name.0) {
            name_string += 1;
        }
        strings.insert(name_string, location.name.0);

        let mut function_string = location.function.get_hash();
        while strings.get(&function_string).is_some_and(|t| t != &location.function.0) {
            function_string += 1;
        }
        strings.insert(function_string, location.function.0);

        let mut file_string = location.file.get_hash();
        while strings.get(&file_string).is_some_and(|t| t != &location.file.0) {
            file_string += 1;
        }
        strings.insert(file_string, location.file.0);

        locations.insert(i as usize, SourceLocation {
            name: name_string,
            function: function_string,
            file: file_string,
            line: location.line,
            color_r: location.color[0],
            color_g: location.color[1],
            color_b: location.color[2],
        });
    }

    let events_position = file_reader.stream_position().unwrap();

    let header_ref = Box::leak(Box::new(header));
    let locations_ref = Box::leak(Box::new(locations));
    let strings_ref = Box::leak(Box::new(strings));
    let skip_frames_ref = Box::leak(Box::new(skip_frames));
    let limit_frames_ref = Box::leak(Box::new(limit_frames));

    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).unwrap();
    println!("Server listening on port {port}");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                let mut file_reader = BufReader::new(File::open(&args[1]).expect("Error opening file"));
                file_reader.seek(SeekFrom::Start(events_position)).unwrap();
                thread::spawn(|| {
                    if let Err(msg) = handle_client(stream, header_ref, locations_ref, strings_ref, file_reader, *skip_frames_ref, *limit_frames_ref) {
                        println!("Client disconnected with error: {}", msg)
                    }
                });
            }
            Err(e) => {
                println!("Network error: {}", e);
            }
        }
    }
    drop(listener);
}