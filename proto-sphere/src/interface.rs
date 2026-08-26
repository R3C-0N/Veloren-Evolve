//! Le menu de debug.
//!
//! Il n'est pas décoratif : chaque réglage correspond à une chose que D27
//! affirme et qu'on veut pouvoir mettre en défaut d'un clic — la position brute
//! face à la position repliée, et les trois endroits du monde où la topologie
//! se voit : une arête, un coin, une calotte.

use crate::cube::{FACE, NOMS, RAYON, replier_bloc};
use crate::monde::{TAILLE_CHUNK, biome_de};
use crate::{COIN, MILIEU_ARETE, Vue};
use crate::App;

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
            ui.selectable_value(&mut self.vue, Vue::Trois, "3D — la planète");
            ui.selectable_value(&mut self.vue, Vue::Deux, "2D — le patron");
        });
        ui.label(egui::RichText::new("F1 pour basculer").weak().small());
        ui.separator();

        // --- La planète ------------------------------------------------------
        ui.strong("La planète");
        egui::Grid::new("planete").num_columns(2).show(ui, |ui| {
            ui.label("rayon");
            ui.monospace(format!("{:.0} blocs", RAYON));
            ui.end_row();
            ui.label("tour du monde");
            ui.monospace(format!("{} blocs", 4 * FACE));
            ui.end_row();
            ui.label("faces");
            ui.monospace(format!("6 × {} chunks", crate::cube::FACE_CHUNKS));
            ui.end_row();
        });
        ui.label(
            egui::RichText::new(
                "La rondeur n'est plus un réglage : chaque chunk est dessiné à \
                 sa vraie place sur la sphère. C'est ce qui supprime les fausses \
                 adjacences des coins — et c'est pour ça que le rayon vaut la \
                 taille du monde, et rien d'autre.",
            )
            .weak()
            .small(),
        );
        ui.add(
            egui::Slider::new(&mut self.reglages.aplatissement, 1.0..=12.0)
                .text("aplatir (×)")
                .logarithmic(true),
        );
        ui.label(
            egui::RichText::new(
                "Réglage de debug : gonfler le rayon aplatit l'horizon, au prix \
                 d'un mensonge sur la taille des blocs. Le seul endroit du \
                 programme qui s'y autorise.",
            )
            .weak()
            .small(),
        );
        ui.separator();

        // --- Rendu ----------------------------------------------------------
        ui.strong("Rendu");
        ui.add(
            egui::Slider::new(&mut self.reglages.distance_rendu, 2..=20).text("distance (chunks)"),
        );
        ui.add(egui::Slider::new(&mut self.reglages.champ, 40.0..=110.0).text("champ (°)"));
        ui.add(egui::Slider::new(&mut self.vitesse, 4.0..=400.0).text("vitesse").logarithmic(true));
        ui.checkbox(&mut self.reglages.teinte_chunks, "Teinter les chunks (une teinte par face)");
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
            ui.monospace(biome_de(latitude.abs() / std::f64::consts::FRAC_PI_2).nom());
            ui.end_row();

            ui.label("arêtes franchies");
            ui.monospace(format!("{}", self.aretes));
            ui.end_row();

            ui.label("bloc visé");
            ui.monospace(match self.vise {
                Some((f, u, v, z)) => format!("{}  {}  {}  ·  {}", u, v, z, NOMS[f as usize]),
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
        let (dessines, memoire) = self.compteurs();
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
