//! Vue 2D : c'est ici qu'on juge la topologie.
//!
//! Le monde entier de dessus, à une case pour quatre blocs. La carte est
//! échantillonnée par les mêmes fonctions que le terrain 3D, donc ce qu'on y
//! voit est ce qu'on y marchera. Les coutures sont tracées par-dessus : un
//! relief qui s'y coupe net condamne D27.

use crate::monde::{BLOCS_H, BLOCS_W, Biome, Bloc, Generateur, NIVEAU_MER, biome};
use crate::rendu::FORMAT_COULEUR;
use bytemuck::{Pod, Zeroable};

pub const CARTE_W: u32 = 512;
pub const CARTE_H: u32 = 256;
const PAS: i32 = BLOCS_W / CARTE_W as i32;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Unif {
    cadre: [f32; 4],
    camera: [f32; 4],
}

pub struct Vue2d {
    pipeline: wgpu::RenderPipeline,
    buf: wgpu::Buffer,
    bg_unif: wgpu::BindGroup,
    bg_texture: wgpu::BindGroup,
    texture: wgpu::Texture,
}

impl Vue2d {
    pub fn nouvelle(device: &wgpu::Device, queue: &wgpu::Queue, gen: &Generateur) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("carte"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/carte.wgsl").into()),
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("carte"),
            size: wgpu::Extent3d {
                width: CARTE_W,
                height: CARTE_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let layout_unif = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("carte unif"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let layout_texture = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("carte texture"),
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

        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("carte unif"),
            size: std::mem::size_of::<Unif>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bg_unif = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("carte unif"),
            layout: &layout_unif,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });

        let echantillonneur = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("carte"),
            // Répétition sur X : la carte est un cylindre, même son filtrage.
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let vue = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bg_texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("carte texture"),
            layout: &layout_texture,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&vue),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&echantillonneur),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("carte"),
            bind_group_layouts: &[&layout_unif, &layout_texture],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("carte"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT_COULEUR,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        let vue2d = Self { pipeline, buf, bg_unif, bg_texture, texture };
        vue2d.regenerer(queue, gen);
        vue2d
    }

    /// Recalcule la carte entière. Quelques centaines de milliers
    /// d'échantillons : de l'ordre de la seconde, seulement au changement de
    /// graine.
    pub fn regenerer(&self, queue: &wgpu::Queue, gen: &Generateur) {
        let mut pixels = vec![0u8; (CARTE_W * CARTE_H * 4) as usize];

        for py in 0..CARTE_H {
            for px in 0..CARTE_W {
                let x = px as i32 * PAS;
                let y = py as i32 * (BLOCS_H / CARTE_H as i32);
                let h = gen.hauteur(x, y);

                // Ombrage : la pente est-ouest, prise à travers le repliement.
                // Au bord de la carte, la comparaison porte donc sur le relief
                // d'en face — si une couture coupait, elle noircirait ici.
                let pente = (gen.hauteur(x + PAS, y) - h) * 0.10;

                let couleur = if h <= NIVEAU_MER as f32 {
                    let p = ((NIVEAU_MER as f32 - h) / 24.0).clamp(0.0, 1.0);
                    let c = Bloc::Eau.couleur();
                    [
                        c[0] * (1.0 - p * 0.6) + 0.10 * (1.0 - p),
                        c[1] * (1.0 - p * 0.6) + 0.14 * (1.0 - p),
                        c[2] * (1.0 - p * 0.4) + 0.10 * (1.0 - p),
                    ]
                } else {
                    let base = match biome(y) {
                        Biome::Prairie => Bloc::Herbe.couleur(),
                        Biome::Tempere => {
                            let c = Bloc::Herbe.couleur();
                            [c[0] * 0.8, c[1] * 0.85, c[2] * 0.9]
                        }
                        Biome::Neigeux => Bloc::Neige.couleur(),
                        Biome::Glacier => Bloc::Glace.couleur(),
                    };
                    // L'altitude déteint vers la roche puis la neige.
                    let alt = ((h - NIVEAU_MER as f32) / 46.0).clamp(0.0, 1.0);
                    let roche = Bloc::Roche.couleur();
                    [
                        base[0] * (1.0 - alt) + roche[0] * alt + alt * alt * 0.5,
                        base[1] * (1.0 - alt) + roche[1] * alt + alt * alt * 0.5,
                        base[2] * (1.0 - alt) + roche[2] * alt + alt * alt * 0.5,
                    ]
                };

                let i = ((py * CARTE_W + px) * 4) as usize;
                pixels[i] = encoder_srgb(couleur[0] + pente);
                pixels[i + 1] = encoder_srgb(couleur[1] + pente);
                pixels[i + 2] = encoder_srgb(couleur[2] + pente);
                pixels[i + 3] = 255;
            }
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(CARTE_W * 4),
                rows_per_image: Some(CARTE_H),
            },
            wgpu::Extent3d {
                width: CARTE_W,
                height: CARTE_H,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn dessiner(
        &self,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
        cible: &crate::rendu::Cible,
        camera: [f32; 2],
    ) {
        let (l, h) = cible.taille;
        let aspect_ecran = l as f32 / h as f32;
        let aspect_carte = CARTE_W as f32 / CARTE_H as f32;

        // Boîte aux lettres : la carte garde ses proportions quoi qu'il arrive.
        let (sx, sy) = if aspect_ecran > aspect_carte {
            (aspect_carte / aspect_ecran, 1.0)
        } else {
            (1.0, aspect_ecran / aspect_carte)
        };
        let (sx, sy) = (sx * 0.96, sy * 0.96);

        queue.write_buffer(
            &self.buf,
            0,
            bytemuck::bytes_of(&Unif {
                cadre: [-sx, -sy, sx, sy],
                camera: [camera[0], camera[1], 1.0 / CARTE_H as f32, 0.0],
            }),
        );

        let mut passe = encodeur.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("carte"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &cible.couleur,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.02,
                        g: 0.02,
                        b: 0.03,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        passe.set_pipeline(&self.pipeline);
        passe.set_bind_group(0, &self.bg_unif, &[]);
        passe.set_bind_group(1, &self.bg_texture, &[]);
        passe.draw(0..6, 0..1);
    }
}

fn encoder_srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let e = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0 + 0.5) as u8
}
