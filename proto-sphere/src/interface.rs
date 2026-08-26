//! Le menu de debug.
//!
//! Il n'est pas décoratif : chaque réglage correspond à une chose que D27
//! affirme et qu'on veut pouvoir mettre en défaut d'un clic — la courbure comme
//! pur fait de rendu, la position brute face à la position repliée, et les deux
//! endroits du monde où la topologie se voit.

use crate::monde::{BLOCS_H, BLOCS_W, biome, plier_bloc};
use crate::{App, Vue};

impl App {
    pub(crate) fn menu(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading("D27 — banc d'essai");
        ui.label(
            egui::RichText::new(
                "Le monde se calcule à plat et se joue comme une planète.",
            )
            .italics()
            .weak(),
        );
        ui.separator();

        // --- Vue ------------------------------------------------------------
        ui.horizontal(|ui| {
            ui.label("Vue");
            ui.selectable_value(&mut self.vue, Vue::Trois, "3D — l'illusion");
            ui.selectable_value(&mut self.vue, Vue::Deux, "2D — la topologie");
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
                "Rien de ce réglage ne redescend dans la logique : le raycast \
                 vise toujours à plat.",
            )
            .weak()
            .small(),
        );
        ui.separator();

        // --- Rendu ----------------------------------------------------------
        ui.strong("Rendu");
        ui.add(
            egui::Slider::new(&mut self.reglages.distance_rendu, 2..=16)
                .text("distance (chunks)"),
        );
        ui.add(egui::Slider::new(&mut self.reglages.champ, 40.0..=110.0).text("champ (°)"));
        ui.add(egui::Slider::new(&mut self.vitesse, 4.0..=120.0).text("vitesse"));
        ui.checkbox(
            &mut self.reglages.teinte_chunks,
            "Teinter les chunks (les repliés en rouge)",
        );
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
                "{} × {} chunks · {} × {} blocs",
                crate::monde::MONDE_W,
                crate::monde::MONDE_H,
                BLOCS_W,
                BLOCS_H
            ))
            .weak()
            .small(),
        );

        ui.add_space(4.0);
        ui.label("Aller voir :");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Couture est-ouest").clicked() {
                self.aller(0.0, BLOCS_H as f32 * 0.5);
            }
            if ui.button("Pôle nord").clicked() {
                self.aller(BLOCS_W as f32 * 0.25, 3.0);
            }
            if ui.button("Pôle sud").clicked() {
                self.aller(BLOCS_W as f32 * 0.25, BLOCS_H as f32 - 3.0);
            }
            if ui.button("Prairie de départ").clicked() {
                self.aller_apparition();
            }
        });
        ui.separator();

        // --- Position -------------------------------------------------------
        ui.strong("Position");
        let p = self.cam.position;
        let (wx, wy, plis) = plier_bloc(p.x.floor() as i32, p.y.floor() as i32);
        egui::Grid::new("position").num_columns(2).show(ui, |ui| {
            ui.label("brute");
            ui.monospace(format!("{:.1}  {:.1}  {:.1}", p.x, p.y, p.z));
            ui.end_row();

            ui.label("repliée");
            ui.monospace(format!("{}  {}", wx, wy));
            ui.end_row();

            ui.label("chunk");
            ui.monospace(format!(
                "{}  {}",
                wx.div_euclid(crate::monde::TAILLE_CHUNK),
                wy.div_euclid(crate::monde::TAILLE_CHUNK)
            ));
            ui.end_row();

            ui.label("biome");
            ui.monospace(biome(wy).nom());
            ui.end_row();

            ui.label("pôles franchis");
            ui.monospace(format!("{}", self.plis));
            ui.end_row();

            ui.label("bloc visé");
            ui.monospace(match self.vise {
                Some(b) => format!("{}  {}  {}", b[0], b[1], b[2]),
                None => "—".to_string(),
            });
            ui.end_row();
        });
        if plis > 0 {
            ui.colored_label(
                egui::Color32::from_rgb(240, 170, 60),
                "Position hors du monde canonique — repliement en cours.",
            );
        }
        ui.separator();

        // --- Compteurs ------------------------------------------------------
        let (dessines, memoire) = self.chunks();
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
