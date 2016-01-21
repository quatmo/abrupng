use std::io::{Read, Seek, SeekFrom};
use abr::byteorder::{self, BigEndian, ReadBytesExt};
use abr::{SampleBrush, OpenError, BrushError};
use abr::helper;

pub struct Abr6Decoder<R> {
    rdr: R,
    subversion: u16,
    sample_section_end: u64,
    next_brush_pos: u64,
}

pub fn open<R: Read + Seek>(mut rdr: R, subversion: u16) -> Result<Abr6Decoder<R>, OpenError> {
    // Find the sample section
    loop {
        let mut buf = [0; 4];
        try!(rdr.read_exact(&mut buf));
        if buf == ['8' as u8, 'b' as u8, 'i' as u8, 'm' as u8] {
            return Err(OpenError::Found8bim);
        }
        try!(rdr.read_exact(&mut buf));
        if buf == ['s' as u8, 'a' as u8, 'm' as u8, 'p' as u8] {
            break;
        }
        let len = try!(rdr.read_u32::<BigEndian>());
        try!(rdr.seek(SeekFrom::Current(len as i64)));
    }
    let len = try!(rdr.read_u32::<BigEndian>()) as u64;
    let cur = try!(helper::tell(&mut rdr));
    Ok(Abr6Decoder {
        rdr: rdr,
        subversion: subversion,
        sample_section_end: cur + len,
        next_brush_pos: cur,
    })
}


pub fn next_brush<R: Read + Seek>(dec: &mut Abr6Decoder<R>)
                                  -> Option<Result<SampleBrush, BrushError>> {
    // Is iteration over?
    if dec.next_brush_pos >= dec.sample_section_end {
        return None;
    }

    // Process the length; if we can't get it, we can't resume on the next brush, so
    // flag the iteration as over by setting next_brush_pos to the end of the
    // section.
    match process_brush_length(dec) {
        Ok(next_brush_pos) => {
            dec.next_brush_pos = next_brush_pos;
        }
        Err(e) => {
            dec.next_brush_pos = dec.sample_section_end;
            return Some(Err(BrushError::IoError(e)));
        }
    }

    Some(process_brush_body(dec))
}

/// Get the reader prepped to begin reading out a brush. Returns the position where
/// the next brush starts.
fn process_brush_length<R: Read + Seek>(dec: &mut Abr6Decoder<R>) -> Result<u64, byteorder::Error> {
    let brush_pos = dec.next_brush_pos;
    try!(dec.rdr.seek(SeekFrom::Start(brush_pos)));

    let len = try!(dec.rdr.read_u32::<BigEndian>()) as u64;
    // We are now at brush_pos + 4.
    let end_pos = (brush_pos + 4) + len;
    // Brushes are aligned to 4-byte boundaries; round up to get to one.
    let next_brush_pos = (end_pos + 3) & !3;

    Ok(next_brush_pos)
}

fn process_brush_body<R: Read + Seek>(dec: &mut Abr6Decoder<R>) -> Result<SampleBrush, BrushError> {
    // Skip over... something.
    try!(dec.rdr.seek(SeekFrom::Current(if dec.subversion == 1 {
        47
    } else {
        301
    })));

    let top = try!(dec.rdr.read_u32::<BigEndian>());
    let left = try!(dec.rdr.read_u32::<BigEndian>());
    let bottom = try!(dec.rdr.read_u32::<BigEndian>());
    let right = try!(dec.rdr.read_u32::<BigEndian>());

    let depth = try!(dec.rdr.read_u16::<BigEndian>());
    if depth != 8 {
        return Err(BrushError::UnsupportedBitDepth { depth: depth });
    }

    let compressed = try!(dec.rdr.read_u8()) != 0;

    let width = right - left;
    let height = bottom - top;
    let size = (width as usize) * (height as usize) * (depth as usize >> 3);

    let data = if compressed {
        try!(helper::read_rle_data(&mut dec.rdr, height, size))
    } else {
        let mut v = vec![0; size];
        try!(dec.rdr.read_exact(&mut v));
        v
    };

    Ok(SampleBrush {
        width: width,
        height: height,
        depth: depth,
        data: data,
    })
}