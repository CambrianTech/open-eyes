//! GPU texture handles — the ONLY thing that flows through the pipeline for image data.
//!
//! A texture is an ID. Not bytes. Not pixels. An integer handle.
//! The GPU owns the memory. The CPU never downloads it for filter operations.
//! When the CPU needs geometric results from GPU work (e.g., optical flow vectors),
//! it reads back a TINY buffer of sparse points — not the full image.

/// Opaque GPU texture handle. This is what flows between pipeline stages.
/// The actual pixel data lives on the GPU and never crosses to CPU for filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuTexture {
    /// Internal GPU texture ID
    id: u64,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    /// Single channel (grayscale, flow magnitude, edges)
    R8,
    /// Two channels (optical flow dx, dy)
    Rg16f,
    /// Three channels (RGB, normals XYZ)
    Rgba8,
    /// YUV 4:2:0 (raw camera input on some platforms)
    Yuv420,
}

impl GpuTexture {
    pub fn new(id: u64, width: u32, height: u32, format: TextureFormat) -> Self {
        Self { id, width, height, format }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Quarter resolution (for optical flow tier-1 processing)
    pub fn quarter_res(&self) -> (u32, u32) {
        (self.width / 4, self.height / 4)
    }

    /// Half resolution (for feature detection)
    pub fn half_res(&self) -> (u32, u32) {
        (self.width / 2, self.height / 2)
    }
}

/// What the GPU pipeline produces after running filters on a camera frame.
/// These are texture HANDLES, not pixel data. The CPU sees IDs.
pub struct GpuFrameOutputs {
    /// The raw camera frame as a GPU texture
    pub source: GpuTexture,
    /// Grayscale at full resolution
    pub gray: GpuTexture,
    /// Grayscale at quarter resolution (for optical flow)
    pub gray_quarter: GpuTexture,
    /// Edge detection output (Sobel magnitude)
    pub edges: GpuTexture,
    /// Optical flow vectors (Rg16f — dx, dy per pixel at quarter res)
    pub flow: Option<GpuTexture>,
}

/// Sparse readback from GPU — geometric data extracted from textures.
/// This is what the CPU actually processes. Tiny compared to the texture.
pub struct FlowReadback {
    /// Sampled flow vectors at sparse grid points (not every pixel)
    /// Each entry: (pixel_x, pixel_y, flow_dx, flow_dy)
    pub samples: Vec<(f32, f32, f32, f32)>,
    /// Overall motion magnitude (computed on GPU via reduction shader)
    pub magnitude: f32,
    /// Dominant motion direction (computed on GPU)
    pub dominant_direction: (f32, f32),
}

/// Sparse feature readback — pixel coordinates of detected features.
/// GPU runs the detector, CPU gets back a list of (x, y, response) tuples.
pub struct FeatureReadback {
    pub points: Vec<(f32, f32, f32)>,  // x, y, response strength
}
