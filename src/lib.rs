#![no_std]

use byteorder::ByteOrder;
use byteorder::LittleEndian;

#[derive(Debug)]
pub enum Error {
    Unknown,
    InvalidBlz,
    DecompressionBufferTooSmall,
    CompressionBufferTooSmall,
}

const BLZ_SHIFT: u8 = 1;
const BLZ_MASK: u8 = 0x80;
const BLZ_THRESHOLD: usize = 2;
const BLZ_MAX_OFFSET: usize = 0x1002;
const BLZ_MAX_CODED: usize = (1 << 4) + BLZ_THRESHOLD;

pub type BlzResult<T> = core::result::Result<T, Error>;

#[inline]
pub fn get_worst_compression_buffer_size(raw_len: usize) -> usize {
    raw_len + ((raw_len + 7) / 8) + 15
}

fn get_size_for_decompression(data: &[u8]) -> BlzResult<(u32, u32, u32)> {
    if data.len() < 4 {
        return Err(Error::InvalidBlz);
    }

    let inc_len = LittleEndian::read_u32(&data[data.len() - 4..]);
    if inc_len == 0 {
        let raw_len = data.len() as u32 - 4;
        Ok((raw_len, 0, raw_len))
    } else {
        if data.len() < 8 {
            return Err(Error::InvalidBlz);
        }

        let header_len = LittleEndian::read_u32(&data[data.len() - 8..]);
        if data.len() <= header_len as usize {
            return Err(Error::InvalidBlz);
        }

        let enc_len = LittleEndian::read_u32(&data[data.len() - 12..]);
        let dec_len = data.len() as u32 - enc_len;
        let pak_len = enc_len - header_len;
        let raw_len = dec_len + enc_len + inc_len;

        Ok((dec_len, pak_len, raw_len))
    }
}

pub fn get_decompression_buffer_size(data: &[u8]) -> BlzResult<usize> {
    Ok(get_size_for_decompression(data)?.2 as usize)
}

fn invert_slice(data: &mut [u8]) {
    let mut top_position = 0;
    let mut bottom_position = data.len() - 1;

    while top_position < bottom_position {
        let tmp = data[top_position];
        data[top_position] = data[bottom_position];
        data[bottom_position] = tmp;
        bottom_position -= 1;
        top_position += 1;
    }
}

fn compression_search(data: &[u8], current_position: usize) -> (usize, usize) {
    let mut len = BLZ_THRESHOLD as usize;
    let mut pos = 0;
    let max = if current_position >= BLZ_MAX_OFFSET {
        BLZ_MAX_OFFSET
    } else {
        current_position
    };

    for tmp_pos in 3..=max {
        let mut tmp_len = 0;
        while tmp_len < BLZ_MAX_CODED {
            if tmp_len == data[current_position..].len() || tmp_len >= tmp_pos {
                break;
            }

            if data[current_position + tmp_len] != data[current_position + tmp_len - tmp_pos] {
                break;
            }

            tmp_len += 1;
        }

        if tmp_len > len {
            pos = tmp_pos;
            len = tmp_len;
            if len == BLZ_MAX_CODED {
                break;
            }
        }
    }

    (len, pos)
}

pub fn compress_raw(
    decompressed_buffer: &mut [u8],
    compression_buffer: &mut [u8],
) -> BlzResult<usize> {
    if compression_buffer.len() < get_worst_compression_buffer_size(decompressed_buffer.len()) {
        return Err(Error::CompressionBufferTooSmall);
    }

    invert_slice(decompressed_buffer);

    let mut compressed_size_tmp = 0;
    let mut decompressed_size_tmp = decompressed_buffer.len();

    let mut mask = 0;
    let mut decompressed_pos = 0;
    let mut compressed_pos = 0;
    let mut flag_pos = 0;

    while decompressed_pos < decompressed_buffer.len() {
        mask >>= BLZ_SHIFT;
        if mask == 0 {
            flag_pos = compressed_pos;
            compression_buffer[flag_pos] = 0;
            compressed_pos += 1;
            mask = BLZ_MASK;
        }

        let (mut len_best, pos_best) = compression_search(&decompressed_buffer, decompressed_pos);

        if len_best > BLZ_THRESHOLD {
            if decompressed_pos + len_best < decompressed_buffer.len() {
                decompressed_pos += len_best;
                let (mut len_next, _) = compression_search(&decompressed_buffer, decompressed_pos);
                decompressed_pos -= len_best - 1;
                let (mut len_post, _) = compression_search(&decompressed_buffer, decompressed_pos);
                decompressed_pos -= 1;

                if len_next <= BLZ_THRESHOLD {
                    len_next = 1;
                }
                if len_post <= BLZ_THRESHOLD {
                    len_post = 1;
                }
                if len_best + len_next <= 1 + len_post {
                    len_best = 1;
                }
            }
        }

        compression_buffer[flag_pos] <<= 1;
        if len_best > BLZ_THRESHOLD {
            decompressed_pos += len_best;
            compression_buffer[flag_pos] |= 1;
            compression_buffer[compressed_pos] =
                (((len_best - (BLZ_THRESHOLD + 1)) << 4) | ((pos_best - 3) >> 8)) as u8;
            compression_buffer[compressed_pos + 1] = ((pos_best - 3) & 0xFF) as u8;
            compressed_pos += 2;
        } else {
            compression_buffer[compressed_pos] = decompressed_buffer[decompressed_pos];
            compressed_pos += 1;
            decompressed_pos += 1;
        }

        if compressed_pos + decompressed_buffer.len() - decompressed_pos
            < compressed_size_tmp + decompressed_size_tmp
        {
            compressed_size_tmp = compressed_pos;
            decompressed_size_tmp = decompressed_buffer.len() - decompressed_pos;
        }
    }

    while mask != 0 && mask != 1 {
        mask >>= BLZ_SHIFT;
        compression_buffer[flag_pos] <<= 1;
    }

    let compressed_size = compressed_pos;

    invert_slice(decompressed_buffer);
    invert_slice(&mut compression_buffer[0..compressed_size]);

    let result_size;

    // Is the compressed buffer actually bigger than the uncompressed one?
    if compressed_size_tmp == 0
        || (decompressed_buffer.len() + 4
            < ((compressed_size_tmp + decompressed_size_tmp + 3) & 0xFFFFFFFC) + 8)
    {
        // We just make it uncompressed.
        &(compression_buffer[0..decompressed_buffer.len()]).copy_from_slice(&decompressed_buffer);

        compressed_pos = decompressed_buffer.len();

        // Align the result
        while (compressed_pos & 3) != 0 {
            compression_buffer[compressed_pos] = 0;
            compressed_pos += 1;
        }

        // Write the partial header.
        LittleEndian::write_u32(&mut compression_buffer[compressed_pos..], 0);
        compressed_pos += 4;

        result_size = compressed_pos;
    } else {
        // First we copy the packed data to the right position to avoid possible corruption of the uncompressed data.
        let mut i = 0;

        while i < compressed_size_tmp {
            compression_buffer[decompressed_size_tmp + i] =
                compression_buffer[i + compressed_pos - compressed_size_tmp];
            i += 1;
        }

        // Then copy the decompressed data.
        (&mut compression_buffer[0..decompressed_size_tmp])
            .copy_from_slice(&decompressed_buffer[0..decompressed_size_tmp]);

        compressed_pos = decompressed_size_tmp + compressed_size_tmp;

        let compressed_len = compressed_size_tmp;
        let mut header_size = 12;
        let inc_len = decompressed_buffer.len() - compressed_len - decompressed_size_tmp;

        while (compressed_pos & 3) != 0 {
            compression_buffer[compressed_pos] = 0xFF;
            compressed_pos += 1;
            header_size += 1;
        }

        LittleEndian::write_u32(
            &mut compression_buffer[compressed_pos..],
            (compressed_len + header_size) as u32,
        );
        LittleEndian::write_u32(
            &mut compression_buffer[compressed_pos + 4..],
            header_size as u32,
        );
        LittleEndian::write_u32(
            &mut compression_buffer[compressed_pos + 8..],
            (inc_len - header_size) as u32,
        );
        compressed_pos += 12;

        result_size = compressed_pos;
    }

    Ok(result_size)
}

pub fn decompress_raw(
    compressed_data: &mut [u8],
    decompression_buffer: &mut [u8],
) -> BlzResult<usize> {
    let (dec_len, pak_len, raw_len) = get_size_for_decompression(compressed_data)?;

    if (decompression_buffer.len() as u32) < raw_len {
        return Err(Error::DecompressionBufferTooSmall);
    }

    let mut pak_buffer = &mut compressed_data[0..(dec_len + pak_len) as usize];
    let mut raw_buffer = &mut decompression_buffer[0..raw_len as usize];

    (&mut raw_buffer[0..dec_len as usize]).copy_from_slice(&pak_buffer[0..dec_len as usize]);

    pak_buffer = &mut pak_buffer[dec_len as usize..];
    raw_buffer = &mut raw_buffer[dec_len as usize..];

    // revert the data
    invert_slice(pak_buffer);

    let mut mask = 0;
    let mut decompression_buffer_position: usize = 0;
    let mut pak_position: usize = 0;
    let pak_position_end: usize = pak_len as usize;
    let mut flags = 0u8;

    while decompression_buffer_position < raw_buffer.len() {
        mask >>= BLZ_SHIFT;
        if mask == 0 {
            if pak_position == pak_position_end {
                break;
            }
            flags = pak_buffer[pak_position];
            pak_position += 1;
            mask = BLZ_MASK;
        }

        if (flags & mask) == 0 {
            if pak_position == pak_position_end {
                break;
            }
            raw_buffer[decompression_buffer_position] = pak_buffer[pak_position];
            decompression_buffer_position += 1;
            pak_position += 1;
        } else {
            if pak_position + 1 >= pak_position_end {
                break;
            }

            let mut pos: u32 = (u32::from(pak_buffer[pak_position]) << 8)
                | u32::from(pak_buffer[pak_position + 1]);
            pak_position += 2;
            let mut len: u32 = (pos >> 12) + BLZ_THRESHOLD as u32 + 1;

            // Invalid decompression length?!
            if decompression_buffer_position + len as usize > raw_buffer.len() {
                len = (raw_buffer.len() - decompression_buffer_position) as u32;
            }
            pos = (pos & 0xFFF) + 3;

            while len != 0 {
                raw_buffer[decompression_buffer_position] =
                    raw_buffer[decompression_buffer_position - pos as usize];
                decompression_buffer_position += 1;
                len -= 1;
            }
        }
    }

    // Invert data
    invert_slice(raw_buffer);

    debug_assert!(
        decompression_buffer_position == raw_buffer.len(),
        "Unexpected end of decompression"
    );

    Ok(decompression_buffer_position + dec_len as usize)
}
