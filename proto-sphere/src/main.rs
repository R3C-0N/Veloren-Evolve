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
mod conforme;
mod cube;
mod diag;
mod film;
mod interface;
mod maillage;
mod monde;
mod rendu;
mod vue2d;
mod vue3d;

use cube::{FACE, NET_H, NET_W, RAYON, vers_net};
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
    pub(crate) vise: Option<vue3d::Vise>,
    pub(crate) aretes: u32,
    pub(crate) ms: f32,
    pub(crate) refaire_carte: bool,
    pub(crate) oublier_chunks: bool,

    vue3d: Vue3d,
    vue2d: Vue2d,
    cible: Cible,
    horloge: Instant,
    film: Option<film::Film>,
}

impl App {
    fn nouvelle(cc: &eframe::CreationContext<'_>) -> Self {
        let etat = cc
            .wgpu_render_state
            .as_ref()
            .expect("proto-sphere exige le moteur de rendu wgpu");

        let graine = 1;
        let gen = Generateur::nouveau(graine);
        let vue3d = Vue3d::nouvelle(&etat.device, &etat.queue);
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
                // Amorçage : posé juste après, une fois la caméra construite.
                regard: Vec3::X,
                tangage: -0.22,
            },
            gen,
            graine,
            reglages: Reglages {
                distance_rendu: 10,
                champ: 70.0,
                teinte_chunks: 0.0,
                aplatissement: 1.0,
                budget: 8,
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
            film: None,
        }
    }

    /// Pose la caméra au-dessus du sol, quelque part sur le cube.
    pub(crate) fn aller(&mut self, face: u8, u: i32, v: i32) {
        self.cam.face = face;
        self.cam.position.x = u as f32 + 0.5;
        self.cam.position.y = v as f32 + 0.5;
        let _ = self.cam.replier();
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

    pub(crate) fn compteurs(&self) -> (usize, usize) {
        (self.vue3d.chunks_dessines, self.vue3d.chunks_en_memoire())
    }

    fn entrees(&mut self, ctx: &egui::Context, dt: f32, glisse: egui::Vec2, actif: bool) {
        if !actif {
            return;
        }

        // Regard : glisser dans la vue. Le tangage se bloque juste avant la
        // verticale, sinon la base de la caméra dégénère.
        if glisse != egui::Vec2::ZERO {
            // Le regard tourne autour de la verticale locale, dans le monde.
            self.cam.tourner(-glisse.x * 0.005);
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

        // L'intention est formée dans le monde : c'est là que vit le regard.
        let (_, vise, haut) = self.cam.repere_3d(RAYON);
        let d3 = vise * (avant * pas) + self.cam.droite() * (cote * pas);

        // Puis redressée une fois, à l'entrée, vers les coordonnées de la face.
        let montee = d3.dot(haut) + vertical * pas;
        let (du, dv) = self.cam.vers_coordonnees(d3);

        // Le seul endroit où la topologie touche le joueur — et elle le touche
        // par un déplacement découpé, pas par un saut suivi d'un repliement.
        self.aretes += self.cam.avancer(Vec3::new(du, dv, montee));
        self.cam.position.z = self
            .cam
            .position
            .z
            .clamp(1.0, monde::HAUTEUR_CHUNK as f32 - 2.0);
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

        if self.film.is_some() {
            self.tourner_le_film(ctx, frame);
            return;
        }
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
            let rayon = RAYON * self.reglages.aplatissement as f64;
            vue3d::viser(&self.gen, &self.cam, rayon, 260.0)
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

impl App {
    /// Une image du film : avancer sur la trajectoire, tout générer, rendre,
    /// relire, enregistrer.
    ///
    /// Rien n'est budgété ici : on veut le monde complet à chaque image,
    /// quitte à ce que la première prenne une seconde.
    fn tourner_le_film(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let etat = frame.wgpu_render_state().expect("moteur wgpu").clone();
        self.cible.ajuster(
            &etat.device,
            &mut etat.renderer.write(),
            (film::LARGEUR, film::HAUTEUR),
        );

        let Some(mut bobine) = self.film.take() else { return };
        if !bobine.avancer(&self.gen, &mut self.cam) {
            bobine.ecrire_journal();
            println!("film terminé : {} images dans {}", bobine.image, bobine.dossier);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Deux passes : la première génère les chunks entrés dans le champ,
        // la seconde les dessine. Sans cela, chaque image manquerait la
        // bordure que le déplacement vient de découvrir.
        for _ in 0..2 {
            let mut encodeur = etat.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("film") },
            );
            self.vue3d.dessiner(
                &etat.device,
                &etat.queue,
                &mut encodeur,
                &self.cible,
                &self.gen,
                &self.cam,
                &self.reglages,
                None,
            );
            etat.queue.submit(Some(encodeur.finish()));
        }

        bobine.poser(&self.cible.relire(&etat.device, &etat.queue));
        if bobine.image % 20 == 0 {
            println!("  image {} sur {}", bobine.image, film::IMAGES);
        }
        self.film = Some(bobine);
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

/// La terre ferme la plus proche d'un endroit donné, s'il y en a dans les
/// parages. Un coin de cube tombe souvent en mer, et une étendue d'eau plate
/// ne montre pas la forme de ses cases — or c'est elle qu'on vient juger.
fn terre_pres(gen: &Generateur, face: u8, u: i32, v: i32) -> (u8, i32, i32) {
    let mut meilleur = (face, u, v);
    let mut distance = i32::MAX;
    for dv in (-1400..=1400).step_by(28) {
        for du in (-1400..=1400).step_by(28) {
            let (sol, _) = gen.colonne(face, u + du, v + dv);
            let d = du * du + dv * dv;
            if sol > monde::NIVEAU_MER + 3 && d < distance {
                distance = d;
                meilleur = (face, u + du, v + dv);
            }
        }
    }
    meilleur
}

/// `--ou <lieu>`, `--teinte` et `--ras` : de quoi rejouer exactement la même
/// vue d'une fois sur l'autre, sans passer par la souris.
fn depart(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--teinte") {
        app.reglages.teinte_chunks = 1.0;
    }
    if let Some(i) = args.iter().position(|a| a == "--film") {
        let dossier = args.get(i + 1).cloned().unwrap_or_else(|| "film".into());
        let mut bobine = film::Film::nouveau(dossier);
        bobine.apex = args.iter().any(|a| a == "--apex");
        app.cam = bobine.debut(&app.gen);
        app.film = Some(bobine);
        app.reglages.budget = 4096;
        app.reglages.distance_rendu = 9;
        app.reglages.teinte_chunks = 0.45;
    }
    if args.iter().any(|a| a == "--ras") {
        // Au ras du sol : c'est la seule distance à laquelle la forme d'une
        // case se voit.
        let sol = app.gen.hauteur(
            app.cam.face,
            app.cam.position.x as i32,
            app.cam.position.y as i32,
        );
        app.cam.position.z = sol.max(monde::NIVEAU_MER as f32) + 3.0;
        app.cam.tangage = -0.30;
        app.reglages.distance_rendu = 5;
    }
    if let Some(i) = args.iter().position(|a| a == "--ou") {
        match args.get(i + 1).map(String::as_str) {
            Some("coin") => {
                let (f, u, v) = terre_pres(&app.gen, COIN.0, COIN.1, COIN.2);
                app.aller(f, u, v);
                app.cam.poser_cap(2.36);
                app.cam.tangage = -0.45;
            }
            Some("arete") => {
                let (f, u, v) =
                    terre_pres(&app.gen, MILIEU_ARETE.0, MILIEU_ARETE.1, MILIEU_ARETE.2);
                app.aller(f, u, v);
            }
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
