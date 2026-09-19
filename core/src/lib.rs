//! Platform independent parts of the merman3 viewer: syntax specification,
//! structural matching of JSON documents against the syntax, and the layout
//! engine (a port of merman's visual/wall/course model with its idle task
//! queue, hover and selection).
pub mod spec;
pub mod direction;
pub mod syntax;
pub mod document;
pub mod matcher;
pub mod measure;
pub mod serialize;
pub mod context;
pub mod visual;
pub mod wall;
pub mod alignment;
pub mod attachment;
pub mod cursor;
pub mod iteration;
pub mod input;
pub mod render;
