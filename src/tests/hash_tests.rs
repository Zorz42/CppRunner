use crate::hash::stable_hash;

#[test]
fn hashes_are_fixed() {
    assert_eq!(stable_hash(&[]), GOLDEN[0]);
    assert_eq!(stable_hash(&[b""]), GOLDEN[1]);
    assert_eq!(stable_hash(&[b"int main() {}"]), GOLDEN[2]);
}

#[test]
fn parts_are_kept_apart() {
    assert_ne!(stable_hash(&[b"ab", b"c"]), stable_hash(&[b"a", b"bc"]));
    assert_ne!(stable_hash(&[b"abc"]), stable_hash(&[b"abc", b""]));
    assert_ne!(stable_hash(&[]), stable_hash(&[b""]));
}

const GOLDEN: [u64; 3] = [0x2d06_8005_38d3_94c2, 0xc77b_3abb_6f87_acd9, 0x85c0_b39a_51ab_cb69];
