pub struct Pow2<const N: i32>;

pub trait IsPow2 {}
impl IsPow2 for Pow2<1> {}
impl IsPow2 for Pow2<2> {}
impl IsPow2 for Pow2<4> {}
impl IsPow2 for Pow2<8> {}
impl IsPow2 for Pow2<16> {}
impl IsPow2 for Pow2<32> {}
impl IsPow2 for Pow2<64> {}
impl IsPow2 for Pow2<128> {}
impl IsPow2 for Pow2<256> {}
impl IsPow2 for Pow2<512> {}
impl IsPow2 for Pow2<1024> {}
impl IsPow2 for Pow2<2048> {}
impl IsPow2 for Pow2<4096> {}
impl IsPow2 for Pow2<8192> {}
impl IsPow2 for Pow2<16384> {}
impl IsPow2 for Pow2<32768> {}
impl IsPow2 for Pow2<65536> {}
