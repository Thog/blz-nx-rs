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

    let mut decompression_buffer = Vec::new();
    in_file.read_to_end(&mut decompression_buffer).unwrap();

    let compression_buffer_size = get_worst_compression_buffer_size(decompression_buffer.len());

    let mut compression_buffer = Vec::new();
    compression_buffer.resize(compression_buffer_size, 0u8);

    let compressed_size = compress_raw(&mut decompression_buffer, &mut compression_buffer)?;
    out_file
        .write(&compression_buffer[0..compressed_size])
        .unwrap();

    Ok(())
}
