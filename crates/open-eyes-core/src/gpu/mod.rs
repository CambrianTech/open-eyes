//! GPU compute pipeline — all image-space work stays on GPU.
//!
//! RULE: The CPU never touches pixels for filter operations.
//! Raw camera texture → GPU shaders → results stay as GPU textures.
//! CPU only reads geometric outputs (feature coords, flow vectors, plane params).
//!
//! This mirrors CBAR's approach: optical flow at quarter-res on GPU every frame,
//! color conversion on GPU, edge detection on GPU. The CPU does geometry (RANSAC,
//! triangulation, tracking) on sparse point data, never on dense pixel grids.

pub mod compute;
pub mod textures;
