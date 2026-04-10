//! GPU compute pipeline — lazy-evaluated filter chain.
//!
//! Same OnceLock pattern as Frame, but for GPU textures.
//! Nothing runs until something downstream asks for it.
//! Even GPU shaders are lazy — won't compute edges unless an analyzer needs edges.
//! Won't compute quarter-res gray unless optical flow needs it.
//!
//! The GPU is fast, but it's not free. Don't burn shader cycles on
//! textures nobody will read this frame.

use std::sync::OnceLock;
use super::textures::*;

/// A GPU-resident frame with lazy-evaluated derived textures.
/// Mirrors the CPU Frame's OnceLock pattern, but everything stays on GPU.
///
/// The source texture is uploaded once (or provided directly by the camera API
/// as a hardware texture — zero-copy on iOS Metal / Android Vulkan).
/// Derived textures are computed on-demand by dispatching compute shaders.
pub struct GpuFrame {
    /// Source camera texture (uploaded or zero-copy from camera hardware)
    source: GpuTexture,

    /// Previous frame's grayscale at quarter res (for optical flow delta)
    prev_gray_quarter: Option<GpuTexture>,

    // --- Lazy-evaluated derived textures ---
    // None of these run until someone calls the getter.

    gray: OnceLock<GpuTexture>,
    gray_quarter: OnceLock<GpuTexture>,
    edges: OnceLock<GpuTexture>,
    flow: OnceLock<Option<GpuTexture>>,

    // --- Sparse readbacks (CPU-side geometric results) ---
    // These are tiny: a few hundred floats, not megapixel images.

    flow_readback: OnceLock<FlowReadback>,
    feature_readback: OnceLock<FeatureReadback>,
}

impl GpuFrame {
    pub fn new(source: GpuTexture, prev_gray_quarter: Option<GpuTexture>) -> Self {
        Self {
            source,
            prev_gray_quarter,
            gray: OnceLock::new(),
            gray_quarter: OnceLock::new(),
            edges: OnceLock::new(),
            flow: OnceLock::new(),
            flow_readback: OnceLock::new(),
            feature_readback: OnceLock::new(),
        }
    }

    /// Source texture — always available, zero-cost.
    pub fn source(&self) -> &GpuTexture {
        &self.source
    }

    /// Grayscale at full res — computed on first access via GPU shader.
    pub fn gray(&self) -> &GpuTexture {
        self.gray.get_or_init(|| {
            // TODO: dispatch RGB→gray compute shader
            // For now, return a placeholder with same dimensions
            GpuTexture::new(
                self.source.id() + 1000,
                self.source.width,
                self.source.height,
                TextureFormat::R8,
            )
        })
    }

    /// Grayscale at quarter res — for optical flow. Lazy.
    pub fn gray_quarter(&self) -> &GpuTexture {
        self.gray_quarter.get_or_init(|| {
            // Depends on gray() — which is also lazy
            let _full = self.gray();
            let (qw, qh) = self.source.quarter_res();
            // TODO: dispatch downsample compute shader (bilinear, single pass)
            GpuTexture::new(
                self.source.id() + 2000,
                qw, qh,
                TextureFormat::R8,
            )
        })
    }

    /// Edge detection (Sobel magnitude) — lazy, only if an analyzer needs edges.
    pub fn edges(&self) -> &GpuTexture {
        self.edges.get_or_init(|| {
            let _gray = self.gray();
            // TODO: dispatch Sobel compute shader (3x3 kernel, single pass)
            GpuTexture::new(
                self.source.id() + 3000,
                self.source.width,
                self.source.height,
                TextureFormat::R8,
            )
        })
    }

    /// Optical flow — lazy, needs previous frame's quarter-res gray.
    /// Returns None if no previous frame exists (first frame).
    pub fn flow(&self) -> Option<&GpuTexture> {
        self.flow.get_or_init(|| {
            let prev = self.prev_gray_quarter.as_ref()?;
            let curr = self.gray_quarter();
            let (qw, qh) = self.source.quarter_res();
            // TODO: dispatch Lucas-Kanade or Farneback flow compute shader
            // Input: prev quarter gray + curr quarter gray
            // Output: Rg16f texture (dx, dy per pixel)
            Some(GpuTexture::new(
                self.source.id() + 4000,
                qw, qh,
                TextureFormat::Rg16f,
            ))
        }).as_ref()
    }

    /// Sparse flow readback — THE ONLY THING THE CPU READS.
    /// GPU computes flow, then a reduction shader samples sparse grid points
    /// and computes overall magnitude. CPU gets back ~100 sample points + 1 float.
    /// NOT a full-res pixel download.
    pub fn flow_readback(&self) -> &FlowReadback {
        self.flow_readback.get_or_init(|| {
            if self.flow().is_none() {
                return FlowReadback {
                    samples: Vec::new(),
                    magnitude: 0.0,
                    dominant_direction: (0.0, 0.0),
                };
            }

            // TODO: dispatch reduction compute shader on flow texture
            // - Sample flow at 10x10 grid → 100 points
            // - Compute 75th percentile magnitude (parallel reduction)
            // - Compute dominant direction (weighted average)
            // - Read back ONE small buffer (~100 * 4 floats + 3 scalars)

            FlowReadback {
                samples: Vec::new(), // placeholder
                magnitude: 0.0,
                dominant_direction: (0.0, 0.0),
            }
        })
    }

    /// Sparse feature readback — GPU runs a corner/FAST detector,
    /// reads back point coordinates only. NOT the descriptor images.
    pub fn feature_readback(&self) -> &FeatureReadback {
        self.feature_readback.get_or_init(|| {
            let _gray = self.gray();
            // TODO: dispatch FAST/Harris corner detector compute shader
            // - GPU finds corners in the grayscale texture
            // - Atomic append to a small buffer (max 500 features)
            // - Read back: 500 * 3 floats = 6KB
            FeatureReadback {
                points: Vec::new(), // placeholder
            }
        })
    }

    /// Get the quarter-res gray texture for the NEXT frame's flow computation.
    /// This is how the pipeline chains: current frame's gray_quarter becomes
    /// next frame's prev_gray_quarter. The texture stays on GPU.
    pub fn gray_quarter_for_next_frame(&self) -> GpuTexture {
        *self.gray_quarter()
    }
}
