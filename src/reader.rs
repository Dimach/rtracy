use crate::UtracyFormat;
use bincode::{de::read::Reader, error::DecodeError};
use std::{
    fs::File,
    io::{BufReader, Seek},
};
use zeekstd::{Decoder, Seekable};

pub enum ReadWrapper<'a> {
    Uncompressed(BufReader<File>),
    Compressed(Decoder<'a, File>),
}

impl<'a> ReadWrapper<'a> {
    pub fn open(file_name: impl AsRef<str>, format: UtracyFormat) -> Self {
        let file_name = file_name.as_ref();
        let file = File::open(file_name).expect("Error opening file");
        if format == UtracyFormat::Uncompressed {
            Self::Uncompressed(BufReader::new(file))
        } else if format == UtracyFormat::Compressed {
            Self::Compressed(Decoder::new(file).expect("zstd decoder setup failed"))
        } else if file_name.ends_with(".zst") {
            Self::Compressed(Decoder::new(file).expect("zstd decoder setup failed"))
        } else {
            Self::Uncompressed(BufReader::new(file))
        }
    }

    fn read_impl_compressed(
        compressed: &mut Decoder<'a, File>,
        bytes: &mut [u8],
    ) -> Result<(), DecodeError> {
        let required = bytes.len();
        let length = compressed
            .read(bytes)
            .map_err(|err| DecodeError::OtherString(err.to_string()))?;
        if required > length {
            Err(DecodeError::UnexpectedEnd {
                additional: required - length,
            })
        } else {
            Ok(())
        }
    }
}

impl<'a> Reader for ReadWrapper<'a> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        match self {
            Self::Uncompressed(reader) => Reader::read(reader, bytes),
            Self::Compressed(compressed) => Self::read_impl_compressed(compressed, bytes),
        }
    }

    fn peek_read(&mut self, v: usize) -> Option<&[u8]> {
        match self {
            Self::Uncompressed(reader) => reader.peek_read(v),
            _ => None,
        }
    }

    fn consume(&mut self, v: usize) {
        if let Self::Uncompressed(reader) = self {
            reader.consume(v);
        }
    }
}

impl<'a> Seek for ReadWrapper<'a> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::Uncompressed(reader) => reader.seek(pos),
            Self::Compressed(reader) => reader.seek(pos),
        }
    }

    fn rewind(&mut self) -> std::io::Result<()> {
        match self {
            Self::Uncompressed(reader) => reader.rewind(),
            Self::Compressed(reader) => reader.rewind(),
        }
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        match self {
            Self::Uncompressed(reader) => reader.stream_position(),
            Self::Compressed(reader) => reader.stream_position(),
        }
    }

    fn seek_relative(&mut self, offset: i64) -> std::io::Result<()> {
        match self {
            Self::Uncompressed(reader) => reader.seek_relative(offset),
            Self::Compressed(reader) => reader.seek_relative(offset),
        }
    }
}
