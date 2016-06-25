use std::io::{self, Read, Seek, SeekFrom};
use super::byteorder::{BigEndian, ReadBytesExt};
use super::{ImageBrush, OpenError, BrushError};
use super::util;

pub struct Decoder<R> {
    rdr: R,
    version: u16,
    count: u16,
    next_brush_pos: u64,
}

pub fn open<R: Read + Seek>(mut rdr: R,
                            version: u16,
                            count: u16)
                            -> Result<Decoder<R>, OpenError> {
    let cur_pos = try!(util::tell(&mut rdr));
    Ok(Decoder {
        rdr: rdr,
        version: version,
        count: count,
        next_brush_pos: cur_pos,
    })
}

pub fn next_brush<R: Read + Seek>(dec: &mut Decoder<R>)
                                  -> Option<Result<ImageBrush, BrushError>> {
    if dec.count == 0 {
        return None;
    }

    dec.count -= 1;

    Some(match do_brush_head(dec) {
        Ok(res) => {
            dec.next_brush_pos = res.next_brush_pos;
            do_brush_body(dec)
        }
        Err(e) => {
            // We didn't get the next brush's position, so we can't resume on
            // the next brush. Flag the iteration as over before we error out.
            dec.count = 0;
            Err(e.into())
        }
    })
}

struct BrushHeadResult {
    next_brush_pos: u64,
}

/// Moves `dec` into position to read out the next brush with `do_brush_body`.
/// Returns where the brush after this one is located.
fn do_brush_head<R: Read + Seek>(dec: &mut Decoder<R>)
                                 -> Result<BrushHeadResult, io::Error> {
    let brush_pos = dec.next_brush_pos;
    try!(dec.rdr.seek(SeekFrom::Start(brush_pos)));

    let len = try!(dec.rdr.read_u16::<BigEndian>()) as u64;
    // We are now at brush_pos + 2.
    let next_brush_pos = (brush_pos + 2) + len;

    Ok(BrushHeadResult { next_brush_pos: next_brush_pos })
}

/// With `dec` positioned by `do_brush_head`, reads out a brush.
fn do_brush_body<R: Read + Seek>(dec: &mut Decoder<R>) -> Result<ImageBrush, BrushError> {
    let ty = try!(dec.rdr.read_u16::<BigEndian>());
    if ty != 2 {
        return Err(BrushError::UnsupportedBrushType { ty: ty });
    }

    let _misc = try!(dec.rdr.read_u32::<BigEndian>());
    let _spacing = try!(dec.rdr.read_u16::<BigEndian>());

    if dec.version == 2 {
        // Skip over a length-prefixed UCS2 String
        let len = try!(dec.rdr.read_u32::<BigEndian>()) as i64;
        let len_in_bytes = 2 * len;
        try!(dec.rdr.seek(SeekFrom::Current(len_in_bytes)));
    }

    let _antialiasing = try!(dec.rdr.read_u8());

    let top = try!(dec.rdr.read_u16::<BigEndian>());
    let left = try!(dec.rdr.read_u16::<BigEndian>());
    let bottom = try!(dec.rdr.read_u16::<BigEndian>());
    let right = try!(dec.rdr.read_u16::<BigEndian>());

    let _topl = try!(dec.rdr.read_u32::<BigEndian>());
    let _leftl = try!(dec.rdr.read_u32::<BigEndian>());
    let _bottoml = try!(dec.rdr.read_u32::<BigEndian>());
    let _rightl = try!(dec.rdr.read_u32::<BigEndian>());

    let depth = try!(dec.rdr.read_u16::<BigEndian>());
    if depth != 8 {
        return Err(BrushError::UnsupportedBitDepth { depth: depth });
    }

    let compressed = try!(dec.rdr.read_u8()) != 0;

    let width = (right - left) as u32;
    let height = (bottom - top) as u32;
    let size = (width as usize) * (height as usize) * (depth as usize >> 3);

    let data = if compressed {
        try!(util::read_rle_data(&mut dec.rdr, height, size))
    } else {
        let mut v = vec![0; size];
        try!(dec.rdr.read_exact(&mut v));
        v
    };

    Ok(ImageBrush {
        width: width,
        height: height,
        depth: depth,
        data: data,
    })
}
