pub mod hander;
pub mod tracker;

// TODO: Maybe enforce this to be a primitive number type? IDK
/// A handle for the handed ID
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct Id(pub usize);

impl Id {
    /// The maximum possible valid `max_id`
    pub const MAX_ID: Id = Id(usize::MAX);
}
