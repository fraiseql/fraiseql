//! Federation commands
pub mod check;
pub mod graph;
#[cfg(feature = "federation")]
pub mod sdl;

#[cfg(test)]
mod tests;
