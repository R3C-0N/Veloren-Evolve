//! Vue 3D : c'est ici qu'on juge l'illusion, et le prix du cube.
//!
//! Les chunks sont maillés une fois, en coordonnées locales à leur face, puis
//! dessinés à un décalage relatif à la caméra recalculé chaque image. Ce
//! décalage porte toute la topologie : le monde est **déroulé** dans le repère
//! de la face où se trouve la caméra, en prolongeant ce repère au-delà de ses
//! bords. Une face voisine arrive comme un voisin, tournée d'un quart de tour
//! si l'arête l'exige.
//!
//! C'est aussi ici que le défaut des huit coins se voit. Autour d'un coin, le
//! plan déroulé offre 360° là où la surface n'en a que 270° : les 90° en trop
//! retombent sur du terrain déjà placé. Le prototype ne cache pas cette
//! duplication, il la détecte et laisse le menu décider si on la montre.

use crate::chunk::Chunk;
use crate::cube::{COS_SIN, replier_bloc, replier_chunk};
use crate::maillage::{self, Sommet};
use crate::monde::{Bloc, Generateur, HAUTEUR_CHUNK, TAILLE_CHUNK};
use crate::rendu::{FORMAT_COULEUR, FORMAT_PROFONDEUR};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};
use wgpu::util::DeviceExt;

const ALIGNEMENT: u64 = 256;
const CHUNKS_MAX: u64 = 4096;
/// Chunks générés par image : au-delà, le déplacement saccade.
const BUDGET_GENERATION: usize = 6;

pub const CIEL: [f32; 4] = [0.42, 0.60, 0.82, 1.0];

/// Une case canonique du monde : face, puis coordonnées dans cette face.
pub type Cle = (u8, i32, i32);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globaux {
    vue_projection: [[f32; 4]; 4],
    params: [f32; 4],
    ciel: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UnifChunk {
    decalage: [f32; 4],
    rotation: [f32; 4],
    teinte: [f32; 4],
}

struct MailleGpu {
    sommets: wgpu::Buffer,
    indices: wgpu::Buffer,
    nb: u32,
}

pub struct Vue3d {
    pipeline: wgpu::RenderPipeline,
    buf_globaux: wgpu::Buffer,
    bg_globaux: wgpu::BindGroup,
    buf_chunks: wgpu::Buffer,
    bg_chunks: wgpu::BindGroup,
    surligneur: MailleGpu,
    chunks: HashMap<Cle, Option<MailleGpu>>,
    pub chunks_dessines: usize,
    /// Chunks placés deux fois dans la même image : la surface que le défaut
    /// des coins oblige à dupliquer.
    pub doublons: usize,
}

impl Vue3d {
    pub fn nouvelle(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/terrain.wgsl").into()),
        });

        let layout_globaux = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globaux"),
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

        let layout_chunks = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chunks"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<UnifChunk>() as u64
                    ),
                },
                count: None,
            }],
        });

        let buf_globaux = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globaux"),
            size: std::mem::size_of::<Globaux>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let buf_chunks = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunks"),
            size: CHUNKS_MAX * ALIGNEMENT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bg_globaux = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globaux"),
            layout: &layout_globaux,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf_globaux.as_entire_binding(),
            }],
        });

        let bg_chunks = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chunks"),
            layout: &layout_chunks,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buf_chunks,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<UnifChunk>() as u64),
                }),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[&layout_globaux, &layout_chunks],
            push_constant_ranges: &[],
        });

        const ATTRIBUTS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Sommet>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRIBUTS,
                }],
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
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Les recollements du cube sont des rotations, jamais des
                // réflexions : l'orientation des triangles est préservée, donc
                // l'élimination des faces arrière reste valable partout.
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: FORMAT_PROFONDEUR,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            buf_globaux,
            bg_globaux,
            buf_chunks,
            bg_chunks,
            surligneur: cube_surligneur(device),
            chunks: HashMap::new(),
            chunks_dessines: 0,
            doublons: 0,
        }
    }

    pub fn oublier_tout(&mut self) { self.chunks.clear(); }

    pub fn chunks_en_memoire(&self) -> usize { self.chunks.len() }

    #[allow(clippy::too_many_arguments)]
    pub fn dessiner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
        cible: &crate::rendu::Cible,
        gen: &Generateur,
        cam: &Camera,
        reglages: &Reglages,
        vise: Option<[i32; 3]>,
    ) {
        let (largeur, hauteur) = cible.taille;
        let aspect = largeur as f32 / hauteur as f32;
        let portee = (reglages.distance_rendu * TAILLE_CHUNK) as f32 * 1.5;
        let projection = Mat4::perspective_rh(reglages.champ.to_radians(), aspect, 0.1, portee);
        let vue = Mat4::look_to_rh(Vec3::ZERO, cam.avant(), Vec3::Z);

        queue.write_buffer(
            &self.buf_globaux,
            0,
            bytemuck::bytes_of(&Globaux {
                vue_projection: (projection * vue).to_cols_array_2d(),
                params: [
                    reglages.rayon_courbure,
                    portee * 0.45,
                    portee * 0.95,
                    if reglages.teinte_chunks { 1.0 } else { 0.0 },
                ],
                ciel: CIEL,
            }),
        );

        // --- Dérouler le monde autour de la caméra ---------------------------
        let r = reglages.distance_rendu;
        let ccu = (cam.position.x / TAILLE_CHUNK as f32).floor() as i32;
        let ccv = (cam.position.y / TAILLE_CHUNK as f32).floor() as i32;

        // On classe par distance avant d'attribuer : le placement le plus proche
        // de la caméra gagne, les suivants sont les doublons du défaut de coin.
        let mut candidats: Vec<(Cle, f32, [f32; 3], u8)> = Vec::new();
        for dv in -r..=r {
            for du in -r..=r {
                let (vu, vv) = (ccu + du, ccv + dv);
                let (fc, cuc, cvc, k) = replier_chunk(cam.face, vu, vv);

                let ou = (vu * TAILLE_CHUNK) as f32 - cam.position.x;
                let ov = (vv * TAILLE_CHUNK) as f32 - cam.position.y;
                let dist = (ou + 16.0).hypot(ov + 16.0);
                if dist > portee {
                    continue;
                }
                candidats.push(((fc, cuc, cvc), dist, [ou, ov, -cam.position.z], k));
            }
        }
        candidats.sort_by(|a, b| a.1.total_cmp(&b.1));

        let mut visibles: Vec<(Cle, UnifChunk)> = Vec::new();
        let mut manquants: Vec<(Cle, f32)> = Vec::new();
        let mut vus: HashSet<Cle> = HashSet::new();
        let mut doublons = 0;

        for (cle, dist, decalage, k) in candidats {
            let doublon = !vus.insert(cle);
            if doublon {
                doublons += 1;
                if reglages.montrer_defaut {
                    // On laisse le trou : les 90° manquants apparaissent tels
                    // qu'ils sont, plutôt que comblés par une copie.
                    continue;
                }
            }

            if !self.chunks.contains_key(&cle) {
                manquants.push((cle, dist));
                continue;
            }

            // Le maillage est stocké dans le repère canonique de sa face ; le
            // déroulé est celui de la caméra. On tourne donc de −k.
            let (cos, sin) = COS_SIN[((4 - k) % 4) as usize];
            visibles.push((
                cle,
                UnifChunk {
                    decalage: [decalage[0], decalage[1], decalage[2], 0.0],
                    rotation: [cos as f32, sin as f32, 0.0, 0.0],
                    teinte: teinte_chunk(cle, doublon),
                },
            ));
        }
        self.doublons = doublons;

        manquants.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (cle, _) in manquants.into_iter().take(BUDGET_GENERATION) {
            let chunk = Chunk::generer(gen, cle.0, cle.1, cle.2);
            let (sommets, indices) = maillage::mailler(&chunk);
            self.chunks.insert(cle, televerser(device, &sommets, &indices));
        }

        // On garde une marge : revenir sur ses pas ne doit pas tout regénérer.
        if self.chunks.len() > (2 * r as usize + 6).pow(2) {
            self.chunks.retain(|cle, _| vus.contains(cle));
        }

        // --- Le surligneur, calculé à plat puis courbé comme le reste ---------
        if let Some(b) = vise {
            visibles.push((
                (u8::MAX, i32::MIN, i32::MIN),
                UnifChunk {
                    decalage: [
                        b[0] as f32 - cam.position.x,
                        b[1] as f32 - cam.position.y,
                        b[2] as f32 - cam.position.z,
                        0.0,
                    ],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                    teinte: [1.0, 1.0, 1.0, 1.0],
                },
            ));
        }

        let mut octets: Vec<u8> = Vec::with_capacity(visibles.len() * ALIGNEMENT as usize);
        for (_, unif) in &visibles {
            let debut = octets.len();
            octets.extend_from_slice(bytemuck::bytes_of(unif));
            octets.resize(debut + ALIGNEMENT as usize, 0);
        }
        if !octets.is_empty() {
            queue.write_buffer(&self.buf_chunks, 0, &octets);
        }

        // --- Passe de rendu ---------------------------------------------------
        let mut passe = encodeur.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terrain"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &cible.couleur,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: CIEL[0] as f64,
                        g: CIEL[1] as f64,
                        b: CIEL[2] as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &cible.profondeur,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        passe.set_pipeline(&self.pipeline);
        passe.set_bind_group(0, &self.bg_globaux, &[]);

        let mut dessines = 0;
        for (i, (cle, _)) in visibles.iter().enumerate() {
            let maille = if cle.0 == u8::MAX {
                Some(&self.surligneur)
            } else {
                self.chunks.get(cle).and_then(|m| m.as_ref())
            };
            let Some(maille) = maille else { continue };

            passe.set_bind_group(1, &self.bg_chunks, &[i as u32 * ALIGNEMENT as u32]);
            passe.set_vertex_buffer(0, maille.sommets.slice(..));
            passe.set_index_buffer(maille.indices.slice(..), wgpu::IndexFormat::Uint32);
            passe.draw_indexed(0..maille.nb, 0, 0..1);
            dessines += 1;
        }
        self.chunks_dessines = dessines;
    }
}

fn televerser(device: &wgpu::Device, sommets: &[Sommet], indices: &[u32]) -> Option<MailleGpu> {
    if indices.is_empty() {
        return None;
    }
    Some(MailleGpu {
        sommets: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sommets"),
            contents: bytemuck::cast_slice(sommets),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        nb: indices.len() as u32,
    })
}

/// Cube légèrement plus grand qu'un bloc : le surligneur de visée.
fn cube_surligneur(device: &wgpu::Device) -> MailleGpu {
    let mut sommets = Vec::new();
    let mut indices = Vec::new();
    let (a, b) = (-0.03f32, 1.03f32);
    let coins = [
        [a, a, a], [b, a, a], [b, b, a], [a, b, a],
        [a, a, b], [b, a, b], [b, b, b], [a, b, b],
    ];
    // Faces orientées vers l'extérieur : le surligneur passe par le même
    // pipeline que le terrain, élimination des faces arrière comprise.
    let faces = [
        [0usize, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4],
        [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7],
    ];
    for f in faces.iter() {
        let debut = sommets.len() as u32;
        for i in f.iter() {
            sommets.push(Sommet { position: coins[*i], couleur: [1.0, 0.85, 0.1] });
        }
        indices.extend_from_slice(&[debut, debut + 1, debut + 2, debut, debut + 2, debut + 3]);
    }
    televerser(device, &sommets, &indices).expect("cube non vide")
}

fn teinte_chunk(cle: Cle, doublon: bool) -> [f32; 4] {
    // Le terrain dupliqué par le défaut des coins vire au rouge : on voit ce
    // que le déroulement doit inventer.
    if doublon {
        return [1.6, 0.5, 0.5, 1.0];
    }
    let h = (cle.1.wrapping_mul(73_856_093) ^ cle.2.wrapping_mul(19_349_663))
        .wrapping_add(cle.0 as i32 * 83_492_791) as u32;
    [
        0.6 + (h & 0xFF) as f32 / 400.0,
        0.6 + ((h >> 8) & 0xFF) as f32 / 400.0,
        0.6 + ((h >> 16) & 0xFF) as f32 / 400.0,
        1.0,
    ]
}

// --------------------------------------------------------------------------
// Caméra et visée — à plat, toujours
// --------------------------------------------------------------------------

pub struct Camera {
    /// La face où se trouve la caméra. Tout le déroulé se fait dans son repère.
    pub face: u8,
    /// Position en blocs, dans le repère de `face`.
    pub position: Vec3,
    pub lacet: f32,
    pub tangage: f32,
}

impl Camera {
    pub fn avant(&self) -> Vec3 {
        let (sl, cl) = self.lacet.sin_cos();
        let (st, ct) = self.tangage.sin_cos();
        Vec3::new(ct * cl, ct * sl, st)
    }

    pub fn droite(&self) -> Vec3 {
        let (sl, cl) = self.lacet.sin_cos();
        Vec3::new(sl, -cl, 0.0)
    }

    /// Replie la position dans le patron, et rend le nombre de quarts de tour.
    ///
    /// Franchir une arête fait tourner le monde sous les pieds du joueur : le
    /// lacet tourne d'autant, sans quoi la marche repartirait de travers.
    pub fn replier(&mut self) -> u8 {
        let bu = self.position.x.floor() as i32;
        let bv = self.position.y.floor() as i32;
        let (fc, u, v, k) = replier_bloc(self.face, bu, bv);
        if k == 0 && fc == self.face && u == bu && v == bv {
            return 0;
        }

        // La part fractionnaire tourne aussi, autour du centre du bloc.
        let fu = self.position.x - bu as f32 - 0.5;
        let fv = self.position.y - bv as f32 - 0.5;
        let (cos, sin) = COS_SIN[k as usize];
        let (cos, sin) = (cos as f32, sin as f32);

        self.face = fc;
        self.position.x = u as f32 + 0.5 + (fu * cos - fv * sin);
        self.position.y = v as f32 + 0.5 + (fu * sin + fv * cos);
        self.lacet += k as f32 * std::f32::consts::FRAC_PI_2;
        k
    }
}

pub struct Reglages {
    pub rayon_courbure: f32,
    pub distance_rendu: i32,
    pub champ: f32,
    pub teinte_chunks: bool,
    /// Laisser le trou de 90° aux coins au lieu de le combler par une copie.
    pub montrer_defaut: bool,
}

/// Raycast à pas fixe dans le plan déroulé. C'est la règle de D27 : la
/// sélection de bloc ne lit jamais une position courbée, seul son affichage se
/// courbe.
pub fn viser(gen: &Generateur, cam: &Camera, portee: f32) -> Option<[i32; 3]> {
    let dir = cam.avant();
    let mut colonne: Option<((i32, i32), (i32, crate::monde::Biome))> = None;
    let pas = 0.15;
    let mut t = 0.0;
    // Tant qu'on n'a pas vu d'air, on est dans un bloc : viser depuis
    // l'intérieur d'une montagne ou depuis le fond de l'eau ne surligne rien.
    let mut vu_air = false;

    while t < portee {
        t += pas;
        let p = cam.position + dir * t;
        let (bu, bv, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if !(0..HAUTEUR_CHUNK).contains(&bz) {
            continue;
        }
        let cle = (bu, bv);
        if colonne.map(|(c, _)| c) != Some(cle) {
            colonne = Some((cle, gen.colonne(cam.face, bu, bv)));
        }
        let (sol, biome) = colonne.unwrap().1;
        if gen.bloc(sol, biome, bz) == Bloc::Air {
            vu_air = true;
        } else if vu_air {
            return Some([bu, bv, bz]);
        }
    }
    None
}
