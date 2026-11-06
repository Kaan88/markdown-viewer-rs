//! The merman route as the engine would use it: real font metrics during layout,
//! then a small SVG repair pass, then resvg.
pub mod patch;
pub mod corpus;

#[cfg(feature = "r-merman")]
pub mod metrics;
#[cfg(feature = "r-merman")]
pub mod pipeline;
