pub mod xor;
pub use xor::Xor;

pub mod head;
pub use head::Head;

pub trait Transform {
    fn transform(&self, data: &mut [u8]);
}
pub type IFilter = dyn crate::filters::Transform + Send + Sync;
