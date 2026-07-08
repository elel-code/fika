impl IconRenderer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fika-wgpu-icon-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fika-wgpu-icon-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture = create_icon_texture(device, 1, 1);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_icon_bind_group(device, &bind_group_layout, &texture_view, &sampler);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-icon-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXTURE_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-icon-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-icon-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(TextVertex::layout())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let vertex_capacity = 6;
        let vertex_buffer = create_text_vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            texture,
            texture_view,
            bind_group,
            texture_width: 1,
            texture_height: 1,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
            overlay_vertex_start: 0,
            overlay_vertex_count: 0,
            last_vertices_hash: None,
            last_icon_upload_keys: HashSet::new(),
            resolver: FileIconResolver::new(),
            thumbnails: ThumbnailRasterResolver::new(),
            icon_rasters: IconRasterResolver::new(),
            raster_cache: IconRasterCache::new(ICON_CACHE_MAX_BYTES),
            role_raster_cache: IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES),
        }
    }

    fn prewarm_common_file_icon_rasters(&mut self, icon_size: f32) -> usize {
        let size_px = icon_cache_size(icon_size);
        let roles = [
            FileIconRoleCacheKey {
                kind: FileIconKind::Directory,
            },
            FileIconRoleCacheKey {
                kind: FileIconKind::File { extension: None },
            },
        ];
        let mut rasterized = 0usize;
        for role in roles {
            let path_key = FileIconPathCacheKey {
                role: role.clone(),
                size_px,
            };
            let snapshot = self.resolver.resolve_path_cache_key_fast(path_key);
            let Some(path) = snapshot.path else {
                continue;
            };
            let key = IconRasterCacheKey::icon(path, size_px);
            if let Some(raster) = self.raster_cache.get(&key) {
                self.role_raster_cache.insert(role, raster);
                continue;
            }
            let Some(raster) = rasterize_icon(&key.path, size_px as u32) else {
                continue;
            };
            let raster = self.raster_cache.insert(key, raster);
            self.role_raster_cache.insert(role, raster);
            rasterized += 1;
        }
        rasterized
    }

    fn prewarm_small_directory_file_icon_rasters(
        &mut self,
        projections: &[ShellPaneProjection<'_>],
    ) -> IconRasterPrewarmStats {
        self.icon_rasters.drain_results(&mut self.raster_cache);
        let deadline = Instant::now() + DOLPHIN_MAX_BLOCK_TIMEOUT;
        let mut stats = IconRasterPrewarmStats::default();
        let mut seen = HashSet::new();
        for projection in projections {
            if projection.view.filtered_entry_count() > DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT {
                continue;
            }
            let Some(icon_size) = projection.visible_items.first().map(|item| {
                item.layout
                    .icon_rect
                    .width
                    .max(item.layout.icon_rect.height)
                    .clamp(16.0, 256.0)
            }) else {
                continue;
            };
            let size_px = icon_cache_size(icon_size);
            for entry_index in projection.view.filtered_indexes.iter().copied() {
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    return stats;
                }
                let Some(entry) = projection.view.entries.get(entry_index) else {
                    continue;
                };
                let path = projection.view.path.join(entry.name.as_ref());
                let path_key = file_icon_path_cache_key(
                    &path,
                    entry.is_dir,
                    entry.mime_type.clone(),
                    entry.mime_magic_checked,
                    icon_size,
                );
                let role_key = path_key.role.clone();
                let Some(snapshot) = self.resolver.resolve_path_cache_key(path_key) else {
                    continue;
                };
                let Some(icon_path) = snapshot.path else {
                    stats.failed += 1;
                    continue;
                };
                let raster_key = IconRasterCacheKey::icon(icon_path, size_px);
                if !seen.insert(raster_key.clone()) {
                    continue;
                }
                stats.entries += 1;
                if let Some(raster) = self.raster_cache.get(&raster_key) {
                    stats.cache_hits += 1;
                    self.role_raster_cache.insert(role_key, raster);
                    continue;
                }
                stats.cache_misses += 1;
                let raster_start = Instant::now();
                let Some(raster) = rasterize_icon(&raster_key.path, size_px as u32) else {
                    stats.raster_us += raster_start.elapsed().as_micros();
                    stats.failed += 1;
                    continue;
                };
                stats.raster_us += raster_start.elapsed().as_micros();
                let raster = self.raster_cache.insert(raster_key, raster);
                self.role_raster_cache.insert(role_key, raster);
            }
        }
        stats
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &mut IconFrame,
    ) -> VertexBufferUploadStats {
        if frame.width != self.texture_width || frame.height != self.texture_height {
            self.texture = create_icon_texture(device, frame.width, frame.height);
            self.texture_view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.bind_group = create_icon_bind_group(
                device,
                &self.bind_group_layout,
                &self.texture_view,
                &self.sampler,
            );
            self.texture_width = frame.width;
            self.texture_height = frame.height;
            self.last_vertices_hash = None;
            self.last_icon_upload_keys.clear();
        }

        let mut current_upload_keys = HashSet::with_capacity(frame.uploads.len());
        let mut atlas_uploads = 0usize;
        let mut atlas_upload_skips = 0usize;
        for upload in &frame.uploads {
            let key = IconAtlasUploadKey::from_upload(upload);
            let skip_upload = self.last_icon_upload_keys.contains(&key);
            current_upload_keys.insert(key);
            if skip_upload {
                atlas_upload_skips += 1;
                continue;
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: upload.atlas.x as u32,
                        y: upload.atlas.y as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                upload.raster.pixels.as_ref(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(upload.raster.width * 4),
                    rows_per_image: Some(upload.raster.height),
                },
                wgpu::Extent3d {
                    width: upload.raster.width,
                    height: upload.raster.height,
                    depth_or_array_layers: 1,
                },
            );
            atlas_uploads += 1;
        }
        self.last_icon_upload_keys = current_upload_keys;
        frame.stats.atlas_uploads = atlas_uploads;
        frame.stats.atlas_upload_skips = atlas_upload_skips;

        let total_vertices = frame.vertices.len() + frame.overlay_vertices.len();
        if total_vertices > self.vertex_capacity {
            self.vertex_capacity = total_vertices.next_power_of_two();
            self.vertex_buffer = create_text_vertex_buffer(device, self.vertex_capacity);
            self.last_vertices_hash = None;
        }
        self.vertex_count = frame.vertices.len();
        self.overlay_vertex_start = frame.vertices.len();
        self.overlay_vertex_count = frame.overlay_vertices.len();
        let Some(hash) = vertex_pair_hash(&frame.vertices, &frame.overlay_vertices) else {
            self.last_vertices_hash = None;
            return VertexBufferUploadStats::default();
        };
        if self.last_vertices_hash == Some(hash) {
            return VertexBufferUploadStats {
                writes: 0,
                skips: 1,
            };
        }
        let mut vertices = Vec::with_capacity(total_vertices);
        vertices.extend_from_slice(&frame.vertices);
        vertices.extend_from_slice(&frame.overlay_vertices);
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.last_vertices_hash = Some(hash);
        VertexBufferUploadStats {
            writes: 1,
            skips: 0,
        }
    }

    fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }

    fn draw_overlay<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.overlay_vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        let start = self.overlay_vertex_start as u32;
        let end = start + self.overlay_vertex_count as u32;
        pass.draw(start..end, 0..1);
    }

    fn batch_count(&self) -> usize {
        usize::from(self.vertex_count > 0) + usize::from(self.overlay_vertex_count > 0)
    }
}
#[derive(Clone, Copy, Debug, Default)]
struct TextFrameStats {
    labels: usize,
    quads: usize,
    deferred: usize,
    atlas_reused: usize,
    atlas_uploads: usize,
    atlas_upload_skips: usize,
    atlas_width: u32,
    atlas_height: u32,
    atlas_bytes: usize,
    cache_hits: usize,
    cache_misses: usize,
    cache_entries: usize,
    cache_bytes: usize,
    swash_image_entries: usize,
    swash_outline_entries: usize,
    swash_resets: usize,
    raster_us: u128,
}
impl TextFrameStats {
    fn merged(self, other: Self) -> Self {
        Self {
            labels: self.labels + other.labels,
            quads: self.quads + other.quads,
            deferred: self.deferred + other.deferred,
            atlas_reused: self.atlas_reused + other.atlas_reused,
            atlas_uploads: self.atlas_uploads + other.atlas_uploads,
            atlas_upload_skips: self.atlas_upload_skips + other.atlas_upload_skips,
            atlas_width: self.atlas_width.max(other.atlas_width),
            atlas_height: self.atlas_height.max(other.atlas_height),
            atlas_bytes: self.atlas_bytes + other.atlas_bytes,
            cache_hits: self.cache_hits + other.cache_hits,
            cache_misses: self.cache_misses + other.cache_misses,
            cache_entries: self.cache_entries + other.cache_entries,
            cache_bytes: self.cache_bytes + other.cache_bytes,
            swash_image_entries: self.swash_image_entries.max(other.swash_image_entries),
            swash_outline_entries: self.swash_outline_entries.max(other.swash_outline_entries),
            swash_resets: self.swash_resets + other.swash_resets,
            raster_us: self.raster_us + other.raster_us,
        }
    }
}
struct TextFrame {
    vertices: Vec<TextVertex>,
    pixels: Vec<u8>,
    uploads: Vec<TextAtlasUpload>,
    width: u32,
    height: u32,
    stats: TextFrameStats,
}
const TEXT_ATLAS_GUARD_TEXELS: u32 = 1;
#[derive(Clone, Debug)]
struct PendingTextDraw {
    key: LabelCacheKey,
    pixels: Arc<[u8]>,
    atlas_upload_required: bool,
    screen: ViewRect,
    rect: ViewRect,
    label_width: u32,
    label_height: u32,
    color: TextColor,
}
#[derive(Clone, Debug)]
struct TextAtlasUpload {
    atlas: AtlasRect,
    pixels: Arc<[u8]>,
    width: u32,
    height: u32,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextAtlasUploadKey {
    atlas_x: u32,
    atlas_y: u32,
    atlas_width: u32,
    atlas_height: u32,
    upload_width: u32,
    upload_height: u32,
    pixels_hash: u64,
}
impl TextAtlasUploadKey {
    fn from_upload(upload: &TextAtlasUpload) -> Self {
        Self {
            atlas_x: upload.atlas.x as u32,
            atlas_y: upload.atlas.y as u32,
            atlas_width: upload.atlas.width as u32,
            atlas_height: upload.atlas.height as u32,
            upload_width: upload.width,
            upload_height: upload.height,
            pixels_hash: hash_bytes_with_len(upload.pixels.as_ref()),
        }
    }
}
fn text_atlas_max_label_width(atlas_width: u32) -> u32 {
    atlas_width
        .saturating_sub(TEXT_PADDING * 2 + TEXT_ATLAS_GUARD_TEXELS * 2)
        .max(1)
}
fn text_atlas_guarded_extent(extent: u32) -> u32 {
    extent + TEXT_ATLAS_GUARD_TEXELS * 2
}
fn padded_text_atlas_pixels(pixels: Arc<[u8]>, width: u32, height: u32) -> (Arc<[u8]>, u32, u32) {
    if TEXT_ATLAS_GUARD_TEXELS == 0 || width == 0 || height == 0 {
        return (pixels, width, height);
    }

    let guard = TEXT_ATLAS_GUARD_TEXELS;
    let padded_width = text_atlas_guarded_extent(width);
    let padded_height = text_atlas_guarded_extent(height);
    let mut padded = vec![0; (padded_width * padded_height) as usize];
    for y in 0..padded_height {
        let src_y = y.saturating_sub(guard).min(height.saturating_sub(1));
        for x in 0..padded_width {
            let src_x = x.saturating_sub(guard).min(width.saturating_sub(1));
            padded[(y * padded_width + x) as usize] = pixels[(src_y * width + src_x) as usize];
        }
    }

    (padded.into(), padded_width, padded_height)
}
