use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;

use blz_nx::*;

fn main() -> BlzResult<()> {
    let input_file = std::env::args().nth(1).unwrap();
    let output_file = std::env::args().nth(2).unwrap();

    let mut in_file = File::open(input_file).unwrap();
    let mut option = OpenOptions::new();
    let mut out_file = option
        .write(true)
        .create(true)
        .truncate(true)
        .open(output_file)
        .unwrap();

    let mut compression_buffer = Vec::new();
    in_file.read_to_end(&mut compression_buffer).unwrap();

    let decompression_buffer_size = get_decompression_buffer_size(&compression_buffer)?;

    let mut decompression_buffer = Vec::new();
    decompression_buffer.resize(decompression_buffer_size, 0u8);

    let decompressed_size = decompress_raw(&mut compression_buffer, &mut decompression_buffer)?;
    out_file
        .write(&decompression_buffer[0..decompressed_size])
        .unwrap();

    Ok(())
}
