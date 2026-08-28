//! Vue 3D : c'est ici qu'on juge l'illusion — sauf qu'elle n'en est plus une.
//!
//! Chaque chunk est dessiné **une seule fois, à sa vraie place sur la
//! planète** : ses coordonnées de face passent par la projection cube → sphère,
//! la même que `cube::direction`. Il n'y a plus de plan déroulé, donc plus
//! aucune duplication et plus aucune fausse adjacence — ce qui était le prix du
//! déroulement à plat, et ce qui coupait les montagnes près des coins.
//!
//! Ce que la logique garde de plat, elle le garde entièrement : la caméra vit
//! dans le repère de sa face, la visée s'y calcule, et le déplacement aussi.
//! Seul le rendu connaît la sphère. C'est exactement la règle de D27, appliquée
//! plus strictement qu'avant.

use crate::chunk::Chunk;
use crate::cube::{
    BASES, FACE, RAYON, depuis_direction, direction, direction_continue,
    replier_bloc, replier_chunk, replier_continu,
};
use crate::maillage::{self, Sommet};
use crate::monde::{Bloc, Generateur, HAUTEUR_CHUNK, TAILLE_CHUNK, TAILLE_CHUNK as TC};
use crate::poche::{self, CameraPlate, Poche};
use crate::rendu::{FORMAT_COULEUR, FORMAT_PROFONDEUR};
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Vec2, Vec3};
use std::collections::{HashMap, HashSet, VecDeque};
use wgpu::util::DeviceExt;

const ALIGNEMENT: u64 = 256;
const CHUNKS_MAX: u64 = 8192;

/// Où commencent les uniformes de chunk de l'aperçu, en emplacements.
///
/// Les deux passes d'une même image écrivent dans les mêmes tampons, et
/// `queue.write_buffer` n'est pas enregistré dans l'encodeur : il s'applique
/// **avant** que les commandes ne s'exécutent. Deux écritures au même endroit,
/// et c'est la dernière qui gagne — les deux passes lisent alors les mêmes
/// données, ce qui donne un aperçu peint avec la matrice du présent, donc vide.
///
/// D'où deux régions disjointes plutôt qu'un tampon partagé : le tampon est
/// coupé en deux moitiés égales. La poche n'a que trente-six chunks, mais
/// l'aperçu du **présent** en a autant que la vue ordinaire — c'est le prix
/// annoncé d'une fenêtre : un parcours de monde complet de plus par image.
const BASE_APERCU: u32 = (CHUNKS_MAX / 2) as u32;

pub const CIEL: [f32; 4] = [0.42, 0.60, 0.82, 1.0];

/// Une case canonique du monde : face, puis coordonnées dans cette face.
pub type Cle = (u8, i32, i32);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globaux {
    vue_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    params: [f32; 4],
    planete: [f32; 4],
    ciel: [f32; 4],
    /// `(normale, distance)` : on garde ce qui vérifie `dot(n, p) + d >= 0`.
    /// Tout à zéro = on ne coupe rien.
    coupe: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UnifChunk {
    base_r: [f32; 4],
    base_h: [f32; 4],
    base_n: [f32; 4],
    origine: [f32; 4],
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
    /// Le second jeu de globaux, à son propre décalage : l'aperçu du passé se
    /// peint dans la même image que le présent, et ne peut donc pas partager
    /// ses uniformes.
    bg_globaux_apercu: wgpu::BindGroup,
    buf_chunks: wgpu::Buffer,
    bg_chunks: wgpu::BindGroup,
    surligneur: MailleGpu,
    chunks: HashMap<Cle, Option<MailleGpu>>,
    /// Le second cache, pour le second monde.
    ///
    /// La clé porte la graine de l'instance et n'a **pas le même type** que
    /// [`Cle`] : une collision entre les deux mondes n'est pas improbable, elle
    /// est inécrivable. Et la graine dedans fait qu'une fenêtre rouverte ne
    /// réutilise jamais les mailles de la précédente — D9.
    chunks_poche: HashMap<(u32, i32, i32), Option<MailleGpu>>,
    /// Le cadre du portail, et la case où il se tient. Le maillage est refait à
    /// chaque ouverture parce que son orientation vient du monde, pas de la
    /// grille : deux portails posés au même cap n'ont pas les mêmes sommets si
    /// leurs cases sont différentes.
    /// Le cadre du portail, sa nappe, et la case où il se tient.
    ///
    /// Deux mailles et non une : le cadre est de la pierre ordinaire et passe
    /// par le pipeline du terrain ; la nappe est la fenêtre, et va chercher son
    /// image dans les coulisses.
    portail: Option<(MailleGpu, MailleGpu, u8, i32, i32, i32)>,
    /// Le portail de sortie, côté poche : cadre et nappe. Il est fixe — c'est
    /// la poche qui décide où il se tient — donc il est bâti une fois.
    sortie: (MailleGpu, MailleGpu),
    /// Le pipeline de la nappe, et de quoi lui lier les coulisses.
    pipeline_nappe: wgpu::RenderPipeline,
    layout_coulisses: wgpu::BindGroupLayout,
    /// Là où le passé est peint avant d'être découpé dans la nappe.
    coulisses: crate::rendu::Coulisse,
    /// Le passé a-t-il été peint pour cette image ? Sinon la nappe n'a rien à
    /// montrer et on ne la dessine pas.
    apercu_pret: bool,
    pub chunks_dessines: usize,
}

impl Vue3d {
    pub fn nouvelle(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/terrain.wgsl").into()),
        });

        let layout_globaux = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globaux"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
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

        // Deux emplacements : le présent et l'aperçu du passé, dans la même
        // image. Voir [`BASE_APERCU`].
        let buf_globaux = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globaux"),
            size: 2 * ALIGNEMENT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let buf_chunks = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunks"),
            size: CHUNKS_MAX * ALIGNEMENT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vue_conforme = texture_conforme(device, queue);
        let groupe_globaux = |decalage: u64| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globaux"),
                layout: &layout_globaux,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buf_globaux,
                            offset: decalage,
                            size: wgpu::BufferSize::new(
                                std::mem::size_of::<Globaux>() as u64
                            ),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&vue_conforme),
                    },
                ],
            })
        };
        let bg_globaux = groupe_globaux(0);
        let bg_globaux_apercu = groupe_globaux(ALIGNEMENT);

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

        // La nappe : même sommet, même géométrie, mais un fragment qui va
        // chercher son pixel dans les coulisses. Elle a donc un groupe de plus.
        let layout_coulisses = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("coulisses"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let module_nappe = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("portail"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/portail.wgsl").into()),
        });

        let layout_nappe = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("portail"),
            bind_group_layouts: &[&layout_globaux, &layout_chunks, &layout_coulisses],
            push_constant_ranges: &[],
        });

        let pipeline_nappe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("portail"),
            layout: Some(&layout_nappe),
            vertex: wgpu::VertexState {
                module: &module_nappe,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Sommet>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRIBUTS,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module_nappe,
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
                // La nappe se regarde des deux côtés : on la voit encore une
                // fraction de seconde en la franchissant.
                cull_mode: None,
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
            bg_globaux_apercu,
            buf_chunks,
            bg_chunks,
            surligneur: cube_surligneur(device),
            chunks: HashMap::new(),
            chunks_poche: HashMap::new(),
            portail: None,
            sortie: cadre_portail(device, Vec2::X, Vec2::Y),
            pipeline_nappe,
            layout_coulisses,
            coulisses: crate::rendu::Coulisse::nouvelle(device, (1280, 720)),
            apercu_pret: false,
            chunks_dessines: 0,
        }
    }

    pub fn oublier_tout(&mut self) { self.chunks.clear(); }

    /// La fenêtre s'est refermée : ce qui était dans la poche n'est plus (D9).
    pub fn oublier_poche(&mut self) { self.chunks_poche.clear(); }

    pub fn chunks_en_memoire(&self) -> usize { self.chunks.len() + self.chunks_poche.len() }

    /// Pose — ou retire — le cadre du portail.
    pub fn poser_portail(
        &mut self,
        device: &wgpu::Device,
        portail: Option<&crate::ancre::Portail>,
    ) {
        self.portail = portail.map(|p| {
            let (cadre, nappe) = cadre_portail(device, p.axe_droite, p.axe_avant);
            (cadre, nappe, p.face, p.u, p.v, p.z)
        });
    }

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
        vise: Option<Vise>,
    ) {
        self.passe_sphere(
            device,
            queue,
            encodeur,
            (&cible.couleur, &cible.profondeur, cible.taille),
            gen,
            cam,
            reglages,
            vise,
            [0.0; 4],
            (0, 0),
        );
    }

    /// Peint le présent dans les coulisses, pour la nappe du portail de sortie.
    ///
    /// Le pendant exact de [`Vue3d::dessiner_apercu`], dans l'autre sens. La
    /// caméra vient de la poche, transportée par le portail ; le plan de coupe
    /// retire le relief qui se trouve derrière elle.
    #[allow(clippy::too_many_arguments)]
    pub fn dessiner_apercu_sphere(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
        taille: (u32, u32),
        gen: &Generateur,
        cam: &Camera,
        reglages: &Reglages,
        coupe: [f32; 4],
    ) {
        self.coulisses.ajuster(device, taille);
        let (couleur, profondeur) = (
            self.coulisses.couleur.clone(),
            self.coulisses.profondeur.clone(),
        );
        self.passe_sphere(
            device,
            queue,
            encodeur,
            (&couleur, &profondeur, self.coulisses.taille),
            gen,
            cam,
            reglages,
            None,
            coupe,
            (ALIGNEMENT, BASE_APERCU),
        );
        self.apercu_pret = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn passe_sphere(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
        sortie: (&wgpu::TextureView, &wgpu::TextureView, (u32, u32)),
        gen: &Generateur,
        cam: &Camera,
        reglages: &Reglages,
        vise: Option<Vise>,
        coupe: [f32; 4],
        region: (u64, u32),
    ) {
        let (vue_couleur, vue_profondeur, (largeur, hauteur)) = sortie;
        let (decalage_globaux, base_slot) = region;
        let apercu = decalage_globaux != 0;
        let aspect = largeur as f32 / hauteur as f32;
        let rayon = RAYON * reglages.aplatissement as f64;
        let portee = (reglages.distance_rendu * TAILLE_CHUNK) as f32 * 1.4;

        let (position, avant, haut) = cam.repere_3d(rayon);
        let projection = Mat4::perspective_rh(reglages.champ.to_radians(), aspect, 0.2, portee);
        let vue = Mat4::look_to_rh(Vec3::ZERO, avant, haut);

        queue.write_buffer(
            &self.buf_globaux,
            decalage_globaux,
            bytemuck::bytes_of(&Globaux {
                vue_projection: (projection * vue).to_cols_array_2d(),
                camera: [position.x as f32, position.y as f32, position.z as f32, 0.0],
                params: [
                    portee * 0.45,
                    portee * 0.98,
                    reglages.teinte_chunks,
                    0.0,
                ],
                planete: [FACE as f32, rayon as f32, crate::conforme::N as f32, 0.0],
                ciel: CIEL,
                coupe,
            }),
        );

        // --- Quels chunks ----------------------------------------------------
        //
        // Par proche en proche, en marchant sur la surface. Balayer un
        // rectangle dans le repère de la caméra ne suffit pas : près d'un
        // coin, ce rectangle recouvre deux fois certaines régions et **en
        // manque d'autres** — le trou se voyait à l'écran. Un parcours en
        // largeur ne peut ni sauter un voisin ni en visiter deux fois.
        let r = reglages.distance_rendu;
        let ccu = (cam.position.x / TAILLE_CHUNK as f32).floor() as i32;
        let ccv = (cam.position.y / TAILLE_CHUNK as f32).floor() as i32;

        let distance = |cle: Cle| {
            let c = DVec3::from_array(direction(
                cle.0,
                (cle.1 * TC) as f64 + 16.0,
                (cle.2 * TC) as f64 + 16.0,
            )) * (rayon + crate::monde::NIVEAU_MER as f64);
            (c - position).length() as f32
        };

        let depart = {
            let (f, u, v, _) = replier_chunk(cam.face, ccu, ccv);
            (f, u, v)
        };
        let mut vus: HashSet<Cle> = HashSet::from([depart]);
        let mut file = VecDeque::from([depart]);
        let mut candidats: Vec<(Cle, f32)> = Vec::new();
        let limite = portee + 64.0;

        while let Some(cle) = file.pop_front() {
            let d = distance(cle);
            if d > limite {
                continue;
            }
            candidats.push((cle, d));
            for (du, dv) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (f, u, v, _) = replier_chunk(cle.0, cle.1 + du, cle.2 + dv);
                if vus.insert((f, u, v)) {
                    file.push_back((f, u, v));
                }
            }
        }
        candidats.sort_by(|a, b| a.1.total_cmp(&b.1));

        let mut visibles: Vec<(Cle, UnifChunk)> = Vec::new();
        let mut manquants: Vec<Cle> = Vec::new();

        for (cle, _) in &candidats {
            if !self.chunks.contains_key(cle) {
                manquants.push(*cle);
                continue;
            }
            let b = BASES[cle.0 as usize];
            visibles.push((
                *cle,
                UnifChunk {
                    base_r: [b.r[0] as f32, b.r[1] as f32, b.r[2] as f32, 0.0],
                    base_h: [b.h[0] as f32, b.h[1] as f32, b.h[2] as f32, 0.0],
                    base_n: [b.n[0] as f32, b.n[1] as f32, b.n[2] as f32, 0.0],
                    origine: [(cle.1 * TC) as f32, (cle.2 * TC) as f32, 0.0, 0.0],
                    teinte: teinte_chunk(*cle),
                },
            ));
        }

        for cle in manquants.into_iter().take(reglages.budget) {
            let chunk = Chunk::generer(gen, cle.0, cle.1, cle.2);
            let (sommets, indices) = maillage::mailler(&chunk);
            self.chunks.insert(cle, televerser(device, &sommets, &indices));
        }

        // On garde une marge : revenir sur ses pas ne doit pas tout regénérer.
        // Et jamais depuis un aperçu : les deux vues ne voient pas les mêmes
        // chunks, et se répondre l'une l'autre les ferait toutes deux
        // regénérer sans fin.
        if !apercu && self.chunks.len() > 3 * (2 * r as usize + 8).pow(2) {
            self.chunks.retain(|cle, _| vus.contains(cle));
        }

        // --- Le surligneur ----------------------------------------------------
        //
        // Il est calculé à plat par `viser`, puis passe par la même projection
        // que le terrain : c'est un « chunk » d'un bloc de côté.
        if let Some((fc, bu, bv, bz)) = vise {
            let base = BASES[fc as usize];
            visibles.push((
                (u8::MAX, i32::MIN, i32::MIN),
                UnifChunk {
                    base_r: [base.r[0] as f32, base.r[1] as f32, base.r[2] as f32, 0.0],
                    base_h: [base.h[0] as f32, base.h[1] as f32, base.h[2] as f32, 0.0],
                    base_n: [base.n[0] as f32, base.n[1] as f32, base.n[2] as f32, 0.0],
                    origine: [bu as f32, bv as f32, bz as f32, 0.0],
                    teinte: [1.0, 1.0, 1.0, 1.0],
                },
            ));
        }

        // --- Le portail -------------------------------------------------------
        //
        // Comme le surligneur : un « chunk » posé à sa case, qui passe par la
        // même projection que le terrain. C'est ce qui le fait tenir droit sur
        // le sol même près d'un coin, où le déroulé à plat l'aurait couché.
        // **Un portail ne se dessine jamais dans son propre aperçu.** C'est la
        // fenêtre par laquelle on regarde : la peindre reviendrait à poser sa
        // vitre devant la caméra qui doit voir au travers, et l'aperçu ne
        // montrerait que la vitre. C'est précisément ce qui est arrivé le jour
        // où le portail de sortie a reçu un cadre — les deux fenêtres se sont
        // bouché la vue l'une l'autre, et les deux nappes sont devenues plates.
        let mut nappe: Option<usize> = None;
        if let (false, Some((_, _, f, pu, pv, pz))) = (apercu, &self.portail) {
            let base = BASES[*f as usize];
            let unif = UnifChunk {
                base_r: [base.r[0] as f32, base.r[1] as f32, base.r[2] as f32, 0.0],
                base_h: [base.h[0] as f32, base.h[1] as f32, base.h[2] as f32, 0.0],
                base_n: [base.n[0] as f32, base.n[1] as f32, base.n[2] as f32, 0.0],
                origine: [*pu as f32 + 0.5, *pv as f32 + 0.5, *pz as f32, 0.0],
                teinte: [1.0, 1.0, 1.0, 1.0],
            };
            visibles.push(((u8::MAX - 1, i32::MIN, i32::MIN), unif));
            // La nappe partage le placement du cadre — elle occupe donc une
            // seconde entrée, avec le même uniforme à son propre décalage.
            // Sans aperçu prêt, elle se peint à plat par le pipeline ordinaire.
            if self.apercu_pret {
                nappe = Some(base_slot as usize + visibles.len());
            }
            visibles.push(((u8::MAX - 2, i32::MIN, i32::MIN), unif));
        }

        let mut octets: Vec<u8> = Vec::with_capacity(visibles.len() * ALIGNEMENT as usize);
        for (_, unif) in &visibles {
            let debut = octets.len();
            octets.extend_from_slice(bytemuck::bytes_of(unif));
            octets.resize(debut + ALIGNEMENT as usize, 0);
        }
        if !octets.is_empty() {
            queue.write_buffer(&self.buf_chunks, base_slot as u64 * ALIGNEMENT, &octets);
        }

        // Le groupe de liaison des coulisses est refait à chaque image : la
        // texture change dès qu'on redimensionne la fenêtre, et un groupe gardé
        // trop longtemps pointerait sur l'ancienne. Une allocation par image
        // pour une texture — moins cher que le bogue.
        let bg_coulisses = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("coulisses"),
            layout: &self.layout_coulisses,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.coulisses.couleur),
            }],
        });

        // --- Passe de rendu ---------------------------------------------------
        let mut passe = encodeur.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terrain"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: vue_couleur,
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
                view: vue_profondeur,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let bg_globaux = if apercu { &self.bg_globaux_apercu } else { &self.bg_globaux };
        passe.set_pipeline(&self.pipeline);
        passe.set_bind_group(0, bg_globaux, &[]);

        let mut dessines = 0;
        for (i, (cle, _)) in visibles.iter().enumerate() {
            // La nappe n'est dessinée ici que dans un aperçu, et à plat : le
            // pipeline du terrain rend simplement sa couleur de sommet. Dans la
            // passe principale elle a son propre pipeline et passe après la
            // boucle, avec les coulisses en main.
            if cle.0 == u8::MAX - 2 && nappe.is_some() {
                continue;
            }
            let maille = if cle.0 == u8::MAX {
                Some(&self.surligneur)
            } else if cle.0 == u8::MAX - 1 {
                self.portail.as_ref().map(|(cadre, ..)| cadre)
            } else if cle.0 == u8::MAX - 2 {
                self.portail.as_ref().map(|(_, nappe, ..)| nappe)
            } else {
                self.chunks.get(cle).and_then(|m| m.as_ref())
            };
            let Some(maille) = maille else { continue };

            let slot = base_slot + i as u32;
            passe.set_bind_group(1, &self.bg_chunks, &[slot * ALIGNEMENT as u32]);
            passe.set_vertex_buffer(0, maille.sommets.slice(..));
            passe.set_index_buffer(maille.indices.slice(..), wgpu::IndexFormat::Uint32);
            passe.draw_indexed(0..maille.nb, 0, 0..1);
            dessines += 1;
        }

        // --- La fenêtre -------------------------------------------------------
        //
        // En dernier, avec son pipeline et les coulisses en main. Le test de
        // profondeur ordinaire suffit à la faire cacher par le relief : c'est
        // une surface du monde comme une autre, elle ne triche pas.
        if let (Some(i), Some((_, maille, ..))) = (nappe, self.portail.as_ref()) {
            passe.set_pipeline(&self.pipeline_nappe);
            passe.set_bind_group(0, bg_globaux, &[]);
            passe.set_bind_group(1, &self.bg_chunks, &[i as u32 * ALIGNEMENT as u32]);
            passe.set_bind_group(2, &bg_coulisses, &[]);
            passe.set_vertex_buffer(0, maille.sommets.slice(..));
            passe.set_index_buffer(maille.indices.slice(..), wgpu::IndexFormat::Uint32);
            passe.draw_indexed(0..maille.nb, 0, 0..1);
            dessines += 1;
        }

        if !apercu {
            self.chunks_dessines = dessines;
        }
    }
}

// --------------------------------------------------------------------------
// Le second monde
// --------------------------------------------------------------------------

impl Vue3d {
    /// Dessine la poche : même pipeline, même tampon, même shader — un seul
    /// régime de plus, porté par `params.w`.
    ///
    /// Tout ce qui change tient dans trois lignes de la passe, et c'est le
    /// résultat qu'on venait chercher. D17 dit que le coût d'un second monde
    /// est architectural et non volumétrique : le rendu, lui, ne coûte presque
    /// rien. Ce qui coûte est ailleurs — dans l'aiguillage, dans les deux
    /// caches, dans l'état à restituer.
    pub fn dessiner_poche(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
        cible: &crate::rendu::Cible,
        poche: &Poche,
        cam: &CameraPlate,
        reglages: &Reglages,
    ) {
        // Le joueur est dans la poche : aucune autre passe ne tourne dans cette
        // image, la région ordinaire est libre.
        self.passe_poche(
            device,
            queue,
            encodeur,
            (&cible.couleur, &cible.profondeur, cible.taille),
            poche,
            cam.repere(),
            [0.0; 4],
            (0, 0),
            reglages,
        );
    }

    /// Peint le passé dans les coulisses, pour que la nappe l'y découpe.
    ///
    /// La caméra n'est pas celle d'un joueur : c'est celle du joueur du présent,
    /// transportée à travers le portail. Les deux images se recouvrent alors
    /// pixel pour pixel, et la fenêtre n'est plus qu'un découpage.
    #[allow(clippy::too_many_arguments)]
    pub fn dessiner_apercu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
        taille: (u32, u32),
        poche: &Poche,
        repere: (Vec3, Vec3, Vec3),
        coupe: [f32; 4],
        reglages: &Reglages,
    ) {
        self.coulisses.ajuster(device, taille);
        let (couleur, profondeur) = (
            // On ne peut pas emprunter `self.coulisses` et `self` en même temps
            // dans l'appel qui suit : on prend des vues à part.
            self.coulisses.couleur.clone(),
            self.coulisses.profondeur.clone(),
        );
        self.passe_poche(
            device,
            queue,
            encodeur,
            (&couleur, &profondeur, self.coulisses.taille),
            poche,
            repere,
            coupe,
            (ALIGNEMENT, BASE_APERCU),
            reglages,
        );
        self.apercu_pret = true;
    }

    /// Le passé est-il peint pour cette image ?
    pub fn oublier_apercu(&mut self) { self.apercu_pret = false; }

    pub fn apercu_pret(&self) -> bool { self.apercu_pret }

    /// Redescend du GPU ce que la fenêtre montrait.
    pub fn relire_coulisses(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        self.coulisses.relire(device, queue)
    }

    #[allow(clippy::too_many_arguments)]
    fn passe_poche(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
        sortie: (&wgpu::TextureView, &wgpu::TextureView, (u32, u32)),
        poche: &Poche,
        repere: (Vec3, Vec3, Vec3),
        coupe: [f32; 4],
        // Où écrire dans les deux tampons partagés : décalage des globaux, et
        // premier emplacement de chunk. Voir `BASE_APERCU`.
        region: (u64, u32),
        reglages: &Reglages,
    ) {
        let (decalage_globaux, base_slot) = region;
        let apercu = decalage_globaux != 0;
        let (vue_couleur, vue_profondeur, (largeur, hauteur)) = sortie;
        let aspect = largeur as f32 / hauteur as f32;
        // La salle fait 192 blocs de côté : la portée couvre sa diagonale, avec
        // de quoi voir le mur d'en face sans brume.
        let portee = poche::COTE as f32 * 2.2;

        let (position, avant, haut) = repere;
        let projection = Mat4::perspective_rh(reglages.champ.to_radians(), aspect, 0.2, portee);
        let vue = Mat4::look_to_rh(Vec3::ZERO, avant, haut);

        queue.write_buffer(
            &self.buf_globaux,
            decalage_globaux,
            bytemuck::bytes_of(&Globaux {
                vue_projection: (projection * vue).to_cols_array_2d(),
                camera: [position.x, position.y, position.z, 0.0],
                // Le quatrième : le régime. 1 = plat.
                params: [portee * 0.55, portee * 0.99, reglages.teinte_chunks, 1.0],
                planete: [poche::COTE as f32, 0.0, crate::conforme::N as f32, 0.0],
                ciel: poche::CIEL_POCHE,
                coupe,
            }),
        );

        // --- Quels chunks ----------------------------------------------------
        //
        // Un balayage rectangulaire, et c'est **légitime ici et seulement ici**.
        // La règle de D27 — ne jamais chercher les chunks visibles en balayant
        // un rectangle — vaut pour la sphère, où il en manque près d'un coin.
        // La poche n'a ni coin ni repliement : c'est un vrai plan fini, un
        // rectangle y est un rectangle. Trente-six chunks, tous visités.
        let mut visibles: Vec<((u32, i32, i32), UnifChunk)> = Vec::new();

        for cv in 0..poche::COTE_CHUNKS {
            for cu in 0..poche::COTE_CHUNKS {
                let cle = (poche.graine, cu, cv);
                // Pas de budget par image : la salle entière tient en
                // trente-six chunks. Les étaler ferait apparaître les murs par
                // morceaux, et on ne saurait plus si un trou est un défaut ou
                // une attente.
                self.chunks_poche.entry(cle).or_insert_with(|| {
                    let chunk = Chunk::poche(poche, cu, cv);
                    let (sommets, indices) = maillage::mailler(&chunk);
                    televerser(device, &sommets, &indices)
                });
                visibles.push((
                    cle,
                    UnifChunk {
                        // Le shader plat les ignore ; on les remplit quand même
                        // pour que le tampon reste lisible au debug.
                        base_r: [1.0, 0.0, 0.0, 0.0],
                        base_h: [0.0, 1.0, 0.0, 0.0],
                        base_n: [0.0, 0.0, 0.0, 0.0],
                        origine: [(cu * TC) as f32, (cv * TC) as f32, 0.0, 0.0],
                        teinte: teinte_poche(cu, cv),
                    },
                ));
            }
        }

        // --- Le portail de sortie ---------------------------------------------
        //
        // Posé à plat comme un chunk de la poche : le shader lit `origine` puis
        // ajoute les coordonnées locales, donc il suffit de l'ancrer au pied du
        // cadre. Rien à projeter — ici le monde est le plan.
        //
        // Et rien du tout dans un aperçu : c'est *par lui* qu'on regarde. La
        // caméra de l'aperçu se tient juste derrière sa nappe ; la dessiner
        // reviendrait à lui coller sa propre vitre sur l'objectif.
        let mut nappe_sortie = None;
        if !apercu {
            let unif_sortie = UnifChunk {
                base_r: [1.0, 0.0, 0.0, 0.0],
                base_h: [0.0, 1.0, 0.0, 0.0],
                base_n: [0.0, 0.0, 0.0, 0.0],
                origine: [poche::SORTIE_U, poche::SORTIE_V, poche::SORTIE_PIED, 0.0],
                teinte: [1.0, 1.0, 1.0, 1.0],
            };
            let cadre_sortie = visibles.len();
            visibles.push(((u32::MAX, i32::MIN, i32::MIN), unif_sortie));
            visibles.push(((u32::MAX - 1, i32::MIN, i32::MIN), unif_sortie));
            if self.apercu_pret {
                nappe_sortie = Some(base_slot as usize + cadre_sortie + 1);
            }
        }

        let mut octets: Vec<u8> = Vec::with_capacity(visibles.len() * ALIGNEMENT as usize);
        for (_, unif) in &visibles {
            let debut = octets.len();
            octets.extend_from_slice(bytemuck::bytes_of(unif));
            octets.resize(debut + ALIGNEMENT as usize, 0);
        }
        if !octets.is_empty() {
            queue.write_buffer(&self.buf_chunks, base_slot as u64 * ALIGNEMENT, &octets);
        }

        let bg_coulisses = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("coulisses"),
            layout: &self.layout_coulisses,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.coulisses.couleur),
            }],
        });

        let mut passe = encodeur.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("poche"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: vue_couleur,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: poche::CIEL_POCHE[0] as f64,
                        g: poche::CIEL_POCHE[1] as f64,
                        b: poche::CIEL_POCHE[2] as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: vue_profondeur,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let bg_globaux = if apercu { &self.bg_globaux_apercu } else { &self.bg_globaux };
        passe.set_pipeline(&self.pipeline);
        passe.set_bind_group(0, bg_globaux, &[]);

        let mut dessines = 0;
        for (i, (cle, _)) in visibles.iter().enumerate() {
            if cle.0 == u32::MAX - 1 && nappe_sortie.is_some() {
                continue;
            }
            let maille = if cle.0 == u32::MAX {
                Some(&self.sortie.0)
            } else if cle.0 == u32::MAX - 1 {
                Some(&self.sortie.1)
            } else {
                self.chunks_poche.get(cle).and_then(|m| m.as_ref())
            };
            let Some(maille) = maille else { continue };

            let slot = base_slot + i as u32;
            passe.set_bind_group(1, &self.bg_chunks, &[slot * ALIGNEMENT as u32]);
            passe.set_vertex_buffer(0, maille.sommets.slice(..));
            passe.set_index_buffer(maille.indices.slice(..), wgpu::IndexFormat::Uint32);
            passe.draw_indexed(0..maille.nb, 0, 0..1);
            dessines += 1;
        }

        // La fenêtre du retour, avec son pipeline.
        if let Some(i) = nappe_sortie {
            passe.set_pipeline(&self.pipeline_nappe);
            passe.set_bind_group(0, bg_globaux, &[]);
            passe.set_bind_group(1, &self.bg_chunks, &[i as u32 * ALIGNEMENT as u32]);
            passe.set_bind_group(2, &bg_coulisses, &[]);
            passe.set_vertex_buffer(0, self.sortie.1.sommets.slice(..));
            passe.set_index_buffer(self.sortie.1.indices.slice(..), wgpu::IndexFormat::Uint32);
            passe.draw_indexed(0..self.sortie.1.nb, 0, 0..1);
            dessines += 1;
        }

        if !apercu {
            self.chunks_dessines = dessines;
        }
    }
}

/// Une teinte par chunk de poche, pour le même usage que sur la sphère : voir
/// où passent les frontières.
fn teinte_poche(cu: i32, cv: i32) -> [f32; 4] {
    if (cu + cv).rem_euclid(2) == 0 {
        [1.25, 1.05, 1.35, 1.0]
    } else {
        [0.85, 0.95, 1.25, 1.0]
    }
}

/// Une boîte quelconque : un coin, et trois arêtes.
///
/// Le trièdre doit être direct — c'est le cas de `(droite, avant, Z)`, puisque
/// `droite = regard × haut` et `avant = regard` donnent `droite × avant = haut`.
/// L'enroulement est alors celui du reste du maillage, et le dos des faces se
/// fait bien éliminer.
fn boite(
    sommets: &mut Vec<Sommet>,
    indices: &mut Vec<u32>,
    coin: Vec3,
    a: Vec3,
    b: Vec3,
    c: Vec3,
    couleur: [f32; 3],
) {
    let p = |x: f32, y: f32, z: f32| coin + a * x + b * y + c * z;
    let coins = [
        p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.),
        p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.),
    ];
    // Mêmes faces, même ordre que le surligneur.
    let faces = [
        [0usize, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4],
        [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7],
    ];
    // Un peu d'ombrage par orientation, sinon le cadre n'est qu'une silhouette.
    let ombres = [0.55, 1.0, 0.78, 0.86, 0.78, 0.86];
    for (f, ombre) in faces.iter().zip(ombres) {
        let debut = sommets.len() as u32;
        for i in f.iter() {
            let s = coins[*i];
            sommets.push(Sommet {
                position: [s.x, s.y, s.z],
                couleur: [couleur[0] * ombre, couleur[1] * ombre, couleur[2] * ombre],
            });
        }
        indices.extend_from_slice(&[debut, debut + 1, debut + 2, debut, debut + 2, debut + 3]);
    }
}

/// Le cadre du portail : deux montants, un linteau, un seuil, et la nappe.
///
/// Les sommets sont engendrés depuis les **deux axes reçus**, exprimés en
/// coordonnées de la face du portail. Ces axes viennent du regard du joueur
/// décomposé sur place : le cadre est donc carré dans le monde, et non dans la
/// grille. Près d'un coin les deux ne coïncident pas — les tangentes s'y
/// coupent à 120° — et un cadre bâti sur `+u` et `+v` s'y montrerait de biais.
fn cadre_portail(
    device: &wgpu::Device,
    axe_droite: Vec2,
    axe_avant: Vec2,
) -> (MailleGpu, MailleGpu) {
    let mut sommets = Vec::new();
    let mut indices = Vec::new();

    let a = Vec3::new(axe_droite.x, axe_droite.y, 0.0);
    let b = Vec3::new(axe_avant.x, axe_avant.y, 0.0);
    let c = Vec3::Z;

    const PIERRE: [f32; 3] = [0.34, 0.27, 0.44];

    // Les blocs du cadre, en (i le long de la largeur, j en hauteur).
    let mut cases: Vec<(i32, i32)> = Vec::new();
    for j in 0..=4 {
        cases.push((-2, j));
        cases.push((2, j));
    }
    for i in -1..=1 {
        cases.push((i, 0));
        cases.push((i, 4));
    }

    let epaisseur = 0.7;
    for (i, j) in cases {
        let coin = a * (i as f32 - 0.5) - b * (epaisseur * 0.5);
        boite(
            &mut sommets,
            &mut indices,
            Vec3::new(coin.x, coin.y, j as f32),
            a,
            b * epaisseur,
            c,
            PIERRE,
        );
    }

    let cadre = televerser(device, &sommets, &indices).expect("cadre non vide");

    // La nappe : trois blocs de large, trois de haut, et **plate**.
    //
    // Une maille à part, parce qu'elle a son propre pipeline — c'est elle la
    // fenêtre. Et plate, désormais : tant qu'elle était violette, une boîte
    // fine faisait l'affaire ; maintenant qu'elle montre une autre image, une
    // épaisseur donnerait deux images décalées sur les tranches.
    let mut sommets = Vec::new();
    let mut indices = Vec::new();
    let (large, haut) = (3.0f32, 3.0f32);
    let coin = a * (-large * 0.5);
    let p = |x: f32, z: f32| {
        let d = coin + a * (large * x);
        Sommet { position: [d.x, d.y, 1.0 + haut * z], couleur: [0.62, 0.28, 0.95] }
    };
    sommets.extend_from_slice(&[p(0., 0.), p(1., 0.), p(1., 1.), p(0., 1.)]);
    indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    let nappe = televerser(device, &sommets, &indices).expect("nappe non vide");

    (cadre, nappe)
}

/// La table conforme, telle quelle, en texture. Le shader la lit avec les
/// mêmes octets et la même bilinéaire que le CPU : c'est cette identité qui
/// garde le réticule sur le bloc surligné.
fn texture_conforme(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let n = crate::conforme::N as u32;
    let (octets, pas) = crate::conforme::table().octets();

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("projection conforme"),
        size: wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &octets,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(pas),
            rows_per_image: Some(n),
        },
        wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
    );

    texture.create_view(&wgpu::TextureViewDescriptor::default())
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
    let (a, b) = (-0.04f32, 1.04f32);
    let coins = [
        [a, a, a], [b, a, a], [b, b, a], [a, b, a],
        [a, a, b], [b, a, b], [b, b, b], [a, b, b],
    ];
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

/// Une teinte par face, nuancée par chunk : le passage d'une face à l'autre se
/// voit, et les frontières de chunks avec.
fn teinte_chunk(cle: Cle) -> [f32; 4] {
    const FACES: [[f32; 3]; 6] = [
        [1.5, 0.7, 0.7],
        [0.7, 1.5, 0.7],
        [0.7, 0.7, 1.5],
        [1.4, 1.4, 0.6],
        [1.4, 0.6, 1.4],
        [0.6, 1.4, 1.4],
    ];
    let c = FACES[cle.0 as usize];
    let h = (cle.1.wrapping_mul(73_856_093) ^ cle.2.wrapping_mul(19_349_663)) as u32;
    let n = 0.85 + (h & 0xFF) as f32 / 850.0;
    [c[0] * n, c[1] * n, c[2] * n, 1.0]
}

// --------------------------------------------------------------------------
// Caméra et visée — à plat, toujours
// --------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Camera {
    /// La face où se trouve la caméra. C'est son repère qui fait loi pour tout
    /// ce qui interroge le monde.
    pub face: u8,
    /// Position en blocs, dans le repère de `face`.
    pub position: Vec3,
    /// Direction du regard, horizontale, unitaire, **dans le monde**.
    ///
    /// Surtout pas un cap rangé dans la grille. Rangée dans la grille,
    /// l'orientation hérite de son cisaillement : près d'un coin les tangentes
    /// se coupent à 120° et non à 90°, si bien qu'un quart de tour en
    /// coordonnées n'est plus un quart de tour dans le monde. Aucune
    /// compensation ne rattrape ça — la visée sautait de 25,6° au
    /// franchissement.
    ///
    /// Un vecteur du monde, lui, n'a pas à savoir dans quelles coordonnées on
    /// l'exprime. Le franchissement devient un non-événement pour
    /// l'orientation, par construction.
    pub regard: Vec3,
    pub tangage: f32,
}

impl Camera {
    /// La verticale locale et les deux tangentes du paramétrage, **telles
    /// quelles** — non orthogonalisées, donc à 120° l'une de l'autre près d'un
    /// coin. C'est leur non-orthogonalité qui rend la décomposition juste.
    ///
    /// Les tangentes sont mises à l'échelle du monde : une unité de coordonnée
    /// y vaut sa vraie longueur en blocs, laquelle varie de 0,69 à 1,00 selon
    /// l'endroit de la face.
    fn base(&self) -> (DVec3, DVec3, DVec3) {
        let (u, v) = (self.position.x as f64, self.position.y as f64);
        let haut = DVec3::from_array(direction(self.face, u, v));

        // Différence centrée sur un bloc, et **repliée** : elle ne se fait pas
        // tronquer au bord de la face.
        let tangente = |du: f64, dv: f64| {
            let devant = DVec3::from_array(direction_continue(self.face, u + du, v + dv));
            let derriere = DVec3::from_array(direction_continue(self.face, u - du, v - dv));
            let t = (devant - derriere) * RAYON;
            t - haut * t.dot(haut)
        };
        (haut, tangente(0.5, 0.0), tangente(0.0, 0.5))
    }

    fn regard_redresse(&self, haut: DVec3) -> DVec3 {
        let r = DVec3::new(
            self.regard.x as f64,
            self.regard.y as f64,
            self.regard.z as f64,
        );
        let plat = r - haut * r.dot(haut);
        if plat.length_squared() > 1e-12 {
            plat.normalize()
        } else {
            // Regard exactement vertical : n'importe quelle horizontale fera
            // l'affaire, autant prendre celle de la grille.
            self.base().1.normalize()
        }
    }

    /// Le repère de la caméra sur la planète : position, direction de visée et
    /// verticale locale.
    ///
    /// Le regard est redressé contre la verticale avant usage. Ce redressement
    /// **est** le transport parallèle discret : c'est lui qui garde le
    /// mouvement fluide quand la caméra se déplace sur la sphère.
    pub fn repere_3d(&self, rayon: f64) -> (DVec3, Vec3, Vec3) {
        let (u, v) = (self.position.x as f64, self.position.y as f64);
        let haut = DVec3::from_array(direction(self.face, u, v));
        let position = haut * (rayon + self.position.z as f64);

        let regard = self.regard_redresse(haut);
        let (st, ct) = (self.tangage as f64).sin_cos();
        let avant = (regard * ct + haut * st).normalize();

        (
            position,
            Vec3::new(avant.x as f32, avant.y as f32, avant.z as f32),
            Vec3::new(haut.x as f32, haut.y as f32, haut.z as f32),
        )
    }

    /// La droite de la caméra, dans le monde.
    pub fn droite(&self) -> Vec3 {
        let (haut, _, _) = self.base();
        let d = self.regard_redresse(haut).cross(haut);
        Vec3::new(d.x as f32, d.y as f32, d.z as f32)
    }

    /// Le regard, horizontal, dans le monde.
    pub fn avant_plat(&self) -> Vec3 {
        let (haut, _, _) = self.base();
        let r = self.regard_redresse(haut);
        Vec3::new(r.x as f32, r.y as f32, r.z as f32)
    }

    /// Fait tourner le regard autour de la verticale locale.
    pub fn tourner(&mut self, angle: f32) {
        let (haut, _, _) = self.base();
        let r = self.regard_redresse(haut);
        let (s, c) = (angle as f64).sin_cos();
        let tourne = (r * c + haut.cross(r) * s).normalize();
        self.regard = Vec3::new(tourne.x as f32, tourne.y as f32, tourne.z as f32);
    }

    /// Pose le regard depuis un angle lu dans le repère de la face.
    ///
    /// Commodité d'amorçage — pour les téléportations et les tests — et rien de
    /// plus : passé l'amorçage, le regard ne repasse plus jamais par la grille.
    pub fn poser_cap(&mut self, angle: f32) {
        let (haut, tu, _) = self.base();
        let est = (tu - haut * tu.dot(haut)).normalize();
        let nord = haut.cross(est);
        let (s, c) = (angle as f64).sin_cos();
        let r = (est * c + nord * s).normalize();
        self.regard = Vec3::new(r.x as f32, r.y as f32, r.z as f32);
    }

    /// Tourne le regard vers une direction du monde, projetée sur l'horizon.
    ///
    /// C'est ainsi qu'on vise un lieu depuis que le cap vit dans le monde :
    /// poser un angle dans le repère de la face ne désigne plus un endroit, car
    /// la marche suit une géodésique et non une ligne de grille.
    pub fn viser_point(&mut self, cible: Vec3) {
        let (haut, _, _) = self.base();
        let c = DVec3::new(cible.x as f64, cible.y as f64, cible.z as f64);
        let plat = c - haut * c.dot(haut);
        if plat.length_squared() > 1e-12 {
            let r = plat.normalize();
            self.regard = Vec3::new(r.x as f32, r.y as f32, r.z as f32);
        }
    }

    /// Décompose un déplacement tangent du monde sur les deux tangentes de la
    /// grille, en unités de coordonnée.
    ///
    /// Système 2×2 sur la matrice de Gram. Les tangentes ne sont jamais
    /// parallèles — 60° au pire, à un coin — donc le déterminant reste sain.
    ///
    /// C'est le seul endroit où le déplacement consulte la projection : le
    /// redressement à l'entrée que la règle de D27 prévoit. Le monde, lui,
    /// n'est toujours interrogé qu'à plat.
    pub fn vers_coordonnees(&self, d3: Vec3) -> (f32, f32) {
        let (haut, tu, tv) = self.base();
        let d = DVec3::new(d3.x as f64, d3.y as f64, d3.z as f64);
        let d = d - haut * d.dot(haut);

        let (guu, guv, gvv) = (tu.dot(tu), tu.dot(tv), tv.dot(tv));
        let det = guu * gvv - guv * guv;
        if det.abs() < 1e-12 {
            return (0.0, 0.0);
        }
        let (bu, bv) = (d.dot(tu), d.dot(tv));
        (
            ((bu * gvv - bv * guv) / det) as f32,
            ((bv * guu - bu * guv) / det) as f32,
        )
    }

    /// Replie la position dans le patron. Rend le nombre de quarts de tour si
    /// un bord a été franchi.
    ///
    /// **L'orientation n'est pas touchée.** Elle vit dans le monde ; changer de
    /// face ne change que la façon d'écrire la position.
    pub fn replier(&mut self) -> Option<u8> {
        let (fc, u, v, k) = replier_continu(
            self.face,
            self.position.x as f64,
            self.position.y as f64,
        );
        if fc == self.face && k == 0 {
            return None;
        }

        self.face = fc;
        self.position.x = u as f32;
        self.position.y = v as f32;
        Some(k)
    }

    /// Avance de `deplacement`, exprimé en coordonnées de la face, **en
    /// s'arrêtant à chaque bord franchi**.
    ///
    /// Sans ce découpage, un pas de plusieurs blocs qui sort de la face par
    /// deux bords à la fois laisse le repliement résoudre `u` puis `v`. Or
    /// franchir le bord `+u` en un point dont le `v` est déjà dehors, c'est
    /// franchir un prolongement virtuel de ce bord, qui ne correspond à rien
    /// sur la surface : le résultat ne dépend plus de la géométrie mais de
    /// l'ordre du code.
    ///
    /// Rend le nombre de bords franchis.
    pub fn avancer(&mut self, deplacement: Vec3) -> u32 {
        let mut reste = deplacement;
        let mut franchis = 0;

        for _ in 0..8 {
            let t = premiere_sortie(self.position, reste);
            if t >= 1.0 {
                self.position += reste;
                break;
            }

            // On se pose juste au-delà du bord, sinon le repliement ne le voit
            // pas : un millième de bloc suffit, et reste sous la résolution de
            // l'affichage.
            let horizontal = (reste.x * reste.x + reste.y * reste.y).sqrt();
            let marge = 2e-3 / horizontal.max(1e-6);
            let part = (t + marge).min(1.0);

            self.position += reste * part;
            reste *= 1.0 - part;

            if let Some(k) = self.replier() {
                franchis += 1;
                // Le reste du déplacement, lui, est bien en coordonnées : le
                // changement de repère y est exactement le quart de tour.
                let (cos, sin) = crate::cube::COS_SIN[k as usize];
                let (cos, sin) = (cos as f32, sin as f32);
                reste = Vec3::new(
                    reste.x * cos - reste.y * sin,
                    reste.x * sin + reste.y * cos,
                    reste.z,
                );
            }
        }

        // On range un regard propre, redressé contre la nouvelle verticale.
        self.regard = self.avant_plat();
        franchis
    }
}

/// Fraction du déplacement au bout de laquelle on quitte la face, ou plus de
/// `1` si on y reste.
fn premiere_sortie(p: Vec3, d: Vec3) -> f32 {
    let mut t = f32::INFINITY;
    for (c, v) in [(p.x, d.x), (p.y, d.y)] {
        if v > 0.0 {
            t = t.min((FACE as f32 - c) / v);
        } else if v < 0.0 {
            t = t.min(-c / v);
        }
    }
    t.max(0.0)
}

pub struct Reglages {
    pub distance_rendu: i32,
    pub champ: f32,
    /// Dosage de la teinte par face, de 0 à 1. Un interrupteur suffit à
    /// l'écran ; le film en veut juste assez pour lire la topologie sans
    /// perdre les couleurs du terrain.
    pub teinte_chunks: f32,
    /// Multiplie le rayon de rendu : le monde grossit et s'aplatit d'autant.
    /// Réglage de debug — il ment sur la taille des blocs, et c'est le seul
    /// endroit du programme qui s'y autorise.
    pub aplatissement: f32,
    /// Chunks générés par image. Au-delà d'une poignée le déplacement
    /// saccade ; en mode film on veut au contraire tout, tout de suite.
    pub budget: usize,
}

/// Le bloc visé : face, puis position dans cette face.
pub type Vise = (u8, i32, i32, i32);

/// Le rayon de visée part du réticule, donc de l'écran, donc d'une géométrie
/// courbe. Il est **redressé une fois** — `depuis_direction` inverse la
/// projection — puis le monde est interrogé à plat, case par case.
///
/// C'est la règle de D27 dans le bon sens. La version précédente marchait
/// droit dans le repère de la face en espérant que cela revienne au même :
/// `--diag` mesurait jusqu'à 45 blocs d'écart entre le réticule et le bloc
/// surligné. Une droite du repère plat n'est pas une droite à l'écran, et
/// aucune bonne volonté ne rend une projection inversible par accident.
pub fn viser(gen: &Generateur, cam: &Camera, rayon: f64, portee: f32) -> Option<Vise> {
    let (origine, avant, _) = cam.repere_3d(rayon);
    let avant = DVec3::new(avant.x as f64, avant.y as f64, avant.z as f64);

    let mut colonne: Option<(Cle, (i32, crate::monde::Biome))> = None;
    let pas = 0.12;
    let mut t = 0.0;
    // Tant qu'on n'a pas vu d'air, on est dans un bloc : viser depuis
    // l'intérieur d'une montagne ou depuis le fond de l'eau ne surligne rien.
    let mut vu_air = false;

    while t < portee as f64 {
        t += pas;
        let p = origine + avant * t;
        let bz = (p.length() - rayon).floor() as i32;
        if !(0..HAUTEUR_CHUNK).contains(&bz) {
            continue;
        }

        let (f, u, v) = depuis_direction(p.normalize().to_array());
        let (fc, bu, bv, _) = replier_bloc(f, u.floor() as i32, v.floor() as i32);
        let cle = (fc, bu, bv);
        if colonne.map(|(c, _)| c) != Some(cle) {
            colonne = Some((cle, gen.colonne(fc, bu, bv)));
        }
        let (sol, biome) = colonne.unwrap().1;
        if gen.bloc(sol, biome, bz) == Bloc::Air {
            vu_air = true;
        } else if vu_air {
            return Some((fc, bu, bv, bz));
        }
    }
    None
}
