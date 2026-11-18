//! Packed streams (the display cache, the style library): blocks, each the size of its content
//! (u32), the size of its compressed bytes (u32) and those bytes, a zlib stream. A block holds
//! at most 64 KiB of content. A block of two zeros closes the stream; what follows it (four
//! bytes `ED`) is filler.

use crate::Error;
use std::io::Read;

/// More content than any stream of a real file inflates to: a bound against streams that are
/// built to exhaust memory.
pub const MAX_CONTENT: usize = 1 << 30;

/// The content of a packed stream.
pub fn inflate(stream: &[u8]) -> Result<Vec<u8>, Error> {
    let word = |at: usize| stream.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let (mut at, mut out) = (0, Vec::new());
    loop {
        let (Some(size), Some(packed)) = (word(at), word(at + 4)) else { return Err(Error::Compression("no closing block")) };
        at += 8;
        if packed == 0 {
            return if size == 0 { Ok(out) } else { Err(Error::Compression("block without bytes")) };
        }
        let bytes = at.checked_add(packed).and_then(|end| stream.get(at..end)).ok_or(Error::Compression("block cut short"))?;
        if out.len().saturating_add(size) > MAX_CONTENT {
            return Err(Error::Compression("content too large"));
        }
        let before = out.len();
        // one byte more than promised shows a block that lies about its size
        flate2::read::ZlibDecoder::new(bytes).take(size as u64 + 1).read_to_end(&mut out).map_err(|_| Error::Compression("not a zlib stream"))?;
        if out.len() - before != size {
            return Err(Error::Compression("block of another size than it says"));
        }
        at += packed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::pack;

    #[test]
    fn blocks_inflate_to_the_content() {
        let content: Vec<u8> = (0..10_000u32).flat_map(|k| (k * k).to_le_bytes()).collect();
        for block in [100_000, 4096, 1000] {
            assert_eq!(inflate(&pack(&content, block)).unwrap(), content, "blocks of {block}");
        }
    }

    #[test]
    fn an_empty_stream_is_empty() {
        assert_eq!(inflate(&[0; 8]).unwrap(), Vec::<u8>::new());
        assert_eq!(inflate(&pack(&[], 4096)).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn damage_is_an_error() {
        let good = pack(&[42u8; 5000], 4096);
        assert_eq!(inflate(&good[..good.len() - 14]), Err(Error::Compression("block cut short")));
        assert_eq!(inflate(&good[..good.len() - 12]), Err(Error::Compression("no closing block")));
        assert_eq!(inflate(&[]), Err(Error::Compression("no closing block")));
        // a block that says it is smaller than it is
        let mut lying = good.clone();
        lying[..4].copy_from_slice(&100u32.to_le_bytes());
        assert_eq!(inflate(&lying), Err(Error::Compression("block of another size than it says")));
        let mut broken = good;
        broken[8..24].fill(0xAA);
        assert!(inflate(&broken).is_err());
    }
}
