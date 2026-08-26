//! Le menu de debug.
//!
//! Il n'est pas décoratif : chaque réglage correspond à une chose que D27
//! affirme et qu'on veut pouvoir mettre en défaut d'un clic — la courbure comme
//! pur fait de rendu, la position brute face à la position repliée, et les trois
//! endroits du monde où la topologie se voit : une arête, un coin, une calotte.

use crate::cube::{FACE, NOMS, replier_bloc};
use crate::monde::{TAILLE_CHUNK, biome_de};
use crate::{App, COIN, MILIEU_ARETE, Vue};

impl App {
    pub(crate) fn menu(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading("D27 — banc d'essai");
        ui.label(
            egui::RichText::new("Le monde est le patron d'un cube, joué comme une planète.")
                .italics()
                .weak(),
        );
        ui.separator();

        // --- Vue ------------------------------------------------------------
        ui.horizontal(|ui| {
            ui.label("Vue");
            ui.selectable_value(&mut self.vue, Vue::Trois, "3D — l'illusion");
            ui.selectable_value(&mut self.vue, Vue::Deux, "2D — le patron");
        });
        ui.label(egui::RichText::new("F1 pour basculer").weak().small());
        ui.separator();

        // --- Courbure -------------------------------------------------------
        ui.strong("Courbure du rendu");
        let mut plat = self.reglages.rayon_courbure <= 0.0;
        if ui.checkbox(&mut plat, "Plat (rayon infini)").changed() {
            self.reglages.rayon_courbure = if plat { 0.0 } else { 1500.0 };
        }
        ui.add_enabled(
            !plat,
            egui::Slider::new(&mut self.reglages.rayon_courbure, 300.0..=6000.0)
                .text("rayon (blocs)")
                .logarithmic(true),
        );
        ui.label(
            egui::RichText::new(
                "C'est elle qui arrondit les arêtes du cube, sans rien savoir \
                 d'elles. Et rien de ce réglage ne redescend dans la logique : \
                 le raycast vise toujours à plat.",
            )
            .weak()
            .small(),
        );
        ui.separator();

        // --- Le défaut des coins --------------------------------------------
        ui.strong("Le défaut des huit coins");
        ui.checkbox(
            &mut self.reglages.montrer_defaut,
            "Laisser le trou de 90° au lieu de le combler",
        );
        ui.checkbox(
            &mut self.reglages.teinte_chunks,
            "Teinter les chunks (les dupliqués en rouge)",
        );
        ui.label(
            egui::RichText::new(
                "Trois faces se rejoignent à un coin : 270°, quand le plan \
                 déroulé en offre 360°. Ce qui manque est soit un trou, soit une \
                 copie. Il n'y a pas de troisième option.",
            )
            .weak()
            .small(),
        );
        ui.separator();

        // --- Rendu ----------------------------------------------------------
        ui.strong("Rendu");
        ui.add(
            egui::Slider::new(&mut self.reglages.distance_rendu, 2..=16).text("distance (chunks)"),
        );
        ui.add(egui::Slider::new(&mut self.reglages.champ, 40.0..=110.0).text("champ (°)"));
        ui.add(egui::Slider::new(&mut self.vitesse, 4.0..=120.0).text("vitesse"));
        ui.separator();

        // --- Monde ----------------------------------------------------------
        ui.strong("Monde");
        ui.horizontal(|ui| {
            ui.label("graine");
            if ui
                .add(egui::DragValue::new(&mut self.graine).range(0..=9999))
                .changed()
            {
                self.oublier_chunks = true;
                self.refaire_carte = true;
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "6 faces de {} chunks d'arête · tour du monde {} blocs",
                crate::cube::FACE_CHUNKS,
                4 * FACE
            ))
            .weak()
            .small(),
        );

        ui.add_space(4.0);
        ui.label("Aller voir :");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Un coin").clicked() {
                self.aller(COIN.0, COIN.1, COIN.2);
            }
            if ui.button("Une arête").clicked() {
                self.aller(MILIEU_ARETE.0, MILIEU_ARETE.1, MILIEU_ARETE.2);
            }
            if ui.button("Calotte nord").clicked() {
                self.aller(4, FACE / 2, FACE / 2);
            }
            if ui.button("Calotte sud").clicked() {
                self.aller(5, FACE / 2, FACE / 2);
            }
            if ui.button("Prairie de départ").clicked() {
                self.aller_apparition();
            }
        });
        ui.separator();

        // --- Position -------------------------------------------------------
        ui.strong("Position");
        let p = self.cam.position;
        let (fc, u, v, k) = replier_bloc(self.cam.face, p.x.floor() as i32, p.y.floor() as i32);
        let sphere = crate::cube::point_sphere(fc, u, v);
        let latitude = sphere[2].asin();

        egui::Grid::new("position").num_columns(2).show(ui, |ui| {
            ui.label("face");
            ui.monospace(NOMS[self.cam.face as usize]);
            ui.end_row();

            ui.label("brute");
            ui.monospace(format!("{:.1}  {:.1}  {:.1}", p.x, p.y, p.z));
            ui.end_row();

            ui.label("repliée");
            ui.monospace(format!("{}  {}  ·  {}", u, v, NOMS[fc as usize]));
            ui.end_row();

            ui.label("chunk");
            ui.monospace(format!(
                "{}  {}",
                u.div_euclid(TAILLE_CHUNK),
                v.div_euclid(TAILLE_CHUNK)
            ));
            ui.end_row();

            ui.label("latitude");
            ui.monospace(format!("{:.1}°", latitude.to_degrees()));
            ui.end_row();

            ui.label("biome");
            ui.monospace(
                biome_de(latitude.abs() / std::f64::consts::FRAC_PI_2).nom(),
            );
            ui.end_row();

            ui.label("arêtes franchies");
            ui.monospace(format!("{}", self.aretes));
            ui.end_row();

            ui.label("bloc visé");
            ui.monospace(match self.vise {
                Some(b) => format!("{}  {}  {}", b[0], b[1], b[2]),
                None => "—".to_string(),
            });
            ui.end_row();
        });
        if k != 0 || fc != self.cam.face {
            ui.colored_label(
                egui::Color32::from_rgb(240, 170, 60),
                "Position hors de sa face — repliement en cours.",
            );
        }
        ui.separator();

        // --- Compteurs ------------------------------------------------------
        let (dessines, memoire, doublons) = self.compteurs();
        ui.strong("Compteurs");
        egui::Grid::new("compteurs").num_columns(2).show(ui, |ui| {
            ui.label("image");
            ui.monospace(format!("{:.1} ms", self.ms));
            ui.end_row();
            ui.label("chunks dessinés");
            ui.monospace(format!("{}", dessines));
            ui.end_row();
            ui.label("chunks en mémoire");
            ui.monospace(format!("{}", memoire));
            ui.end_row();
            ui.label("chunks dupliqués");
            ui.monospace(format!("{}", doublons));
            ui.end_row();
        });

        ui.separator();
        ui.label(
            egui::RichText::new(
                "Déplacement : ZQSD/WASD · Espace/C · Maj = vite · Ctrl = lent\n\
                 Regard : glisser dans la vue",
            )
            .weak()
            .small(),
        );
    }
}
