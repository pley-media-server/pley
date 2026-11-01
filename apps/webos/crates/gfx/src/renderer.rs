//! The GL half of the renderer: three tiny programs, one streamed vertex
//! buffer a frame, the atlas texture, picture textures and their uploads.
//!
//! Colours are premultiplied and the blend is `ONE, ONE_MINUS_SRC_ALPHA` for
//! colour and alpha alike, so alpha accumulates as coverage (`a + dst·(1 − a)`):
//! a half-transparent scrim over an opaque page leaves it opaque, and the video
//! plane under the interface shows only where the interface is transparent.
//! This leaves the same framebuffer as the straight-alpha separate blend
//! measured on LG's panels. Dithering is off
//! (on Mali it printed a dot pattern on faint gradients); no depth or stencil
//! buffer is used.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glow::HasContext;
use pley_base::{Color, DisplayList, ImageId, RgbaImage};

use crate::atlas::{self, Atlas};
use crate::frame::{Builder, Inputs, Mesh, Paint, TexRef, Textures, Vertex};
use crate::glyphs::GlyphSource;
use crate::images::ImageCache;
use crate::images::decode::premultiply;

/// Quads one draw call can address with 16-bit indices.
const MAX_QUADS_PER_DRAW: usize = 65536 / 4;

/// A made picture not drawn for this many frames loses its texture.
const RAW_KEEP_FRAMES: u64 = 3;

const ATTR_POS: u32 = 0;
const ATTR_UV: u32 = 1;
const ATTR_MASK: u32 = 2;
const ATTR_COLOR: u32 = 3;

const VERTEX_SHADER: &str = r#"
attribute vec2 a_pos;
attribute vec2 a_uv;
attribute vec2 a_mask;
attribute vec4 a_color;
uniform vec2 u_view;
varying highp vec2 v_uv;
varying highp vec2 v_mask;
varying mediump vec4 v_color;
void main() {
    v_uv = a_uv;
    v_mask = a_mask;
    v_color = a_color;
    gl_Position = vec4(a_pos.x * u_view.x - 1.0, 1.0 - a_pos.y * u_view.y, 0.0, 1.0);
}
"#;

/// Texture coordinates stay `highp` where the GPU has it: Midgard Malis
/// interpolate `mediump` in 16 bits, a texel off across a wide quad.
const FRAGMENT_HEADER: &str = r#"
#ifdef GL_ES
precision mediump float;
#ifdef GL_FRAGMENT_PRECISION_HIGH
#define TEXCOORD highp
#else
#define TEXCOORD mediump
#endif
#else
#define TEXCOORD
#endif
varying TEXCOORD vec2 v_uv;
varying TEXCOORD vec2 v_mask;
varying mediump vec4 v_color;
uniform sampler2D u_tex;
uniform sampler2D u_atlas;
"#;

const FRAGMENT_ATLAS: &str = "void main() { gl_FragColor = v_color * texture2D(u_atlas, v_mask).a; }";
const FRAGMENT_PICTURE: &str = "void main() { gl_FragColor = v_color * texture2D(u_tex, v_uv); }";
const FRAGMENT_PICTURE_MASKED: &str =
    "void main() { gl_FragColor = v_color * texture2D(u_tex, v_uv) * texture2D(u_atlas, v_mask).a; }";

/// Why the renderer couldn't start.
#[derive(Debug)]
pub struct GfxError(String);

impl fmt::Display for GfxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GfxError {}

/// What a frame cost and what it left to do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    pub quads: usize,
    pub draw_calls: usize,
    /// Pictures uploaded during this frame and their bytes.
    pub uploads: usize,
    pub upload_bytes: usize,
    /// Times the atlas filled up mid-frame and was emptied.
    pub atlas_resets: usize,
    /// CPU time in [`Renderer::render`]: building the quads, then issuing GL.
    pub build: Duration,
    pub submit: Duration,
    /// Draw another frame soon: pictures wait to upload, or became ready after
    /// this frame's display list was made.
    pub more: bool,
}

struct Program {
    id: glow::Program,
    view: Option<glow::UniformLocation>,
}

/// A made picture's texture.
struct RawTexture {
    texture: glow::Texture,
    width: u32,
    height: u32,
    last_used: u64,
}

/// A tiny target new textures are drawn into once, so the driver allocates
/// them then rather than in the middle of the next frame's first draw (a
/// 1280×720 backdrop cost 116 ms there on a 2019 set).
struct Warm {
    framebuffer: glow::Framebuffer,
    target: glow::Texture,
}

/// Draws display lists with GL.
pub struct Renderer {
    gl: Arc<glow::Context>,
    glyphs: Arc<dyn GlyphSource>,
    programs: [Program; 3],
    vbo: glow::Buffer,
    ibo: glow::Buffer,
    atlas: Atlas,
    atlas_texture: glow::Texture,
    raws: HashMap<u64, RawTexture>,
    mesh: Mesh,
    warm: Option<Warm>,
    size: (u32, u32),
    scale: f32,
    frame: u64,
    /// Bytes uploaded in the frame `upload_frame`.
    uploaded: usize,
    upload_frame: u64,
    stats: FrameStats,
}

impl Renderer {
    /// A renderer over `gl` (current on this thread) for a `size` surface at
    /// `scale` device pixels per dp, rasterising glyphs with `glyphs`.
    pub fn new(
        gl: Arc<glow::Context>,
        glyphs: Arc<dyn GlyphSource>,
        size: (u32, u32),
        scale: f32,
    ) -> Result<Renderer, GfxError> {
        Renderer::with_atlas(gl, glyphs, size, scale, atlas::DEFAULT_SIDE)
    }

    /// As [`Renderer::new`] with an atlas page of `atlas_side` texels.
    pub fn with_atlas(
        gl: Arc<glow::Context>,
        glyphs: Arc<dyn GlyphSource>,
        size: (u32, u32),
        scale: f32,
        atlas_side: u32,
    ) -> Result<Renderer, GfxError> {
        // SAFETY: the caller made the context current on this thread; every
        // object made here is used only with it.
        unsafe {
            let embedded = gl.version().is_embedded;
            // A core desktop profile draws nothing without a vertex array;
            // GLES 2 has none to bind.
            if !embedded && gl.version().major >= 3 {
                let vao = gl.create_vertex_array().map_err(GfxError)?;
                gl.bind_vertex_array(Some(vao));
            }
            let programs = [
                program(&gl, embedded, FRAGMENT_ATLAS)?,
                program(&gl, embedded, FRAGMENT_PICTURE)?,
                program(&gl, embedded, FRAGMENT_PICTURE_MASKED)?,
            ];
            let vbo = gl.create_buffer().map_err(GfxError)?;
            let ibo = gl.create_buffer().map_err(GfxError)?;
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
            let indices: Vec<u8> = (0..MAX_QUADS_PER_DRAW as u16)
                .flat_map(|q| {
                    let v = q * 4;
                    [v, v + 1, v + 2, v + 2, v + 1, v + 3]
                })
                .flat_map(u16::to_ne_bytes)
                .collect();
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &indices, glow::STATIC_DRAW);

            let atlas = Atlas::new(atlas_side);
            let atlas_texture = gl.create_texture().map_err(GfxError)?;
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas_texture));
            texture_params(&gl);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::ALPHA as i32,
                atlas.side() as i32,
                atlas.side() as i32,
                0,
                glow::ALPHA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.active_texture(glow::TEXTURE0);

            gl.disable(glow::DITHER);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);

            let warm = make_warm(&gl);
            let mut renderer = Renderer {
                gl,
                glyphs,
                programs,
                vbo,
                ibo,
                atlas,
                atlas_texture,
                raws: HashMap::new(),
                mesh: Mesh::default(),
                warm,
                size: (0, 0),
                scale,
                frame: 0,
                uploaded: 0,
                upload_frame: u64::MAX,
                stats: FrameStats::default(),
            };
            renderer.resize(size.0, size.1, scale);
            Ok(renderer)
        }
    }

    /// The surface's new size in device pixels and device pixels per dp.
    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {
        self.size = (width.max(1), height.max(1));
        self.scale = scale;
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        // SAFETY: the context is current (the renderer's contract).
        unsafe {
            for p in &self.programs {
                self.gl.use_program(Some(p.id));
                self.gl.uniform_2_f32(p.view.as_ref(), 2.0 / w, 2.0 / h);
            }
        }
    }

    /// Device pixels per dp.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Uploads decoded pictures within this frame's budget
    /// ([`ImageCacheConfig::upload_budget`](crate::ImageCacheConfig)), so those
    /// that make it can be drawn in the display list built next. Call it before
    /// building the frame's list; [`Renderer::render`] uploads what budget is
    /// left. Returns whether decoded pictures still wait.
    pub fn prepare(&mut self, images: &mut ImageCache) -> bool {
        images.pump();
        if self.upload_frame != self.frame {
            self.upload_frame = self.frame;
            self.uploaded = 0;
        }
        let budget = images.config().upload_budget;
        let mut uploaded = Vec::new();
        while let Some(bytes) = images.next_upload_bytes() {
            if self.uploaded > 0 && self.uploaded + bytes > budget {
                break;
            }
            let Some((req, picture)) = images.take_upload() else { break };
            // SAFETY: the context is current (the renderer's contract).
            let texture = unsafe { self.upload_rgba(picture.width, picture.height, &picture.pixels) };
            let Some(texture) = texture else { continue };
            images.install(req, texture, picture.width, picture.height);
            self.uploaded += bytes;
            self.stats.uploads += 1;
            self.stats.upload_bytes += bytes;
            uploaded.push(texture);
        }
        // SAFETY: as above.
        unsafe {
            self.warm_up(&uploaded);
            for texture in images.evict() {
                self.gl.delete_texture(texture);
            }
        }
        images.has_uploads()
    }

    /// Draws `list` into the current framebuffer (call [`Renderer::prepare`]
    /// first to get this frame's uploads into the list).
    pub fn render(&mut self, list: &DisplayList, images: &mut ImageCache) -> FrameStats {
        let uploads_before = self.stats.uploads;
        let waiting = self.prepare(images);
        let late_uploads = self.stats.uploads > uploads_before;
        let start = Instant::now();
        let mut submit = Duration::ZERO;
        // SAFETY: the context is current (the renderer's contract).
        unsafe {
            let gl = &self.gl;
            gl.viewport(0, 0, self.size.0 as i32, self.size.1 as i32);
            let c = premultiplied(list.clear);
            gl.clear_color(c[0], c[1], c[2], c[3]);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        let mut builder = Builder::new(self.size.0, self.size.1, self.scale);
        let mut from = 0;
        let mut last_stop = usize::MAX;
        loop {
            self.mesh.clear();
            let result = {
                let mut textures =
                    FrameTextures { gl: &self.gl, images: &mut *images, raws: &mut self.raws, frame: self.frame };
                let mut cx = Inputs { atlas: &mut self.atlas, glyphs: &*self.glyphs, textures: &mut textures };
                builder.run(&list.cmds, from, &mut self.mesh, &mut cx)
            };
            let t = Instant::now();
            self.draw_mesh(images);
            submit += t.elapsed();
            match result {
                Ok(()) => break,
                Err(stop) => {
                    // The atlas filled up: what was built is drawn; start the
                    // page afresh and go on from the command that stopped. A
                    // command that stops twice in a row gets what fits.
                    builder.lenient = stop == last_stop;
                    last_stop = stop;
                    from = stop;
                    self.atlas.reset();
                    self.stats.atlas_resets += 1;
                }
            }
        }
        self.end_frame(images);
        let mut stats = std::mem::take(&mut self.stats);
        stats.build = start.elapsed().saturating_sub(submit);
        stats.submit = submit;
        stats.more = waiting || late_uploads;
        stats
    }

    fn end_frame(&mut self, images: &mut ImageCache) {
        images.end_frame();
        let frame = self.frame;
        let gl = &self.gl;
        self.raws.retain(|_, r| {
            let keep = frame - r.last_used < RAW_KEEP_FRAMES;
            if !keep {
                // SAFETY: the context is current (the renderer's contract).
                unsafe { gl.delete_texture(r.texture) };
            }
            keep
        });
        self.frame += 1;
    }

    /// Uploads the atlas's changed rows and the vertices, then issues one draw
    /// per batch.
    fn draw_mesh(&mut self, images: &mut ImageCache) {
        if self.mesh.batches.is_empty() {
            return;
        }
        let gl = &self.gl;
        // SAFETY: the context is current; the vertex bytes are plain `repr(C)`
        // data without padding, read only for the length of the call.
        unsafe {
            let bands = self.atlas.take_dirty();
            if !bands.is_empty() {
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas_texture));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                for (from, to) in bands {
                    gl.tex_sub_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        from as i32,
                        self.atlas.side() as i32,
                        (to - from) as i32,
                        glow::ALPHA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(self.atlas.rows(from, to))),
                    );
                }
                gl.active_texture(glow::TEXTURE0);
            }
            let vertices = &self.mesh.vertices;
            let bytes =
                std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(&vertices[..]));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            // A fresh store each time: the driver needn't wait for draws still
            // reading the last one.
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));
            for attr in [ATTR_POS, ATTR_UV, ATTR_MASK, ATTR_COLOR] {
                gl.enable_vertex_attrib_array(attr);
            }
            let mut program = usize::MAX;
            let mut texture = None;
            let mut blending = true;
            for batch in &self.mesh.batches {
                let (p, tex) = match batch.paint {
                    Paint::Atlas | Paint::Punch => (0, None),
                    Paint::Picture(t) => (1, Some(t)),
                    Paint::PictureMasked(t) => (2, Some(t)),
                };
                if let Some(t) = tex {
                    let found = match t {
                        TexRef::Image(id) => images.texture(id).map(|(tex, _, _)| tex),
                        TexRef::Raw(key) => self.raws.get(&key).map(|r| r.texture),
                    };
                    let Some(found) = found else { continue };
                    if texture != Some(found) {
                        gl.bind_texture(glow::TEXTURE_2D, Some(found));
                        texture = Some(found);
                    }
                }
                if p != program {
                    gl.use_program(Some(self.programs[p].id));
                    program = p;
                }
                let punch = batch.paint == Paint::Punch;
                if punch == blending {
                    if punch {
                        gl.disable(glow::BLEND);
                    } else {
                        gl.enable(glow::BLEND);
                    }
                    blending = !punch;
                }
                let mut first = batch.first as usize;
                let mut left = batch.quads as usize;
                while left > 0 {
                    let n = left.min(MAX_QUADS_PER_DRAW);
                    set_attributes(gl, first * 4 * std::mem::size_of::<Vertex>());
                    gl.draw_elements(glow::TRIANGLES, (n * 6) as i32, glow::UNSIGNED_SHORT, 0);
                    self.stats.draw_calls += 1;
                    first += n;
                    left -= n;
                }
            }
            if !blending {
                gl.enable(glow::BLEND);
            }
            self.stats.quads += self.mesh.quads();
        }
    }

    /// A texture holding premultiplied RGBA pixels.
    unsafe fn upload_rgba(&self, width: u32, height: u32, pixels: &[u8]) -> Option<glow::Texture> {
        let gl = &self.gl;
        // SAFETY: the caller holds the context current.
        unsafe {
            let texture = gl.create_texture().ok()?;
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            texture_params(gl);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(pixels)),
            );
            Some(texture)
        }
    }

    /// Draws each new texture once into the warm-up target.
    unsafe fn warm_up(&self, textures: &[glow::Texture]) {
        let (Some(warm), false) = (&self.warm, textures.is_empty()) else { return };
        let gl = &self.gl;
        // SAFETY: the caller holds the context current.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(warm.framebuffer));
            gl.viewport(0, 0, 4, 4);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(self.programs[1].id));
            let (w, h) = (self.size.0 as f32, self.size.1 as f32);
            let quad = [
                Vertex { pos: [0.0, 0.0], uv: [0.0, 0.0], mask: [0.0; 2], color: [255; 4] },
                Vertex { pos: [w, 0.0], uv: [1.0, 0.0], mask: [0.0; 2], color: [255; 4] },
                Vertex { pos: [0.0, h], uv: [0.0, 1.0], mask: [0.0; 2], color: [255; 4] },
                Vertex { pos: [w, h], uv: [1.0, 1.0], mask: [0.0; 2], color: [255; 4] },
            ];
            let bytes = std::slice::from_raw_parts(quad.as_ptr().cast::<u8>(), std::mem::size_of_val(&quad));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));
            for attr in [ATTR_POS, ATTR_UV, ATTR_MASK, ATTR_COLOR] {
                gl.enable_vertex_attrib_array(attr);
            }
            set_attributes(gl, 0);
            for texture in textures {
                gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_SHORT, 0);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, self.size.0 as i32, self.size.1 as i32);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // SAFETY: dropped on the thread that owns the context, like every use.
        unsafe {
            let gl = &self.gl;
            for p in &self.programs {
                gl.delete_program(p.id);
            }
            gl.delete_buffer(self.vbo);
            gl.delete_buffer(self.ibo);
            gl.delete_texture(self.atlas_texture);
            for r in self.raws.values() {
                gl.delete_texture(r.texture);
            }
            if let Some(w) = &self.warm {
                gl.delete_framebuffer(w.framebuffer);
                gl.delete_texture(w.target);
            }
        }
    }
}

/// Textures as the frame builder sees them: the cache's pictures and the
/// renderer's made ones, uploaded on first sight of their key.
struct FrameTextures<'a> {
    gl: &'a glow::Context,
    images: &'a mut ImageCache,
    raws: &'a mut HashMap<u64, RawTexture>,
    frame: u64,
}

impl Textures for FrameTextures<'_> {
    fn image(&mut self, id: ImageId) -> Option<(u32, u32)> {
        self.images.texture(id).map(|(_, w, h)| (w, h))
    }

    fn raw(&mut self, key: u64, image: &RgbaImage) -> Option<(u32, u32)> {
        if let Some(r) = self.raws.get_mut(&key) {
            r.last_used = self.frame;
            return Some((r.width, r.height));
        }
        if image.width == 0 || image.height == 0 || image.pixels.len() != (image.width * image.height * 4) as usize {
            return None;
        }
        let mut pixels = image.pixels.clone();
        premultiply(&mut pixels);
        let gl = self.gl;
        // SAFETY: the context is current (the renderer's contract).
        let texture = unsafe {
            let texture = gl.create_texture().ok()?;
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            texture_params(gl);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                image.width as i32,
                image.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&pixels)),
            );
            texture
        };
        self.raws.insert(key, RawTexture { texture, width: image.width, height: image.height, last_used: self.frame });
        Some((image.width, image.height))
    }
}

fn premultiplied(c: Color) -> [f32; 4] {
    let a = c.a as f32 / 255.0;
    [c.r as f32 / 255.0 * a, c.g as f32 / 255.0 * a, c.b as f32 / 255.0 * a, a]
}

/// Linear filtering, clamped edges, no mipmaps: pictures come at the size they
/// are drawn.
unsafe fn texture_params(gl: &glow::Context) {
    // SAFETY: the caller holds the context current with a texture bound.
    unsafe {
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    }
}

/// Points the attributes at the vertex buffer from byte `base` on.
unsafe fn set_attributes(gl: &glow::Context, base: usize) {
    let stride = std::mem::size_of::<Vertex>() as i32;
    let base = base as i32;
    // SAFETY: the caller holds the context current with the vertex buffer bound.
    unsafe {
        gl.vertex_attrib_pointer_f32(ATTR_POS, 2, glow::FLOAT, false, stride, base);
        gl.vertex_attrib_pointer_f32(ATTR_UV, 2, glow::FLOAT, false, stride, base + 8);
        gl.vertex_attrib_pointer_f32(ATTR_MASK, 2, glow::FLOAT, false, stride, base + 16);
        gl.vertex_attrib_pointer_f32(ATTR_COLOR, 4, glow::UNSIGNED_BYTE, true, stride, base + 24);
    }
}

/// Compiles one program: the shared vertex shader and a fragment body, as
/// GLSL ES 1.00 on GLES and as GLSL 1.20 (precision words defined away) on
/// desktop GL.
unsafe fn program(gl: &glow::Context, embedded: bool, fragment_main: &str) -> Result<Program, GfxError> {
    let prelude =
        if embedded { "#version 100\n" } else { "#version 120\n#define lowp\n#define mediump\n#define highp\n" };
    let vertex = format!("{prelude}{VERTEX_SHADER}");
    let fragment = format!("{prelude}{FRAGMENT_HEADER}{fragment_main}\n");
    // SAFETY: the caller holds the context current.
    unsafe {
        let program = gl.create_program().map_err(GfxError)?;
        let mut shaders = Vec::new();
        for (kind, source) in [(glow::VERTEX_SHADER, &vertex), (glow::FRAGMENT_SHADER, &fragment)] {
            let shader = gl.create_shader(kind).map_err(GfxError)?;
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(GfxError(format!("shader: {}", gl.get_shader_info_log(shader))));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        for (loc, name) in [(ATTR_POS, "a_pos"), (ATTR_UV, "a_uv"), (ATTR_MASK, "a_mask"), (ATTR_COLOR, "a_color")] {
            gl.bind_attrib_location(program, loc, name);
        }
        gl.link_program(program);
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        if !gl.get_program_link_status(program) {
            return Err(GfxError(format!("program: {}", gl.get_program_info_log(program))));
        }
        gl.use_program(Some(program));
        if let Some(t) = gl.get_uniform_location(program, "u_tex") {
            gl.uniform_1_i32(Some(&t), 0);
        }
        if let Some(a) = gl.get_uniform_location(program, "u_atlas") {
            gl.uniform_1_i32(Some(&a), 1);
        }
        let view = gl.get_uniform_location(program, "u_view");
        Ok(Program { id: program, view })
    }
}

/// The warm-up target, if the driver makes one.
unsafe fn make_warm(gl: &glow::Context) -> Option<Warm> {
    // SAFETY: the caller holds the context current.
    unsafe {
        let target = gl.create_texture().ok()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(target));
        texture_params(gl);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            4,
            4,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        let framebuffer = gl.create_framebuffer().ok()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target), 0);
        let complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        if complete {
            Some(Warm { framebuffer, target })
        } else {
            gl.delete_framebuffer(framebuffer);
            gl.delete_texture(target);
            None
        }
    }
}
