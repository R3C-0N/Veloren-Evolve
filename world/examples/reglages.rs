//! Régler le climat à la main, et voir les biomes bouger (Q27).
//!
//! Une vingtaine de nombres décident de la carte. Tant qu'ils étaient des
//! `const`, en voir l'effet demandait de recompiler *et* de régénérer un monde
//! — plusieurs minutes pour un chiffre qu'on voulait bouger de deux centièmes,
//! et c'est la seule raison pour laquelle Q27 est restée ouverte depuis D39.
//!
//! Le prix de la génération est presque entièrement dans l'érosion, et
//! l'érosion ne lit jamais `temp`. On la paie donc une fois, puis on refait la
//! carte des biomes à chaque mouvement de curseur.
//!
//! **La loi n'est pas ici.** `Etude::parts` appelle `SimChunk::generate` et
//! `get_biome_avec`, les mêmes que le jeu. Une fenêtre qui refarait la loi de
//! son côté réglerait un monde et on en jouerait un autre — c'est l'alibi
//! contre lequel D28 met en garde.
//!
//! ```bash
//! cargo run --release --example reglages -- --x-lg 9
//! ```

use common::{resources::MapKind, terrain::BiomeKind};
use eframe::egui;
use veloren_world::sim::{Etude, FileOpts, GenOpts, Masque, REGLAGES, Reglages};

fn arg(nom: &str) -> Option<String> {
    std::env::args().skip_while(|a| a != nom).nth(1)
}

struct App {
    etude: Etude,
    reglages: Reglages,
    /// Les parts sous les réglages du jeu, pour que chaque ligne porte son
    /// écart. Un chiffre seul ne dit pas si on a amélioré quoi que ce soit.
    reference: Vec<(BiomeKind, u32)>,
    parts: Vec<(BiomeKind, u32)>,
    total: u32,
    duree_ms: f32,
}

impl App {
    fn recalculer(&mut self) {
        let t = std::time::Instant::now();
        self.parts = self.etude.parts(&self.reglages);
        self.duree_ms = t.elapsed().as_secs_f32() * 1000.0;
        self.total = self.parts.iter().map(|&(_, n)| n).sum();
    }

    fn part_de_reference(&self, biome: BiomeKind) -> u32 {
        self.reference
            .iter()
            .find(|&&(b, _)| b == biome)
            .map_or(0, |&(_, n)| n)
    }
}

/// Un groupe de curseurs, replié ou non.
fn groupe(ui: &mut egui::Ui, titre: &str, contenu: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    let mut bouge = false;
    egui::CollapsingHeader::new(titre)
        .default_open(true)
        .show(ui, |ui| bouge = contenu(ui));
    bouge
}

fn curseur(ui: &mut egui::Ui, v: &mut f32, min: f32, max: f32, nom: &str) -> bool {
    ui.add(egui::Slider::new(v, min..=max).text(nom)).changed()
}

fn curseur64(ui: &mut egui::Ui, v: &mut f64, min: f64, max: f64, nom: &str) -> bool {
    ui.add(egui::Slider::new(v, min..=max).text(nom)).changed()
}

fn masque(ui: &mut egui::Ui, m: &mut Masque, nom: &str) -> bool {
    let mut bouge = false;
    ui.label(nom);
    bouge |= curseur64(ui, &mut m.echelle, 400.0, 6000.0, "échelle, blocs");
    bouge |= curseur64(ui, &mut m.bas, -0.6, 0.8, "seuil bas");
    bouge |= curseur64(ui, &mut m.haut, -0.5, 0.9, "seuil haut");
    bouge
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let mut bouge = false;

        egui::SidePanel::left("reglages")
            .default_width(340.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    bouge |= groupe(ui, "Bandes de latitude", |ui| {
                        let b = &mut self.reglages.bandes;
                        let mut v = false;
                        v |= curseur(ui, &mut b.poids[0], 0.0, 6.0, "poids latitude");
                        v |= curseur(ui, &mut b.poids[1], 0.0, 6.0, "poids altitude");
                        v |= curseur(ui, &mut b.poids[2], 0.0, 6.0, "poids bruit");
                        v |= curseur64(ui, &mut b.onde, 0.0, 0.4, "ondulation");
                        v
                    });

                    bouge |= groupe(ui, "Étagement", |ui| {
                        let e = &mut self.reglages.etagement;
                        let mut g = e.gradient * 1000.0;
                        let mut v = curseur(ui, &mut g, 0.0, 4.0, "gradient / 1000 blocs");
                        e.gradient = g / 1000.0;
                        v |= curseur(ui, &mut e.halo, 0.0, 3.0, "halo géothermique");
                        v
                    });

                    bouge |= groupe(ui, "Évaporation", |ui| {
                        let e = &mut self.reglages.evaporation;
                        let mut v = curseur(ui, &mut e.seuil, 0.0, 1.0, "seuil");
                        v |= curseur(ui, &mut e.pente, 0.1, 2.0, "pente");
                        v |= curseur(ui, &mut e.plancher, 0.0, 1.0, "plancher");
                        ui.small(
                            "Sans plancher, le facteur atteint zéro et aucune humidité brute \
                             ne peut plus satisfaire une jungle.",
                        );
                        v
                    });

                    bouge |= groupe(ui, "Calottes", |ui| {
                        let c = &mut self.reglages.calottes;
                        let mut v = curseur(ui, &mut c.banquise, 0.71, 0.99, "banquise, sin(lat)");
                        v |= curseur(ui, &mut c.barriere, 0.71, 0.99, "barrière, sin(lat)");
                        ui.small("Sous 0,707, le front enjambe les coutures de la face polaire.");
                        v |= curseur(ui, &mut c.abysse, 20.0, 300.0, "abysse, blocs d'eau");
                        v
                    });

                    bouge |= groupe(ui, "Masques de région", |ui| {
                        let mut v = masque(ui, &mut self.reglages.volcan, "Volcanique");
                        ui.separator();
                        v |= masque(ui, &mut self.reglages.arcane, "Magie instable");
                        ui.separator();
                        v |= masque(ui, &mut self.reglages.miasme, "Miasme");
                        v
                    });

                    bouge |= groupe(ui, "Seuils de biome", |ui| {
                        let s = &mut self.reglages.seuils;
                        let mut v = curseur(ui, &mut s.desert_temp, 0.0, 1.5, "désert : temp");
                        v |= curseur(ui, &mut s.desert_humidite, 0.0, 0.6, "désert : humidité");
                        ui.separator();
                        v |= curseur(ui, &mut s.jungle_arbres, 0.0, 1.0, "jungle : arbres");
                        v |= curseur(ui, &mut s.jungle_humidite, 0.0, 1.0, "jungle : humidité");
                        v |= curseur(ui, &mut s.jungle_temp, 0.0, 1.0, "jungle : temp");
                        ui.separator();
                        v |= curseur(ui, &mut s.foret_arbres, 0.0, 1.0, "forêt : arbres");
                        v |= curseur(ui, &mut s.montagne_alt, 200.0, 1200.0, "montagne : altitude");
                        v |= curseur(ui, &mut s.montagne_chaos, 0.0, 1.0, "montagne : chaos");
                        v |= curseur(ui, &mut s.montagne_arbres, 0.0, 1.0, "montagne : arbres");
                        ui.separator();
                        v |= curseur(ui, &mut s.fond_humidite, 0.0, 1.0, "marais : humidité");
                        v |= curseur(ui, &mut s.miasme_humidite, 0.0, 1.0, "miasme : humidité");
                        v |= curseur(ui, &mut s.miasme_hauteur, 0.0, 800.0, "miasme : hauteur");
                        v
                    });

                    ui.separator();
                    if ui.button("Revenir aux réglages du jeu").clicked() {
                        self.reglages = REGLAGES;
                        bouge = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Parts des biomes");
            ui.label(format!(
                "{} cases vivantes · recalcul {:.0} ms",
                self.total, self.duree_ms
            ));
            ui.separator();

            egui::Grid::new("parts").striped(true).show(ui, |ui| {
                ui.label("biome");
                ui.label("cases");
                ui.label("part");
                ui.label("écart");
                ui.end_row();
                for &(biome, n) in &self.parts {
                    let reference = self.part_de_reference(biome);
                    let ecart = n as i64 - reference as i64;
                    ui.label(format!("{biome:?}"));
                    ui.label(format!("{n}"));
                    ui.label(format!("{:.2} %", 100.0 * n as f64 / self.total as f64));
                    // Un biome absent d'un cote ou de l'autre est ce qu'on
                    // cherche le plus souvent : on le dit en toutes lettres.
                    ui.colored_label(
                        if ecart > 0 {
                            egui::Color32::from_rgb(120, 200, 120)
                        } else if ecart < 0 {
                            egui::Color32::from_rgb(210, 130, 130)
                        } else {
                            egui::Color32::GRAY
                        },
                        if reference == 0 {
                            "apparu".to_string()
                        } else {
                            format!("{ecart:+}")
                        },
                    );
                    ui.end_row();
                }
                for &(biome, n) in &self.reference {
                    if !self.parts.iter().any(|&(b, _)| b == biome) {
                        ui.label(format!("{biome:?}"));
                        ui.label("0");
                        ui.label("0,00 %");
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 130, 130),
                            format!("disparu (-{n})"),
                        );
                        ui.end_row();
                    }
                }
            });

            ui.separator();
            ui.collapsing("Reporter dans reglages.rs", |ui| {
                let mut txt = format!("{:#?}", self.reglages);
                ui.add(
                    egui::TextEdit::multiline(&mut txt)
                        .code_editor()
                        .desired_width(f32::INFINITY),
                );
            });
        });

        if bouge {
            self.recalculer();
        }
    }
}

fn main() -> eframe::Result {
    let x_lg: u32 = arg("--x-lg").and_then(|v| v.parse().ok()).unwrap_or(9);
    let graine: u32 = arg("--graine").and_then(|v| v.parse().ok()).unwrap_or(42);

    // Une taille, et on s'y tient : les echelles de masque sont relatives au
    // monde, donc regler a 7 pour jouer a 9 ne veut rien dire.
    println!("génération du monde (graine {graine}, --x-lg {x_lg})… une à deux minutes");
    let pool = rayon::ThreadPoolBuilder::new().build().unwrap();
    let mut etude = Etude::nouvelle(
        graine,
        FileOpts::Generate(GenOpts {
            x_lg,
            y_lg: x_lg,
            map_kind: MapKind::Cube,
            ..GenOpts::default()
        }),
        &pool,
    );

    let reference = etude.parts(&REGLAGES);
    let mut app = App {
        etude,
        reglages: REGLAGES,
        parts: reference.clone(),
        reference,
        total: 0,
        duree_ms: 0.0,
    };
    app.total = app.parts.iter().map(|&(_, n)| n).sum();

    eframe::run_native(
        "Réglages des biomes",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(app))),
    )
}
