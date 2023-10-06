use std::ffi::CString;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

use nom::IResult;

use crate::data::SequenceType;

// --- CStringReader -----------------------------------------------------------

#[derive(Debug)]
pub struct CStringReader<R: BufRead> {
    reader: R,
}

impl<R: BufRead> CStringReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub fn next(&mut self) -> Result<CString, std::io::Error> {
        let mut buffer = Vec::new();
        self.reader.read_until(0, &mut buffer)?;
        Ok(CString::from_vec_with_nul(buffer).expect("buffer should contain a single nul byte"))
    }
}

// --- LengthReader ------------------------------------------------------------

#[derive(Debug)]
pub struct LengthReader<R: BufRead> {
    reader: R,
}

impl<R: BufRead> LengthReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub fn next(&mut self) -> Result<u64, std::io::Error> {
        let mut n = 0u64;
        let mut x = u32::MAX;
        let mut buffer = [0u8; 4];

        while x == u32::MAX {
            self.reader.read_exact(&mut buffer[..])?;
            x = nom::number::complete::le_u32::<&[u8], nom::error::Error<&[u8]>>(&buffer[..])
                .unwrap()
                .1;
            n += x as u64;
        }

        Ok(n)
    }
}

// --- TextReader --------------------------------------------------------------

#[derive(Debug)]
pub struct TextReader<R: BufRead> {
    reader: R,
}

impl<R: BufRead> TextReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub fn next(&mut self, length: u64) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![0; length as usize];
        self.reader.read_exact(&mut buffer)?;
        Ok(buffer)
    }
}

// --- SequenceReader ----------------------------------------------------------

#[derive(Debug)]
pub struct SequenceReader<R: BufRead> {
    reader: R,
    ty: SequenceType,
    cache: Option<u8>,
}

impl<R: BufRead> SequenceReader<R> {
    pub fn new(reader: R, ty: SequenceType) -> Self {
        Self {
            reader,
            ty,
            cache: None,
        }
    }

    pub fn next(&mut self, length: u64) -> Result<String, std::io::Error> {
        let mut sequence = Vec::with_capacity(length as usize);
        if self.cache.is_some() && length > 0 {
            sequence.push(self.cache.take().unwrap());
        }

        while (sequence.len() as u64) < length {
            match self.ty {
                SequenceType::Dna => self.read_nucleotide::<b'T'>(length, &mut sequence),
                SequenceType::Rna => self.read_nucleotide::<b'U'>(length, &mut sequence),
                SequenceType::Protein | SequenceType::Text => self.read_text(length, &mut sequence),
            }?;
        }

        String::from_utf8(sequence)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    fn read_text(&mut self, length: u64, sequence: &mut Vec<u8>) -> Result<(), std::io::Error> {
        let buffer = self.reader.fill_buf()?;
        
        let n_to_copy = buffer.len().min(length as usize - sequence.len());
        sequence.extend_from_slice(&buffer[..n_to_copy]);
        self.reader.consume(n_to_copy);

        Ok(())
    }

    fn read_nucleotide<const T: u8>(&mut self, length: u64, sequence: &mut Vec<u8>) -> Result<(), std::io::Error> {
        let mut i = 0;
        let buffer = self.reader.fill_buf()?;

        while i < buffer.len() && (sequence.len() as u64) < length {
            let c1 = Self::decode::<T>(buffer[i] & 0x0F);
            sequence.push(c1);

            let c2 = Self::decode::<T>(buffer[i] >> 4);
            if sequence.len() as u64 == length {
                self.cache = Some(c2);
            } else {
                sequence.push(c2);
            }

            i += 1;
        }

        self.reader.consume(i);
        Ok(())
    }

    fn decode<const T: u8>(c: u8) -> u8 {
        match c {
            0x00 => b'-',
            0x01 => T,
            0x02 => b'G',
            0x03 => b'K',
            0x04 => b'C',
            0x05 => b'Y',
            0x06 => b'S',
            0x07 => b'B',
            0x08 => b'A',
            0x09 => b'W',
            0x0A => b'R',
            0x0B => b'D',
            0x0C => b'M',
            0x0D => b'H',
            0x0E => b'V',
            0x0F => b'N',
            _ => unreachable!(),
        }
    }
}
