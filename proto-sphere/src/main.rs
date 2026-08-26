//! proto-sphere — banc d'essai de D27.
//!
//! Le monde est le patron d'un cube : six faces carrées dans une grille plate,
//! recollées par des rotations d'un quart de tour, rendues avec une courbure
//! d'horizon. Deux vues, un seul générateur : la 3D juge l'illusion, la carte
//! 2D juge la topologie.
//!
//! Ce que le prototype cherche à réfuter, plus qu'à montrer :
//! 1. qu'un recollement puisse rester invisible sans cas particulier ;
//! 2. que la courbure puisse rester un fait de rendu, sans fuir dans la
//!    sélection de bloc ;
//! 3. que le défaut de 90° des huit coins soit supportable en jeu.

mod chunk;
mod cube;
mod diag;
mod interface;
mod maillage;
mod monde;
mod rendu;
mod vue2d;
mod vue3d;

use cube::{FACE, NET_H, NET_W, vers_net};
use glam::Vec3;
use monde::Generateur;
use rendu::Cible;
use std::time::Instant;
use vue2d::Vue2d;
use vue3d::{Camera, Reglages, Vue3d};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Vue {
    Trois,
    Deux,
}

pub struct App {
    pub(crate) gen: Generateur,
    pub(crate) graine: u32,
    pub(crate) cam: Camera,
    pub(crate) reglages: Reglages,
    pub(crate) vue: Vue,
    pub(crate) vitesse: f32,
    pub(crate) vise: Option<[i32; 3]>,
    pub(crate) aretes: u32,
    pub(crate) ms: f32,
    pub(crate) refaire_carte: bool,
    pub(crate) oublier_chunks: bool,

    vue3d: Vue3d,
    vue2d: Vue2d,
    cible: Cible,
    horloge: Instant,
}

impl App {
    fn nouvelle(cc: &eframe::CreationContext<'_>) -> Self {
        let etat = cc
            .wgpu_render_state
            .as_ref()
            .expect("proto-sphere exige le moteur de rendu wgpu");

        let graine = 1;
        let gen = Generateur::nouveau(graine);
        let vue3d = Vue3d::nouvelle(&etat.device);
        let vue2d = Vue2d::nouvelle(&etat.device, &etat.queue, &gen);
        let cible = Cible::nouvelle(&etat.device, &mut etat.renderer.write(), (1280, 720));

        let (face, u, v) = monde::point_apparition(&gen);

        Self {
            cam: Camera {
                face,
                position: Vec3::new(
                    u as f32 + 0.5,
                    v as f32 + 0.5,
                    hauteur_de_vol(&gen, face, u, v),
                ),
                lacet: 0.0,
                tangage: -0.22,
            },
            gen,
            graine,
            reglages: Reglages {
                rayon_courbure: 1500.0,
                distance_rendu: 8,
                champ: 70.0,
                teinte_chunks: false,
                montrer_defaut: false,
            },
            vue: Vue::Trois,
            vitesse: 24.0,
            vise: None,
            aretes: 0,
            ms: 0.0,
            refaire_carte: false,
            oublier_chunks: false,
            vue3d,
            vue2d,
            cible,
            horloge: Instant::now(),
        }
    }

    /// Pose la caméra au-dessus du sol, quelque part sur le cube.
    pub(crate) fn aller(&mut self, face: u8, u: i32, v: i32) {
        self.cam.face = face;
        self.cam.position.x = u as f32 + 0.5;
        self.cam.position.y = v as f32 + 0.5;
        self.cam.replier();
        self.cam.position.z = hauteur_de_vol(
            &self.gen,
            self.cam.face,
            self.cam.position.x as i32,
            self.cam.position.y as i32,
        );
    }

    pub(crate) fn aller_apparition(&mut self) {
        let (face, u, v) = monde::point_apparition(&self.gen);
        self.aller(face, u, v);
    }

    pub(crate) fn compteurs(&self) -> (usize, usize, usize) {
        (
            self.vue3d.chunks_dessines,
            self.vue3d.chunks_en_memoire(),
            self.vue3d.doublons,
        )
    }

    fn entrees(&mut self, ctx: &egui::Context, dt: f32, glisse: egui::Vec2, actif: bool) {
        if !actif {
            return;
        }

        // Regard : glisser dans la vue. Le tangage se bloque juste avant la
        // verticale, sinon la base de la caméra dégénère.
        if glisse != egui::Vec2::ZERO {
            self.cam.lacet -= glisse.x * 0.005;
            self.cam.tangage = (self.cam.tangage - glisse.y * 0.005).clamp(-1.55, 1.55);
        }

        if ctx.wants_keyboard_input() {
            return;
        }

        let (mut avant, mut cote, mut vertical) = (0.0f32, 0.0f32, 0.0f32);
        let mut facteur = 1.0;

        ctx.input(|i| {
            use egui::Key;
            // ZQSD et WASD à la fois : le clavier de la machine décide.
            if i.key_down(Key::W) || i.key_down(Key::Z) {
                avant += 1.0;
            }
            if i.key_down(Key::S) {
                avant -= 1.0;
            }
            if i.key_down(Key::A) || i.key_down(Key::Q) {
                cote -= 1.0;
            }
            if i.key_down(Key::D) {
                cote += 1.0;
            }
            if i.key_down(Key::Space) {
                vertical += 1.0;
            }
            if i.key_down(Key::C) {
                vertical -= 1.0;
            }
            if i.modifiers.shift {
                facteur *= 5.0;
            }
            if i.modifiers.ctrl {
                facteur *= 0.2;
            }
            if i.key_pressed(Key::F1) {
                self.vue = match self.vue {
                    Vue::Trois => Vue::Deux,
                    Vue::Deux => Vue::Trois,
                };
            }
        });

        let pas = self.vitesse * facteur * dt;
        let dir = self.cam.avant();
        let droite = self.cam.droite();
        self.cam.position += dir * (avant * pas) + droite * (cote * pas);
        self.cam.position.z += vertical * pas;
        self.cam.position.z = self
            .cam
            .position
            .z
            .clamp(1.0, monde::HAUTEUR_CHUNK as f32 - 2.0);

        // Le seul endroit où la topologie touche le joueur.
        if self.cam.replier() != 0 {
            self.aretes += 1;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let dt = self.horloge.elapsed().as_secs_f32().min(0.1);
        self.horloge = Instant::now();
        self.ms = self.ms * 0.9 + dt * 1000.0 * 0.1;

        egui::SidePanel::right("debug")
            .default_width(320.0)
            .show(ctx, |ui| self.menu(ui));

        let mut zone = egui::Rect::NOTHING;
        let mut glisse = egui::Vec2::ZERO;
        let mut actif = false;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let (rect, reponse) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                zone = rect;
                glisse = reponse.drag_delta();
                actif = true;

                ui.painter().image(
                    self.cible.id,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                // Réticule : c'est lui qui doit coïncider avec le surligneur,
                // quelle que soit la courbure.
                if self.vue == Vue::Trois {
                    let c = rect.center();
                    let trait_ = egui::Stroke::new(1.5_f32, egui::Color32::from_white_alpha(200));
                    ui.painter()
                        .line_segment([c - egui::vec2(9.0, 0.0), c + egui::vec2(9.0, 0.0)], trait_);
                    ui.painter()
                        .line_segment([c - egui::vec2(0.0, 9.0), c + egui::vec2(0.0, 9.0)], trait_);
                }
            });

        self.entrees(ctx, dt, glisse, actif);

        let etat = frame.wgpu_render_state().expect("moteur wgpu").clone();

        if self.oublier_chunks {
            self.oublier_chunks = false;
            self.gen = Generateur::nouveau(self.graine);
            self.vue3d.oublier_tout();
        }
        if self.refaire_carte {
            self.refaire_carte = false;
            self.vue2d.regenerer(&etat.queue, &self.gen);
        }

        let taille = (
            (zone.width() * ctx.pixels_per_point()).max(1.0) as u32,
            (zone.height() * ctx.pixels_per_point()).max(1.0) as u32,
        );
        self.cible
            .ajuster(&etat.device, &mut etat.renderer.write(), taille);

        self.vise = if self.vue == Vue::Trois {
            vue3d::viser(&self.gen, &self.cam, 220.0)
        } else {
            None
        };

        let mut encodeur = etat
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene"),
            });

        match self.vue {
            Vue::Trois => self.vue3d.dessiner(
                &etat.device,
                &etat.queue,
                &mut encodeur,
                &self.cible,
                &self.gen,
                &self.cam,
                &self.reglages,
                self.vise,
            ),
            Vue::Deux => {
                let (nx, ny) = vers_net(
                    self.cam.face,
                    self.cam.position.x as i32,
                    self.cam.position.y as i32,
                );
                self.vue2d.dessiner(
                    &etat.queue,
                    &mut encodeur,
                    &self.cible,
                    [nx as f32 / NET_W as f32, 1.0 - ny as f32 / NET_H as f32],
                )
            }
        }

        etat.queue.submit(Some(encodeur.finish()));
        ctx.request_repaint();
    }
}

/// Altitude où poser la caméra : au-dessus du sol, jamais sous l'eau, et assez
/// haut pour que l'horizon soit dans le champ — c'est lui qu'on vient juger.
fn hauteur_de_vol(gen: &Generateur, face: u8, u: i32, v: i32) -> f32 {
    gen.hauteur(face, u, v).max(monde::NIVEAU_MER as f32) + 34.0
}

/// Le milieu d'une arête et un coin de face, pour les boutons du menu.
pub(crate) const MILIEU_ARETE: (u8, i32, i32) = (1, FACE / 2, FACE - 2);
pub(crate) const COIN: (u8, i32, i32) = (1, 2, FACE - 3);

/// `--ou <lieu>` et `--defaut` : de quoi rejouer exactement la même vue d'une
/// fois sur l'autre, sans passer par la souris.
fn depart(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--defaut") {
        app.reglages.montrer_defaut = true;
    }
    if args.iter().any(|a| a == "--teinte") {
        app.reglages.teinte_chunks = true;
    }
    if let Some(i) = args.iter().position(|a| a == "--ou") {
        match args.get(i + 1).map(String::as_str) {
            Some("coin") => {
                app.aller(COIN.0, COIN.1, COIN.2);
                // Regarder *vers* le coin : c'est au-delà de lui que le
                // déroulement doit inventer ses 90°.
                app.cam.lacet = 2.36;
                app.cam.tangage = -0.45;
                app.cam.position.z += 40.0;
            }
            Some("arete") => app.aller(MILIEU_ARETE.0, MILIEU_ARETE.1, MILIEU_ARETE.2),
            Some("nord") => app.aller(4, FACE / 2, FACE / 2),
            Some("sud") => app.aller(5, FACE / 2, FACE / 2),
            _ => {}
        }
    }
}

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--diag") {
        diag::executer();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1340.0, 760.0])
            .with_title("proto-sphere — D27 : le monde est le patron d'un cube"),
        ..Default::default()
    };

    eframe::run_native(
        "proto-sphere",
        options,
        Box::new(|cc| {
            let mut app = App::nouvelle(cc);
            depart(&mut app);
            Ok(Box::new(app))
        }),
    )
}
