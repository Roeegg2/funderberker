pub mod hander;
pub mod tracker;

// TODO: Maybe enforce this to be a primitive number type? IDK
/// A handle for the handed ID
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Id(pub usize);

