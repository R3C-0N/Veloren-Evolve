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

mod ancre;
mod chunk;
mod conforme;
mod cube;
mod diag;
mod film;
mod film_ancre;
mod interface;
mod maillage;
mod monde;
mod poche;
mod rendu;
mod vue2d;
mod vue3d;

use ancre::{DUREE_FENETRE, Portail, Sejour};
use cube::{FACE, NET_H, NET_W, RAYON, vers_net};
use glam::Vec3;
use monde::Generateur;
use poche::Poche;
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

    /// Où se trouve le joueur — l'aiguillage entre les deux mondes.
    pub(crate) sejour: Sejour,
    /// Le portail ouvert, s'il y en a un. Il vit dans le monde sphérique et
    /// survit à la traversée : c'est par lui qu'on est revenu.
    pub(crate) portail: Option<Portail>,
    /// Combien de fenêtres ont été ouvertes. Sert de graine d'instance : une
    /// fenêtre rouverte n'est jamais la précédente (D9, D17).
    pub(crate) fenetres: u32,
    /// Le levier a été actionné ; reste à l'appliquer là où le `Device` existe.
    pub(crate) levier: bool,
    /// Ce qu'il reste de la fenêtre, en secondes.
    ///
    /// Elle appartient à l'ancre, pas au séjour : depuis qu'on peut revenir
    /// dans le présent **par le portail**, elle continue de s'user pendant
    /// qu'on n'est plus dans le passé, et se refermera là où le joueur se
    /// trouvera (D16).
    pub(crate) fenetre: Option<f32>,
    /// L'ancre de la dernière fenêtre ouverte, gardée après sa fermeture.
    ///
    /// Elle ne sert plus à revenir — c'est déjà fait — mais à dire de combien on
    /// s'en est écarté depuis. Au moment du retour, ce doit être zéro.
    pub(crate) derniere_ancre: Option<ancre::Ancre>,

    vue3d: Vue3d,
    vue2d: Vue2d,
    cible: Cible,
    horloge: Instant,
    film: Option<film::Film>,
    bobine_ancre: Option<film_ancre::FilmAncre>,
    /// `--ancre` / `--poche` : à appliquer à la première image, là où le
    /// `Device` existe. Le second entre aussi.
    amorce: Option<bool>,
    /// `--vue <fichier>` : une image, puis on quitte.
    portrait: Option<String>,
    /// `--poche-retour` : se retourner vers le portail de sortie en arrivant.
    demi_tour: bool,
    /// `--coulisses` : enregistrer ce que la fenêtre montre, plutôt que l'écran.
    ///
    /// Une fenêtre qui affiche n'importe quoi ne dit pas si le tort vient de la
    /// caméra virtuelle, du plan de coupe ou de l'échantillonnage. Pouvoir
    /// regarder les coulisses telles quelles tranche la question d'un coup.
    coulisses: bool,
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
            sejour: Sejour::Sphere,
            portail: None,
            fenetres: 0,
            levier: false,
            fenetre: None,
            derniere_ancre: None,
            vue3d,
            vue2d,
            cible,
            horloge: Instant::now(),
            film: None,
            bobine_ancre: None,
            amorce: None,
            portrait: None,
            demi_tour: false,
            coulisses: false,
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
            self.pivoter(-glisse.x * 0.005, -glisse.y * 0.005);
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
            // Le levier de l'ancre. Il ne fait rien tout de suite : poser un
            // cadre demande le `Device`, qui n'existe qu'un peu plus loin dans
            // l'image. Même patron que `refaire_carte`.
            if i.key_pressed(Key::P) {
                self.levier = true;
            }
        });

        self.deplacer(avant, cote, vertical, self.vitesse * facteur * dt);
    }

    /// Tourne le regard, dans le monde où l'on se trouve.
    pub(crate) fn pivoter(&mut self, lacet: f32, tangage: f32) {
        match &mut self.sejour {
            // Le regard tourne autour de la verticale locale, dans le monde.
            Sejour::Sphere => {
                self.cam.tourner(lacet);
                self.cam.tangage = (self.cam.tangage + tangage).clamp(-1.55, 1.55);
            }
            Sejour::Poche { cam, .. } => {
                cam.tourner(lacet);
                cam.tangage = (cam.tangage + tangage).clamp(-1.55, 1.55);
            }
        }
    }

    /// Avance de `pas` blocs, dans le monde où l'on se trouve.
    ///
    /// Le clavier et le film passent tous deux par ici, et c'est voulu : un
    /// film qui doublerait le déplacement au lieu de le conduire filmerait sa
    /// propre copie du jeu, et ne prouverait plus rien de celui-ci.
    ///
    /// Rend `true` si le pas a traversé le portail.
    pub(crate) fn deplacer(&mut self, avant: f32, cote: f32, vertical: f32, pas: f32) -> bool {
        if self.sejour.dans_la_poche() {
            self.deplacer_dans_la_poche(avant, cote, vertical, pas)
        } else {
            self.deplacer_sur_la_sphere(avant, cote, vertical, pas)
        }
    }

    fn deplacer_sur_la_sphere(&mut self, avant: f32, cote: f32, vertical: f32, pas: f32) -> bool {
        // L'intention est formée dans le monde : c'est là que vit le regard.
        let (depart, vise, haut) = self.cam.repere_3d(RAYON);
        let d3 = vise * (avant * pas) + self.cam.droite() * (cote * pas);

        // Puis redressée une fois, à l'entrée, vers les coordonnées de la face.
        let montee = d3.dot(haut) + vertical * pas;
        let (du, dv) = self.cam.vers_coordonnees(d3);
        let intention = Vec3::new(du, dv, montee);

        // Où ce pas mènerait-il ? On le joue sur une copie avant de décider : le
        // franchissement se teste sur le **segment**, pas sur le point
        // d'arrivée. À 400 blocs par seconde un pas fait sept blocs, et un test
        // ponctuel traverserait la nappe sans la voir.
        let mut essai = self.cam;
        essai.avancer(intention);
        let arrivee = essai.repere_3d(RAYON).0;

        let coupe = self
            .portail
            .as_ref()
            .and_then(|p| p.franchi(depart, arrivee));

        let Some(t) = coupe else {
            // Le seul endroit où la topologie touche le joueur — et elle le
            // touche par un déplacement découpé, pas par un saut suivi d'un
            // repliement.
            self.aretes += self.cam.avancer(intention);
            self.cam.position.z = self
                .cam
                .position
                .z
                .clamp(1.0, monde::HAUTEUR_CHUNK as f32 - 2.0);
            return false;
        };

        // --- La traversée -----------------------------------------------------
        //
        // Aucune téléportation : on s'arrête **sur** la nappe, on relit l'état
        // dans le repère du portail, et on repart de l'autre côté avec ce qui
        // restait du pas. La caméra qui en sort est celle-là même qui peignait
        // l'aperçu à l'image d'avant — c'est ce qui fait qu'il n'y a pas de
        // couture : ce que la fenêtre montrait est ce qu'on obtient.
        self.aretes += self.cam.avancer(intention * t);
        let Some(portail) = self.portail.as_ref() else { return false };

        let mut plate = portail.camera_de_la_poche(&self.cam);
        let reste = (arrivee - depart) * (1.0 - t) as f64;
        plate.avancer(portail.dans_le_repere(Vec3::new(
            reste.x as f32,
            reste.y as f32,
            reste.z as f32,
        )));

        let poche = Poche::nouvelle(portail.graine);
        self.sejour = Sejour::Poche { retour: portail.retour, cam: plate, poche };
        true
    }

    fn deplacer_dans_la_poche(&mut self, avant: f32, cote: f32, vertical: f32, pas: f32) -> bool {
        let Sejour::Poche { cam, .. } = &self.sejour else { return false };
        let mut plate = *cam;

        // Ici, rien de la sphère. Pas de face, pas de repliement, pas de
        // projection à consulter : le monde est le plan.
        let (_, vise, _) = plate.repere();
        let d = vise * (avant * pas) + plate.droite() * (cote * pas);
        let d = Vec3::new(d.x, d.y, d.z + vertical * pas);
        let (depart, arrivee) = (plate.position, plate.position + d);

        let Some(t) = poche::franchi_sortie(depart, arrivee) else {
            plate.avancer(d);
            if let Sejour::Poche { cam, .. } = &mut self.sejour {
                *cam = plate;
            }
            return false;
        };

        // Le retour, physique lui aussi. `camera_de_la_sphere` appelle
        // `depuis_direction` : sortir du passé demande l'inverse de la
        // projection, que D27 exige et que `--diag` mesure au millième de bloc.
        let Some(portail) = self.portail.as_ref() else { return false };
        plate.position = depart + d * t;
        let mut cam = portail.camera_de_la_sphere(&plate);

        let reste = portail.depuis_le_repere(d * (1.0 - t));
        let (_, _, haut) = cam.repere_3d(RAYON);
        let (du, dv) = cam.vers_coordonnees(reste);
        self.aretes += cam.avancer(Vec3::new(du, dv, reste.dot(haut)));

        self.cam = cam;
        self.sejour = Sejour::Sphere;
        true
    }

    /// Le levier, à trois positions.
    ///
    /// | Où l'on est | Ce que `P` fait |
    /// |---|---|
    /// | Sphère, pas de portail | Pose le portail, et l'ancre avec |
    /// | Sphère, portail ouvert | Referme la fenêtre |
    /// | Dans la poche | Expulse, et referme |
    fn actionner_ancre(&mut self, device: &wgpu::Device) {
        match self.sejour {
            Sejour::Poche { .. } => self.expulser(device),
            Sejour::Sphere if self.portail.is_some() => {
                self.portail = None;
                self.fenetre = None;
                self.vue3d.poser_portail(device, None);
                self.vue3d.oublier_poche();
            }
            Sejour::Sphere => {
                self.fenetres += 1;
                let portail = Portail::ouvrir(&self.gen, &self.cam, self.fenetres);
                self.derniere_ancre = Some(portail.retour);
                self.fenetre = Some(DUREE_FENETRE);
                self.vue3d.poser_portail(device, Some(&portail));
                self.portail = Some(portail);
            }
        }
    }

    /// Entrer sans marcher : pour `--poche`, et rien d'autre.
    ///
    /// Le joueur, lui, ne passe jamais par ici — il traverse. C'est une
    /// commodité de démarrage, et la seule chose du programme qui dépose
    /// quelqu'un dans la poche sans qu'il ait franchi quoi que ce soit.
    fn entrer_dans_la_poche(&mut self) {
        let Some(portail) = self.portail.as_ref() else { return };
        let poche = Poche::nouvelle(portail.graine);
        self.sejour = Sejour::Poche { retour: portail.retour, cam: poche.depart(), poche };
    }

    /// `--ancre` et `--poche` : la même chose que la touche, à la première
    /// image.
    ///
    /// Ce que `--diag` prouve, il ne le montre pas — et le rendu plat, le cadre
    /// du portail et la marche dans la salle ne se mesurent pas, ils se
    /// regardent. Ces deux drapeaux existent pour qu'on puisse les regarder
    /// sans chercher la touche.
    fn amorcer_ancre(&mut self, device: &wgpu::Device, entrer: bool) {
        self.actionner_ancre(device);
        if entrer {
            self.entrer_dans_la_poche();
        }
        if self.demi_tour {
            if let Sejour::Poche { cam, .. } = &mut self.sejour {
                cam.regard = -cam.regard;
                cam.position.y += 14.0;
            }
        }
    }

    /// La fenêtre se referme.
    ///
    /// Le retour est une **recopie** des quatre champs mémorisés : ni
    /// repliement, ni normalisation, ni « repose la caméra au-dessus du sol ».
    /// C'est ce qui le rend exact au bit près, et c'est ce que `--diag` mesure.
    ///
    /// On ne sort pas d'un donjon, on en est expulsé (D8) : la touche et
    /// l'expiration du minuteur empruntent le même chemin, et le portail
    /// disparaît dans les deux cas.
    fn expulser(&mut self, device: &wgpu::Device) {
        let Sejour::Poche { retour, .. } = &self.sejour else { return };
        retour.restituer(&mut self.cam);
        self.sejour = Sejour::Sphere;
        self.portail = None;
        self.fenetre = None;
        self.vue3d.poser_portail(device, None);
        // D9 : la fenêtre refermée efface ce qu'on y a bâti.
        self.vue3d.oublier_poche();
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
        if self.bobine_ancre.is_some() {
            self.tourner_le_film_de_l_ancre(ctx, frame);
            return;
        }
        self.entrees(ctx, dt, glisse, actif);

        let etat = frame.wgpu_render_state().expect("moteur wgpu").clone();

        // La fenêtre s'use, où qu'on se trouve. À zéro elle se referme d'elle-
        // même, par le même chemin que la touche — D8 : on n'en sort pas, on en
        // est expulsé. Depuis le présent, elle se contente de disparaître.
        if let Some(reste) = &mut self.fenetre {
            *reste -= dt;
            if *reste <= 0.0 {
                self.levier = true;
            }
        }
        if let Some(entrer) = self.amorce.take() {
            self.amorcer_ancre(&etat.device, entrer);
        }
        if self.levier {
            self.levier = false;
            self.actionner_ancre(&etat.device);
        }
        if self.portrait.is_some() {
            self.tirer_le_portrait(ctx, frame);
            return;
        }

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

        // La visée est celle de la sphère : dans la poche elle n'a pas d'objet,
        // et l'y appeler reviendrait à interroger un monde depuis l'autre.
        self.vise = if self.vue == Vue::Trois && !self.sejour.dans_la_poche() {
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

        // L'aiguillage. C'est la seule ligne du programme qui sache que deux
        // mondes existent, et c'est ce qu'on est venu chiffrer : le coût d'un
        // second monde n'est pas ici, il est dans tout ce qu'il a fallu tenir
        // disjoint pour que cette ligne suffise (D17).
        match (self.vue, &self.sejour) {
            (Vue::Trois, _) => self.peindre(&etat.device, &etat.queue, &mut encodeur),
            (Vue::Deux, _) => {
                // Depuis la poche, la carte du cube n'a qu'un sens : montrer où
                // l'on reviendra. Le marqueur se pose donc sur l'ancre.
                let repere = match &self.sejour {
                    Sejour::Sphere => (self.cam.face, self.cam.position),
                    Sejour::Poche { retour, .. } => (retour.face, retour.position),
                };
                let (nx, ny) = vers_net(repere.0, repere.1.x as i32, repere.1.y as i32);
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

impl App {
    /// Peint la scène : le passé dans les coulisses s'il y a une fenêtre
    /// ouverte, puis le monde où l'on se trouve.
    ///
    /// Les quatre appelants — la boucle, les deux films, `--vue` — passent tous
    /// par ici. C'est ce qui garantit qu'un film montre bien ce que le jeu
    /// affiche, et non une variante écrite à côté.
    fn peindre(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encodeur: &mut wgpu::CommandEncoder,
    ) {
        // D'abord l'autre monde, dans les coulisses : c'est lui que la nappe
        // ira découper. Les deux sens sont symétriques — depuis le présent on
        // peint le passé, depuis le passé on peint le présent — et chacun coupe
        // ce qui se trouve derrière sa propre caméra virtuelle.
        self.vue3d.oublier_apercu();
        match (&self.sejour, &self.portail) {
            (Sejour::Sphere, Some(portail)) => {
                let repere = portail.vers_la_poche(&self.cam);
                self.vue3d.dessiner_apercu(
                    device,
                    queue,
                    encodeur,
                    self.cible.taille,
                    &Poche::nouvelle(portail.graine),
                    repere,
                    poche::COUPE_SORTIE,
                    &self.reglages,
                );
            }
            (Sejour::Poche { cam, .. }, Some(portail)) => {
                let vue = portail.camera_de_la_sphere(cam);
                let coupe = portail.coupe_entree();
                self.vue3d.dessiner_apercu_sphere(
                    device,
                    queue,
                    encodeur,
                    self.cible.taille,
                    &self.gen,
                    &vue,
                    &self.reglages,
                    coupe,
                );
            }
            _ => {}
        }

        match &self.sejour {
            Sejour::Poche { poche, cam, .. } => self.vue3d.dessiner_poche(
                device,
                queue,
                encodeur,
                &self.cible,
                poche,
                cam,
                &self.reglages,
            ),
            Sejour::Sphere => self.vue3d.dessiner(
                device,
                queue,
                encodeur,
                &self.cible,
                &self.gen,
                &self.cam,
                &self.reglages,
                self.vise,
            ),
        }
    }

    /// Une image du film de l'ancre.
    ///
    /// L'ordre est appliqué **par les mêmes méthodes que le clavier**, puis la
    /// scène est rendue par l'aiguillage ordinaire. Le film ne sait pas dans
    /// quel monde il se trouve : il demande, le jeu répond.
    fn tourner_le_film_de_l_ancre(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        use film_ancre::Ordre;

        let etat = frame.wgpu_render_state().expect("moteur wgpu").clone();
        self.cible.ajuster(
            &etat.device,
            &mut etat.renderer.write(),
            (film::LARGEUR, film::HAUTEUR),
        );

        let Some(mut bobine) = self.bobine_ancre.take() else { return };
        let Some((_, ordre)) = bobine.ordre() else {
            bobine.ecrire_journal();
            println!("film terminé : {} images dans {}", bobine.image, bobine.dossier);
            bobine.resumer();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };

        // --- L'ordre, par les mêmes chemins que le joueur --------------------
        let etait_dans_la_poche = self.sejour.dans_la_poche();
        let mut evenement = None;

        match ordre {
            Ordre::Marcher(blocs) => {
                if self.deplacer(1.0, 0.0, 0.0, blocs) {
                    let quoi = if etait_dans_la_poche {
                        "retour dans le présent"
                    } else {
                        "entrée dans le passé"
                    };
                    evenement = Some(quoi);
                    // La mesure se prendra sur l'image qui suit, contre l'aperçu
                    // gardé de l'image d'avant.
                    bobine.signaler_traversee(quoi);
                }
            }
            Ordre::Tourner(angle) => self.pivoter(angle, 0.0),
            Ordre::Attendre => {}
            Ordre::Levier => {
                self.actionner_ancre(&etat.device);
                evenement = Some(if etait_dans_la_poche {
                    "la fenêtre se referme — expulsion"
                } else if self.portail.is_some() {
                    "l'ancre s'ouvre"
                } else {
                    "la fenêtre se referme"
                });
            }
        }

        // --- Deux passes, comme le film de coin ------------------------------
        self.vise = None;
        for _ in 0..2 {
            let mut encodeur = etat.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("film-ancre") },
            );
            self.peindre(&etat.device, &etat.queue, &mut encodeur);
            etat.queue.submit(Some(encodeur.finish()));
        }

        let rgba = self.cible.relire(&etat.device, &etat.queue);

        // Ce que la fenêtre montre à cette image, gardé pour la suivante : si
        // le pas d'après traverse, c'est à cela qu'on comparera.
        let apercu = self
            .vue3d
            .apercu_pret()
            .then(|| self.vue3d.relire_coulisses(&etat.device, &etat.queue));
        bobine.mesurer(&rgba, apercu);

        bobine.poser(&rgba);
        bobine.noter(self, evenement);
        if bobine.image % 20 == 0 {
            println!("  image {} sur {}", bobine.image, bobine.images());
        }
        self.bobine_ancre = Some(bobine);
        ctx.request_repaint();
    }

    /// `--vue <fichier.jpg>` : une image, puis on quitte.
    ///
    /// Le mode film sait déjà faire tout cela, mais seulement le long de sa
    /// trajectoire de coin. Ce qu'on veut ici est plus bête et plus utile : une
    /// preuve qu'un état donné — la salle, le cadre du portail — s'affiche
    /// vraiment, sans avoir à le décrire de mémoire. D28 le dit sans détour :
    /// les cinq fois où ce banc a réfuté quelque chose, il a fallu regarder
    /// l'écran.
    fn tirer_le_portrait(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let Some(fichier) = self.portrait.clone() else { return };
        let etat = frame.wgpu_render_state().expect("moteur wgpu").clone();
        self.cible.ajuster(
            &etat.device,
            &mut etat.renderer.write(),
            (film::LARGEUR, film::HAUTEUR),
        );

        // Deux passes, comme le film : la première génère, la seconde dessine.
        self.vise = None;
        for _ in 0..2 {
            let mut encodeur = etat.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("portrait") },
            );
            self.peindre(&etat.device, &etat.queue, &mut encodeur);
            etat.queue.submit(Some(encodeur.finish()));
        }

        let rgba = if self.coulisses {
            self.vue3d.relire_coulisses(&etat.device, &etat.queue)
        } else {
            self.cible.relire(&etat.device, &etat.queue)
        };
        let rgb: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let mut sortie = std::io::BufWriter::new(
            std::fs::File::create(&fichier).expect("fichier d'image"),
        );
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut sortie, 88)
            .encode(&rgb, film::LARGEUR, film::HAUTEUR, image::ExtendedColorType::Rgb8)
            .expect("encodage JPEG");
        println!("image écrite : {fichier}");
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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

/// `--ou <lieu>`, `--teinte`, `--ras`, `--ancre` et `--poche` : de quoi rejouer
/// exactement la même vue d'une fois sur l'autre, sans passer par la souris.
fn depart(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--teinte") {
        app.reglages.teinte_chunks = 1.0;
    }
    if let Some(i) = args.iter().position(|a| a == "--film") {
        let dossier = args.get(i + 1).cloned().unwrap_or_else(|| "film".into());
        let mut bobine = film::Film::nouveau(dossier);
        bobine.apex = args.iter().any(|a| a == "--apex");
        if let Some(blocs) = args
            .iter()
            .position(|a| a == "--pas")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<f32>().ok())
        {
            bobine.pas_constant(blocs);
        }
        app.cam = bobine.debut(&app.gen);
        app.film = Some(bobine);
        app.reglages.budget = 4096;
        app.reglages.distance_rendu = 9;
        app.reglages.teinte_chunks = 0.45;
    }
    // L'ancre, dès la première image : `--ancre` pose le portail devant soi,
    // `--poche` le pose et le franchit aussitôt.
    if args.iter().any(|a| a == "--poche") || args.iter().any(|a| a == "--poche-retour") {
        app.amorce = Some(true);
    } else if args.iter().any(|a| a == "--ancre") {
        app.amorce = Some(false);
    }
    // `--poche-retour` : dans la poche, tourné vers le portail de sortie.
    // C'est la seule vue qu'on ne peut pas atteindre autrement sans marcher.
    app.demi_tour = args.iter().any(|a| a == "--poche-retour");
    // Le film de l'ancre : une traversée complète, du présent au passé et
    // retour. Il part de la prairie d'apparition, à hauteur d'homme et à plat —
    // marcher penché ferait descendre la caméra sous le portail qu'elle vient
    // de poser, et le film raterait ce qu'il est censé montrer.
    if let Some(i) = args.iter().position(|a| a == "--film-ancre") {
        let dossier = args.get(i + 1).cloned().unwrap_or_else(|| "ancre".into());
        app.aller_apparition();
        let sol = app.gen.hauteur(
            app.cam.face,
            app.cam.position.x as i32,
            app.cam.position.y as i32,
        );
        app.cam.position.z = sol.max(monde::NIVEAU_MER as f32) + 14.0;
        // Le regard est **horizontal**, et c'est structurel, pas esthétique.
        // La caméra avance dans la direction où elle regarde ; un tangage même
        // léger la fait descendre à chaque pas. Il se transmet à la poche par
        // la traversée, et au bout d'une trentaine de pas la caméra est passée
        // sous l'ouverture — le chemin du retour manque alors le portail, non
        // parce qu'il vise mal, mais parce qu'il est trop bas.
        app.cam.tangage = 0.0;
        app.cam.poser_cap(0.6);
        app.bobine_ancre = Some(film_ancre::FilmAncre::nouveau(dossier));
        app.reglages.budget = 4096;
        app.reglages.distance_rendu = 9;
    }
    if let Some(i) = args.iter().position(|a| a == "--vue") {
        app.portrait = Some(args.get(i + 1).cloned().unwrap_or_else(|| "vue.jpg".into()));
        app.reglages.budget = 4096;
    }
    app.coulisses = args.iter().any(|a| a == "--coulisses");
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
