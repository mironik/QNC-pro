//! Two checksums, fed in pieces of any size. SHA-256 names a document for ever (the identity
//! of a camera document in a record); xxHash64 (seed 0) proves that a copy is the source,
//! as the old QNC proved its copies (`xxh64:<16 hex>`).

pub const XXH64: &str = "xxh64";

const P1: u64 = 0x9E37_79B1_85EB_CA87;
const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const P3: u64 = 0x1656_67B1_9E37_79F9;
const P4: u64 = 0x85EB_CA77_C2B2_AE63;
const P5: u64 = 0x27D4_EB2F_1656_67C5;

#[derive(Debug, Clone)]
pub struct XxHash64 {
    lanes: [u64; 4],
    buffer: [u8; 32],
    buffered: usize,
    total: u64,
}

impl Default for XxHash64 {
    fn default() -> Self {
        Self::new()
    }
}

fn round(acc: u64, input: u64) -> u64 {
    acc.wrapping_add(input.wrapping_mul(P2)).rotate_left(31).wrapping_mul(P1)
}

fn merge(acc: u64, lane: u64) -> u64 {
    (acc ^ round(0, lane)).wrapping_mul(P1).wrapping_add(P4)
}

fn word(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().expect("eight bytes"))
}

impl XxHash64 {
    pub fn new() -> Self {
        Self { lanes: [P1.wrapping_add(P2), P2, 0, 0u64.wrapping_sub(P1)], buffer: [0; 32], buffered: 0, total: 0 }
    }

    fn stripe(&mut self, stripe: &[u8]) {
        for (at, lane) in self.lanes.iter_mut().enumerate() {
            *lane = round(*lane, word(&stripe[at * 8..]));
        }
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        self.total += bytes.len() as u64;
        if self.buffered > 0 {
            let take = (32 - self.buffered).min(bytes.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&bytes[..take]);
            self.buffered += take;
            bytes = &bytes[take..];
            if self.buffered < 32 {
                return;
            }
            let stripe = self.buffer;
            self.stripe(&stripe);
            self.buffered = 0;
        }
        while bytes.len() >= 32 {
            self.stripe(&bytes[..32]);
            bytes = &bytes[32..];
        }
        self.buffer[..bytes.len()].copy_from_slice(bytes);
        self.buffered = bytes.len();
    }

    pub fn digest(&self) -> u64 {
        let [v1, v2, v3, v4] = self.lanes;
        let mut hash = if self.total >= 32 {
            let hash = v1.rotate_left(1).wrapping_add(v2.rotate_left(7)).wrapping_add(v3.rotate_left(12)).wrapping_add(v4.rotate_left(18));
            [v1, v2, v3, v4].into_iter().fold(hash, merge)
        } else {
            P5
        };
        hash = hash.wrapping_add(self.total);
        let mut rest = &self.buffer[..self.buffered];
        while rest.len() >= 8 {
            hash = (hash ^ round(0, word(rest))).rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
            rest = &rest[8..];
        }
        if rest.len() >= 4 {
            let half = u64::from(u32::from_le_bytes(rest[..4].try_into().expect("four bytes")));
            hash = (hash ^ half.wrapping_mul(P1)).rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
            rest = &rest[4..];
        }
        for &byte in rest {
            hash = (hash ^ u64::from(byte).wrapping_mul(P5)).rotate_left(11).wrapping_mul(P1);
        }
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(P2);
        hash ^= hash >> 29;
        hash = hash.wrapping_mul(P3);
        hash ^ (hash >> 32)
    }

    /// `xxh64:<16 hex digits>`, as a copy's proof is written.
    pub fn label(&self) -> String {
        format!("{XXH64}:{:016x}", self.digest())
    }
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `bytes` as 64 lower-case hex digits.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut message = bytes.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((bytes.len() as u64) * 8).to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes(chunk.try_into().expect("four bytes"));
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_known_answers_of_both_checksums() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let mut x = XxHash64::new();
        assert_eq!(x.digest(), 0xef46_db37_51d8_e999);
        x.update(b"a");
        assert_eq!(x.digest(), 0xd24e_c4f1_a98c_6e5b);
        let mut whole = XxHash64::new();
        whole.update(&[7u8; 100]);
        let mut pieces = XxHash64::new();
        for piece in [7u8; 100].chunks(13) {
            pieces.update(piece);
        }
        assert_eq!(whole.label(), pieces.label());
        assert!(whole.label().starts_with("xxh64:"));
    }
}
