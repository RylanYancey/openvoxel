use std::marker::PhantomData;

use bevy::{
    asset::{AssetLoader, RenderAssetUsages, ron},
    image::Image,
    prelude::*,
    render::{
        render_resource::{
            Extent3d, ShaderSize, ShaderType, TextureDimension, TextureFormat,
            encase::private::WriteInto,
        },
        storage::ShaderStorageBuffer,
    },
};
use data::sequence::{RivuletState, Sequence};
use fxhash::FxHashMap;
use image::{RgbaImage, imageops};
use portable_atomic::{AtomicUsize, Ordering};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::sequences::starting::StartupSeq;

#[derive(Asset, TypePath, Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlockTextureMeta {
    /// Whether or not the texture is rotated at random based on position.
    pub random_rotate: bool,

    /// Whether or not the texture is affected by biome tint.
    pub biome_tinted: bool,

    /// Whether or not the texture is affected by water tint.
    pub water_tinted: bool,
}

impl TextureMeta for BlockTextureMeta {
    const LOAD_RIVULET_NAME: &'static str = "load-block-textures";
    const BUILD_RIVULET_NAME: &'static str = "build-block-texture-array";

    type GpuRepr = GpuBlockTextureMeta;
    fn as_gpu(tex: &Texture<Self>) -> Self::GpuRepr {
        GpuBlockTextureMeta {
            index: tex.idx as u32,
            flags: 0u32,
        }
    }
}

#[derive(ShaderType, Clone, Default)]
pub struct GpuBlockTextureMeta {
    pub index: u32,
    pub flags: u32,
}

#[derive(Default)]
pub struct TextureArrayPlugin<M: TextureMeta> {
    sources: TextureArraySources<M>,
}

impl<M: TextureMeta> TextureArrayPlugin<M> {
    pub fn with_folder(mut self, path: impl Into<String>) -> Self {
        self.sources.add_folder(path.into());
        self
    }

    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        self.sources.add_file(path.into());
        self
    }
}

impl<M: TextureMeta> Plugin for TextureArrayPlugin<M> {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .init_asset::<Texture<M>>()
            .init_asset_loader::<TextureLoader<M>>()
            .insert_resource(self.sources.clone())
            .add_systems(Update, (
                load_in_startup::<M>
                    .run_if(in_state(StartupSeq::LoadTextures)),
                build_in_startup::<M>
                    .run_if(in_state(StartupSeq::BuildTextureArrays)),
            ))
        ;
    }
}

fn load_in_startup<M: TextureMeta>(
    seq: Res<Sequence<StartupSeq>>,
    sources: Res<TextureArraySources<M>>,
    mut textures: ResMut<Assets<Texture<M>>>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let mut rivulet = seq.get(M::LOAD_RIVULET_NAME);
    match rivulet.state {
        RivuletState::Uninit => {
            rivulet.state = RivuletState::Finished;

            let mut handles: Vec<Handle<Texture<M>>> = vec![textures.add(Texture {
                idx: 0,
                data: get_debug_texture(),
                meta: M::default(),
            })];

            for source in sources.iter() {
                match source {
                    TextureSource::File(path) => {
                        handles.push(assets.load(path));
                    }
                    TextureSource::Folder(_path) => {
                        unimplemented!()
                    }
                }
            }

            commands.insert_resource(TextureArrayBuilder::new(handles));
        }
        _ => {}
    }
}

fn build_in_startup<M: TextureMeta>(
    seq: Res<Sequence<StartupSeq>>,
    mut builder: ResMut<TextureArrayBuilder<M>>,
    mut commands: Commands,
    server: Res<AssetServer>,
    textures: Res<Assets<Texture<M>>>,
    mut images: ResMut<Assets<Image>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
) {
    let mut rivulet = seq.get(M::BUILD_RIVULET_NAME);
    match rivulet.state {
        RivuletState::Uninit => {
            rivulet.progress =
                builder.total as f32 / builder.count_ready(&server, &textures) as f32;
            if builder.is_ready() {
                rivulet.state = RivuletState::InProgress;
            }
        }
        RivuletState::InProgress => {
            if builder.has_remaining() {
                builder.process(1, &textures);
            } else {
                rivulet.state = RivuletState::Finished;
                let builder =
                    std::mem::replace(&mut *builder, TextureArrayBuilder::new(Vec::new()));
                let array = builder.finish(&mut images, &mut buffers);
                commands.insert_resource(array);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
pub enum TextureSource {
    Folder(String),
    File(String),
}

#[derive(Resource, Deref, Default, Clone)]
pub struct TextureArraySources<M: TextureMeta> {
    #[deref]
    pub sources: Vec<TextureSource>,
    _marker: PhantomData<M>,
}

impl<M: TextureMeta> TextureArraySources<M> {
    pub fn add_folder(&mut self, path: impl Into<String>) {
        self.sources.push(TextureSource::Folder(path.into()));
    }

    pub fn add_file(&mut self, path: impl Into<String>) {
        self.sources.push(TextureSource::File(path.into()))
    }
}

#[derive(Asset, Debug, TypePath)]
pub struct Texture<M: TextureMeta> {
    pub meta: M,
    pub data: RgbaImage,
    pub idx: usize,
}

pub trait TextureMeta:
    Send + Sync + TypePath + Serialize + DeserializeOwned + Default + Clone
{
    /// Name of Rivulet in startup sequence while loading.
    const LOAD_RIVULET_NAME: &'static str;

    /// Name of rivulet in startup sequence while buliding.
    const BUILD_RIVULET_NAME: &'static str;

    /// Gpu-compatible type that will be load onto the gpu as a Vec<GpuRepr>.
    type GpuRepr: Default + ShaderType + ShaderSize + WriteInto + Send + Sync + 'static;

    /// Construct an instance of Self::GpuRepr from a meta and its containing Texture.
    fn as_gpu(tex: &Texture<Self>) -> Self::GpuRepr;
}

#[derive(TypePath, Resource)]
pub struct TextureArray<M: TextureMeta> {
    /// Get the array index by name.
    resolver: FxHashMap<String, usize>,

    /// The images packed into a single column texture.
    images: Handle<Image>,

    /// The GPU-side descriptor array for textures.
    gpu_data: Handle<ShaderStorageBuffer>,
    _marker: PhantomData<M>,
}

impl<M: TextureMeta> TextureArray<M> {
    pub fn resolve(&self, name: impl AsRef<str>) -> Option<usize> {
        self.resolver.get(name.as_ref()).copied()
    }

    pub fn image(&self) -> Handle<Image> {
        self.images.clone()
    }

    pub fn table(&self) -> Handle<ShaderStorageBuffer> {
        self.gpu_data.clone()
    }
}

#[derive(Resource)]
pub struct TextureArrayBuilder<M: TextureMeta> {
    /// Textures that are yet to be added to the array.
    remaining: Vec<Handle<Texture<M>>>,

    /// Map of array element names to indices.
    resolver: FxHashMap<String, usize>,

    /// Whether all remaining textures are loaded and ready for processing.
    is_all_loaded: bool,

    /// Width/Height of the individual tiles.
    tile_size: u32,

    /// Max tile index of a texture in the array.
    max_index: usize,

    /// The total number of textures at the start.
    total: usize,

    /// The output image.
    image: RgbaImage,

    /// The output ShaderStorageBuffer
    gpu_data: Vec<M::GpuRepr>,
}

impl<M: TextureMeta> TextureArrayBuilder<M> {
    pub fn new(handles: Vec<Handle<Texture<M>>>) -> Self {
        Self {
            total: handles.len(),
            remaining: handles,
            is_all_loaded: false,
            resolver: FxHashMap::default(),
            tile_size: 0,
            max_index: 0,
            image: RgbaImage::new(0, 0),
            gpu_data: Vec::new(),
        }
    }

    pub fn count_ready(&mut self, server: &AssetServer, assets: &Assets<Texture<M>>) -> usize {
        let mut sum = 0;
        let mut failed = Vec::new();
        for (i, handle) in self.remaining.iter().enumerate() {
            if i == 0 {
                // index 0 is always the debug texture, and is therefore always loaded.
                sum += 1;
            } else {
                use bevy::asset::RecursiveDependencyLoadState::*;
                match server.recursive_dependency_load_state(handle) {
                    Loaded => sum += 1,
                    Failed(e) => {
                        failed.push(i);
                        error!(
                            "Array texture of type '{}' failed to load with error: '{e}'",
                            Texture::<M>::type_path()
                        );
                    }
                    _ => {}
                }
            }
        }

        while let Some(i) = failed.pop() {
            self.remaining.swap_remove(i);
            self.total -= 1;
        }

        if sum != self.remaining.len() {
            return sum;
        }

        for handle in &self.remaining {
            if let Some(tex) = assets.get(handle) {
                if let Some(path) = server.get_path(handle) {
                    self.resolver.insert(path.to_string(), tex.idx);
                }

                self.max_index = usize::max(self.max_index, tex.idx);
                self.tile_size = u32::max(
                    self.tile_size,
                    u32::max(tex.data.width(), tex.data.height()),
                );
            }
        }

        self.image = RgbaImage::new(self.tile_size, self.tile_size * (self.max_index as u32 + 1));
        self.gpu_data
            .resize_with(self.max_index + 1, || M::GpuRepr::default());

        self.is_all_loaded = true;
        sum
    }

    pub fn is_ready(&self) -> bool {
        self.is_all_loaded
    }

    pub fn has_remaining(&self) -> bool {
        !self.remaining.is_empty()
    }

    fn put(&mut self, tex: &Texture<M>) {
        // create gpu descriptor for the texture.
        self.gpu_data[tex.idx] = M::as_gpu(&tex);

        // resize to fit dimensions if needed.
        let resized = (tex.data.dimensions() != (self.tile_size, self.tile_size)).then(|| {
            imageops::resize(
                &tex.data,
                self.tile_size,
                self.tile_size,
                imageops::FilterType::Nearest,
            )
        });

        // write data to texture.
        let y = self.tile_size * tex.idx as u32;
        let img = resized.as_ref().unwrap_or(&tex.data);
        imageops::overlay(&mut self.image, img, 0, y as i64);
    }

    pub fn process(&mut self, limit: usize, assets: &Assets<Texture<M>>) {
        let mut i = 0;
        while let Some(handle) = self.remaining.pop() {
            match assets.get(&handle) {
                Some(tex) => self.put(tex),
                None => warn!(
                    "A handle added to texture array: '{}' resolved to `None`.",
                    TextureArray::<M>::type_path()
                ),
            }

            i += 1;
            if i > limit {
                return;
            }
        }
    }

    pub fn finish(
        self,
        images: &mut Assets<Image>,
        buffers: &mut Assets<ShaderStorageBuffer>,
    ) -> TextureArray<M> {
        assert!(
            self.total > 1,
            "[R777] All Texture Arrays must have at least 2 images, found: {}",
            self.total
        );
        if self.total != 0 {
            debug_assert_ne!(self.image.width(), 0);
            debug_assert_ne!(self.image.height(), 0);

            // convert to bevy image array
            let image = convert_rgba_image_to_bevy_texture_array(self.image, self.total as u32);
            let handle = images.add(image);
            TextureArray {
                resolver: self.resolver,
                gpu_data: buffers.add(ShaderStorageBuffer::from(self.gpu_data)),
                images: handle,
                _marker: PhantomData,
            }
        } else {
            warn!("Texture Array had zero total images.");
            TextureArray {
                resolver: FxHashMap::default(),
                gpu_data: Handle::default(),
                images: Handle::default(),
                _marker: PhantomData,
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TextureError {
    #[error("IO Error while reading texture: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to decode texture: {0}")]
    Image(#[from] image::ImageError),
    #[error("Failed to deserialize texture meta: {0}")]
    Ron(#[from] ron::de::SpannedError),
}

pub struct TextureLoader<M: TextureMeta> {
    next_id: AtomicUsize,
    _marker: PhantomData<M>,
}

impl<M: TextureMeta> Default for TextureLoader<M> {
    fn default() -> Self {
        Self {
            // 0 is always the debug texture
            next_id: AtomicUsize::new(1),
            _marker: PhantomData,
        }
    }
}

impl<M: TextureMeta> AssetLoader for TextureLoader<M> {
    type Asset = Texture<M>;
    type Error = TextureError;
    type Settings = M;

    fn extensions(&self) -> &[&str] {
        &["png"]
    }

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        settings: &Self::Settings,
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        let img = image::load_from_memory_with_format(&buf, image::ImageFormat::Png)?.to_rgba8();
        Ok(Texture {
            meta: settings.clone(),
            data: img,
            idx: self.next_id.fetch_add(1, Ordering::Relaxed),
        })
    }
}

fn convert_rgba_image_to_bevy_texture_array(img: RgbaImage, layers: u32) -> Image {
    let (width, height) = img.dimensions();
    let raw = img.into_raw();

    Image::new(
        Extent3d {
            width,
            height: (height / layers),
            depth_or_array_layers: layers,
        },
        TextureDimension::D2,
        raw,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    )
}

fn get_debug_texture() -> RgbaImage {
    const DEBUG_TEXTURE_DATA: [[u8; 4]; 256] = {
        let mut pixels = [[0u8; 4]; 256];
        let mut i = 0;
        while i < 256 {
            let x = i % 16;
            let y = i / 16;
            let is_magenta = ((x / 8) + (y / 8)) % 2 == 0;

            pixels[i] = if is_magenta {
                [255, 0, 255, 255] // Magenta RGBA
            } else {
                [0, 0, 0, 255] // Black RGBA
            };

            i += 1;
        }
        pixels
    };

    RgbaImage::from_raw(
        16,
        16,
        DEBUG_TEXTURE_DATA
            .iter()
            .flat_map(|pixel| pixel.iter().copied())
            .collect::<Vec<u8>>(),
    )
    .unwrap()
}
