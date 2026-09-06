//! Platform independent parts of the merman3 viewer: syntax specification,
//! structural matching of JSON documents against the syntax, and text layout.
pub mod spec;
pub mod direction;
pub mod syntax;
pub mod document;
pub mod matcher;
pub mod measure;
pub mod visual;
pub mod layout;
