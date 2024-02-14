pub mod xor;
pub use xor::Xor;

pub trait Transform {
    fn transform(&self, data: &mut [u8]);
}
