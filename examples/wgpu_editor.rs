use bytemuck::{Pod, Zeroable};
use glyphon::cosmic_text::Shaping;
use glyphon::{
    Attrs, Buffer, Cache, Color as GlyphonColor, Cursor, Family, FontSystem, Metrics, Motion,
    Resolution, Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use hollandaise::{Editor, InlineFormat, LocalBackend};
use std::sync::Arc;
use wgpu::{self, util::DeviceExt};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::EventLoop,
    keyboard::{Key, ModifiersState, NamedKey},
    window::Window,
};

// --- CoordinateMapper ---

struct CoordinateMapper {
    line_char_offsets: Vec<usize>,
    line_texts: Vec<String>,
    total_chars: usize,
}

impl CoordinateMapper {
    fn build(text: &str) -> Self {
        let mut line_char_offsets = Vec::new();
        let mut line_texts = Vec::new();
        let mut char_offset = 0;

        for line in text.split('\n') {
            line_char_offsets.push(char_offset);
            line_texts.push(line.to_string());
            char_offset += line.chars().count() + 1; // +1 for the \n
        }

        let total_chars = text.chars().count();

        if line_char_offsets.is_empty() {
            line_char_offsets.push(0);
            line_texts.push(String::new());
        }

        assert!(
            !line_char_offsets.is_empty(),
            "CoordinateMapper must have at least one line"
        );
        assert_eq!(
            line_char_offsets.len(),
            line_texts.len(),
            "line offsets and texts must have same length"
        );

        Self {
            line_char_offsets,
            line_texts,
            total_chars,
        }
    }

    fn char_to_cosmic(&self, char_pos: usize) -> Cursor {
        assert!(
            char_pos <= self.total_chars,
            "char_pos {} exceeds total_chars {}",
            char_pos,
            self.total_chars
        );

        // Find which line this char_pos falls on
        let line_idx = self
            .line_char_offsets
            .partition_point(|&offset| offset <= char_pos)
            .saturating_sub(1);

        assert!(
            line_idx < self.line_texts.len(),
            "line_idx {} out of bounds ({})",
            line_idx,
            self.line_texts.len()
        );

        let line_start = self.line_char_offsets[line_idx];
        let char_within_line = char_pos.saturating_sub(line_start);
        let line_text = &self.line_texts[line_idx];
        let line_char_count = line_text.chars().count();

        // Clamp to end of line (char_pos might be at the \n boundary)
        let clamped = char_within_line.min(line_char_count);

        // Convert char offset within line to byte offset
        let byte_index: usize = line_text.chars().take(clamped).map(|c| c.len_utf8()).sum();

        assert!(
            byte_index <= line_text.len(),
            "byte_index {} exceeds line length {}",
            byte_index,
            line_text.len()
        );

        Cursor::new(line_idx, byte_index)
    }

    fn cosmic_to_char(&self, cursor: Cursor) -> usize {
        let line_idx = cursor.line;
        assert!(
            line_idx < self.line_texts.len(),
            "cursor line {} out of bounds ({})",
            line_idx,
            self.line_texts.len()
        );

        let line_text = &self.line_texts[line_idx];
        let byte_index = cursor.index;

        assert!(
            byte_index <= line_text.len(),
            "cursor byte index {} exceeds line byte length {}",
            byte_index,
            line_text.len()
        );
        assert!(
            line_text.is_char_boundary(byte_index),
            "cursor byte index {} is not a char boundary",
            byte_index,
        );

        let char_within_line = line_text[..byte_index].chars().count();
        let result = self.line_char_offsets[line_idx] + char_within_line;

        assert!(
            result <= self.total_chars,
            "cosmic_to_char result {} exceeds total_chars {}",
            result,
            self.total_chars,
        );

        result
    }

    fn line_count(&self) -> usize {
        self.line_texts.len()
    }
}

// --- CursorState ---

struct CursorState {
    position: usize,
    anchor: Option<usize>,
    cosmic_cursor: Cursor,
    cosmic_x_opt: Option<i32>,
}

impl CursorState {
    fn new() -> Self {
        Self {
            position: 0,
            anchor: None,
            cosmic_cursor: Cursor::new(0, 0),
            cosmic_x_opt: None,
        }
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        self.anchor.map(|anchor| {
            let start = anchor.min(self.position);
            let end = anchor.max(self.position);
            (start, end)
        })
    }

    fn clear_selection(&mut self) {
        self.anchor = None;
    }

    fn delete_selection_or(
        &mut self,
        editor: &mut Editor<LocalBackend>,
    ) -> bool {
        if let Some((start, end)) = self.selection_range() {
            if start < end {
                editor.delete_range(start, end).unwrap();
                self.position = start;
                self.clear_selection();
                return true;
            }
        }
        false
    }
}

// --- RectRenderer ---

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct RectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

struct ColoredRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
}

struct RectRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
}

impl RectRenderer {
    fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader_src = r#"
struct Uniforms {
    screen_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc_x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rect_uniform_buffer"),
            contents: bytemuck::cast_slice(&[800.0_f32, 600.0_f32]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rect_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rect_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
        }
    }

    fn update_screen_size(&self, queue: &wgpu::Queue, width: f32, height: f32) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[width, height]),
        );
    }

    fn draw_rects(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        rects: &[ColoredRect],
    ) {
        if rects.is_empty() {
            return;
        }

        let mut vertices: Vec<RectVertex> = Vec::with_capacity(rects.len() * 6);
        for rect in rects {
            let x0 = rect.x;
            let y0 = rect.y;
            let x1 = rect.x + rect.width;
            let y1 = rect.y + rect.height;
            let c = rect.color;

            // Two triangles per rect
            vertices.push(RectVertex { position: [x0, y0], color: c });
            vertices.push(RectVertex { position: [x1, y0], color: c });
            vertices.push(RectVertex { position: [x0, y1], color: c });
            vertices.push(RectVertex { position: [x0, y1], color: c });
            vertices.push(RectVertex { position: [x1, y0], color: c });
            vertices.push(RectVertex { position: [x1, y1], color: c });
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rect_vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }
}

// --- WindowState ---

const PADDING: f32 = 20.0;
const FONT_SIZE: f32 = 20.0;
const LINE_HEIGHT: f32 = 28.0;
const BG_COLOR: wgpu::Color = wgpu::Color {
    r: 0.1,
    g: 0.1,
    b: 0.12,
    a: 1.0,
};
const TEXT_COLOR: GlyphonColor = GlyphonColor::rgb(220, 220, 220);
const CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.9, 1.0];
const SELECTION_COLOR: [f32; 4] = [0.2, 0.4, 0.8, 0.4];

struct WindowState {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,

    font_system: FontSystem,
    swash_cache: SwashCache,
    glyphon_cache: Cache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffer: Buffer,

    rect_renderer: RectRenderer,

    editor: Editor<LocalBackend>,
    cursor_state: CursorState,
    coord_mapper: CoordinateMapper,
    modifiers: ModifiersState,
    last_mouse_position: (f64, f64),
}

impl WindowState {
    fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("failed to find a suitable GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: Default::default(),
                experimental_features: Default::default(),
                trace: Default::default(),
            },
        ))
        .expect("failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let glyphon_cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &glyphon_cache);
        let mut atlas = TextAtlas::new(&device, &queue, &glyphon_cache, surface_format);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );

        let mut text_buffer = Buffer::new(&mut font_system, Metrics::new(FONT_SIZE, LINE_HEIGHT));
        text_buffer.set_size(
            &mut font_system,
            Some(width as f32 - PADDING * 2.0),
            Some(height as f32 - PADDING * 2.0),
        );

        let rect_renderer = RectRenderer::new(&device, surface_format);

        let mut editor = Editor::new(LocalBackend::new());
        editor
            .insert_text(0, "Hello, hollandaise!\nThis is a rich text editor.\nTry typing, or use Ctrl+B for bold and Ctrl+I for italic.")
            .unwrap();
        editor
            .toggle_format(InlineFormat::Strong, 0, 19)
            .unwrap();

        let text = editor.text().unwrap();
        let coord_mapper = CoordinateMapper::build(&text);
        let cursor_state = CursorState::new();

        let mut state = Self {
            window,
            device,
            queue,
            surface,
            surface_config,
            font_system,
            swash_cache,
            glyphon_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffer,
            rect_renderer,
            editor,
            cursor_state,
            coord_mapper,
            modifiers: ModifiersState::empty(),
            last_mouse_position: (0.0, 0.0),
        };

        state.rebuild_text_buffer();
        state
    }

    fn rebuild_text_buffer(&mut self) {
        let spans = self.editor.spans().unwrap();
        let rich_text: Vec<(&str, Attrs)> = spans
            .iter()
            .map(|span| {
                let mut attrs = Attrs::new().family(Family::SansSerif).color(TEXT_COLOR);
                if span.formats.strong {
                    attrs = attrs.weight(Weight::BOLD);
                }
                if span.formats.emphasis {
                    attrs = attrs.style(Style::Italic);
                }
                if span.formats.code {
                    attrs = attrs.family(Family::Monospace);
                }
                (span.text.as_str(), attrs)
            })
            .collect();

        let default_attrs = Attrs::new().family(Family::SansSerif).color(TEXT_COLOR);
        self.text_buffer.set_rich_text(
            &mut self.font_system,
            rich_text,
            &default_attrs,
            Shaping::Advanced,
            None,
        );
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }

    fn sync_after_edit(&mut self) {
        let text = self.editor.text().unwrap();
        self.coord_mapper = CoordinateMapper::build(&text);
        let total = self.coord_mapper.total_chars;
        self.cursor_state.position = self.cursor_state.position.min(total);
        self.cursor_state.cosmic_cursor =
            self.coord_mapper.char_to_cosmic(self.cursor_state.position);
        self.cursor_state.cosmic_x_opt = None;
        self.rebuild_text_buffer();
    }

    fn handle_key_event(&mut self, event: &KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }

        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();

        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                if !self.cursor_state.delete_selection_or(&mut self.editor) {
                    let pos = self.cursor_state.position;
                    if pos > 0 {
                        self.editor.delete_range(pos - 1, pos).unwrap();
                        self.cursor_state.position = pos - 1;
                    }
                }
                self.cursor_state.clear_selection();
                self.sync_after_edit();
            }
            Key::Named(NamedKey::Delete) => {
                if !self.cursor_state.delete_selection_or(&mut self.editor) {
                    let pos = self.cursor_state.position;
                    let total = self.coord_mapper.total_chars;
                    if pos < total {
                        self.editor.delete_range(pos, pos + 1).unwrap();
                    }
                }
                self.cursor_state.clear_selection();
                self.sync_after_edit();
            }
            Key::Named(NamedKey::Enter) => {
                self.cursor_state.delete_selection_or(&mut self.editor);
                let pos = self.cursor_state.position;
                self.editor.insert_text(pos, "\n").unwrap();
                self.cursor_state.position = pos + 1;
                self.cursor_state.clear_selection();
                self.sync_after_edit();
            }
            Key::Named(named) if is_arrow_key(named) => {
                let motion = arrow_to_motion(named, ctrl);
                self.move_cursor(motion, shift);
            }
            Key::Named(NamedKey::Home) => {
                self.move_cursor(Motion::Home, shift);
            }
            Key::Named(NamedKey::End) => {
                self.move_cursor(Motion::End, shift);
            }
            Key::Character(ch) if ctrl => {
                match ch.as_str() {
                    "a" => {
                        self.cursor_state.anchor = Some(0);
                        self.cursor_state.position = self.coord_mapper.total_chars;
                        self.cursor_state.cosmic_cursor =
                            self.coord_mapper.char_to_cosmic(self.cursor_state.position);
                        self.cursor_state.cosmic_x_opt = None;
                    }
                    "b" => {
                        if let Some((start, end)) = self.cursor_state.selection_range() {
                            if start < end {
                                self.editor
                                    .toggle_format(InlineFormat::Strong, start, end)
                                    .unwrap();
                                self.rebuild_text_buffer();
                            }
                        }
                    }
                    "i" => {
                        if let Some((start, end)) = self.cursor_state.selection_range() {
                            if start < end {
                                self.editor
                                    .toggle_format(InlineFormat::Emphasis, start, end)
                                    .unwrap();
                                self.rebuild_text_buffer();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Key::Character(ch) if !ctrl => {
                self.cursor_state.delete_selection_or(&mut self.editor);
                let pos = self.cursor_state.position;
                let text = ch.as_str();
                self.editor.insert_text(pos, text).unwrap();
                self.cursor_state.position = pos + text.chars().count();
                self.cursor_state.clear_selection();
                self.sync_after_edit();
            }
            _ => {}
        }
    }

    fn move_cursor(&mut self, motion: Motion, extend_selection: bool) {
        if extend_selection && self.cursor_state.anchor.is_none() {
            self.cursor_state.anchor = Some(self.cursor_state.position);
        }

        let result = self.text_buffer.cursor_motion(
            &mut self.font_system,
            self.cursor_state.cosmic_cursor,
            self.cursor_state.cosmic_x_opt,
            motion,
        );

        if let Some((new_cosmic, new_x_opt)) = result {
            self.cursor_state.cosmic_cursor = new_cosmic;
            self.cursor_state.cosmic_x_opt = new_x_opt;

            // Clamp line to valid range before converting
            let line = new_cosmic.line.min(self.coord_mapper.line_count().saturating_sub(1));
            let clamped_cursor = Cursor::new(line, new_cosmic.index);
            self.cursor_state.position = self.coord_mapper.cosmic_to_char(clamped_cursor);
        }

        if !extend_selection {
            self.cursor_state.clear_selection();
        }
    }

    fn handle_mouse_click(&mut self, x: f64, y: f64) {
        let buffer_x = x as f32 - PADDING;
        let buffer_y = y as f32 - PADDING;

        if let Some(cursor) = self.text_buffer.hit(buffer_x, buffer_y) {
            let line = cursor.line.min(self.coord_mapper.line_count().saturating_sub(1));
            let clamped = Cursor::new(line, cursor.index);
            self.cursor_state.cosmic_cursor = clamped;
            self.cursor_state.cosmic_x_opt = None;
            self.cursor_state.position = self.coord_mapper.cosmic_to_char(clamped);
            self.cursor_state.clear_selection();
        }
    }

    fn compute_cursor_rects(&self) -> Vec<ColoredRect> {
        let mut rects = Vec::new();
        let cursor = self.cursor_state.cosmic_cursor;

        // Find the cursor caret position from layout runs
        for run in self.text_buffer.layout_runs() {
            if run.line_i != cursor.line {
                continue;
            }

            let mut caret_x = 0.0_f32;

            // Find the x position of the cursor within this run
            for glyph in run.glyphs.iter() {
                if cursor.index <= glyph.start {
                    caret_x = glyph.x;
                    break;
                }
                if cursor.index >= glyph.start && cursor.index < glyph.end {
                    // Cursor is within this glyph
                    caret_x = glyph.x;
                    break;
                }
                caret_x = glyph.x + glyph.w;
            }

            rects.push(ColoredRect {
                x: PADDING + caret_x,
                y: PADDING + run.line_top,
                width: 2.0,
                height: run.line_height,
                color: CURSOR_COLOR,
            });
            break;
        }

        // If no layout run matched (empty buffer or cursor at end), put cursor at origin
        if rects.is_empty() {
            let y_offset = cursor.line as f32 * LINE_HEIGHT;
            rects.push(ColoredRect {
                x: PADDING,
                y: PADDING + y_offset,
                width: 2.0,
                height: LINE_HEIGHT,
                color: CURSOR_COLOR,
            });
        }

        rects
    }

    fn compute_selection_rects(&self) -> Vec<ColoredRect> {
        let (start, end) = match self.cursor_state.selection_range() {
            Some(range) if range.0 < range.1 => range,
            _ => return Vec::new(),
        };

        let cursor_start = self.coord_mapper.char_to_cosmic(start);
        let cursor_end = self.coord_mapper.char_to_cosmic(end);
        let mut rects = Vec::new();

        for run in self.text_buffer.layout_runs() {
            if let Some((x_left, x_width)) = run.highlight(cursor_start, cursor_end) {
                if x_width > 0.0 {
                    rects.push(ColoredRect {
                        x: PADDING + x_left,
                        y: PADDING + run.line_top,
                        width: x_width,
                        height: run.line_height,
                        color: SELECTION_COLOR,
                    });
                }
            }
        }

        rects
    }

    fn render(&mut self) {
        let width = self.surface_config.width;
        let height = self.surface_config.height;

        self.viewport.update(
            &self.queue,
            Resolution {
                width,
                height,
            },
        );

        self.rect_renderer
            .update_screen_size(&self.queue, width as f32, height as f32);

        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                [TextArea {
                    buffer: &self.text_buffer,
                    left: PADDING,
                    top: PADDING,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: width as i32,
                        bottom: height as i32,
                    },
                    default_color: TEXT_COLOR,
                    custom_glyphs: &[],
                }],
                &mut self.swash_cache,
            )
            .unwrap();

        let surface_texture = self.surface.get_current_texture().unwrap();
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        let selection_rects = self.compute_selection_rects();
        let cursor_rects = self.compute_cursor_rects();

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BG_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Selection behind text
            self.rect_renderer
                .draw_rects(&self.device, &mut pass, &selection_rects);

            // Text
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .unwrap();

            // Cursor on top
            self.rect_renderer
                .draw_rects(&self.device, &mut pass, &cursor_rects);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        self.atlas.trim();
    }

    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        self.text_buffer.set_size(
            &mut self.font_system,
            Some(width as f32 - PADDING * 2.0),
            Some(height as f32 - PADDING * 2.0),
        );
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }
}

// --- Application ---

struct Application {
    state: Option<WindowState>,
}

impl Application {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title("hollandaise — wgpu rich text editor")
            .with_inner_size(winit::dpi::LogicalSize::new(900, 600));
        let window = Arc::new(event_loop.create_window(window_attrs).unwrap());
        self.state = Some(WindowState::new(window.clone()));
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                state.resize(size.width, size.height);
                state.request_redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.last_mouse_position = (position.x, position.y);
            }
            WindowEvent::MouseInput {
                state: btn_state,
                button: MouseButton::Left,
                ..
            } if btn_state == ElementState::Pressed => {
                let (x, y) = state.last_mouse_position;
                state.handle_mouse_click(x, y);
                state.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                state.handle_key_event(&event);
                state.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                state.render();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        // Keep redrawing while we have state (cursor blink etc in the future)
    }
}

fn is_arrow_key(key: &NamedKey) -> bool {
    matches!(
        key,
        NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::ArrowUp | NamedKey::ArrowDown
    )
}

fn arrow_to_motion(key: &NamedKey, ctrl: bool) -> Motion {
    match (key, ctrl) {
        (NamedKey::ArrowLeft, false) => Motion::Left,
        (NamedKey::ArrowRight, false) => Motion::Right,
        (NamedKey::ArrowUp, false) => Motion::Up,
        (NamedKey::ArrowDown, false) => Motion::Down,
        (NamedKey::ArrowLeft, true) => Motion::LeftWord,
        (NamedKey::ArrowRight, true) => Motion::RightWord,
        (NamedKey::ArrowUp, true) => Motion::ParagraphStart,
        (NamedKey::ArrowDown, true) => Motion::ParagraphEnd,
        _ => unreachable!("is_arrow_key guards this"),
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
    let mut app = Application::new();
    event_loop.run_app(&mut app).unwrap();
}
