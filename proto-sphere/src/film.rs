//! Le mode film : `proto-sphere --film <dossier>`.
//!
//! Une trajectoire scriptée qui traverse un coin du cube, exportée image par
//! image. Une capture d'écran ne montre pas si un franchissement est doux —
//! seule une suite d'images le montre, et c'est la seule chose que `--diag` ne
//! sait pas dire.
//!
//! La trajectoire passe **par le coin lui-même**, en diagonale : le chemin
//! quitte une face par une arête puis une seconde, à quelques blocs du point
//! où trois faces se rejoignent. C'est le pire endroit du monde, donc le seul
//! qui vaille d'être filmé.

use crate::cube::{FACE, NOMS, point_sphere, replier_bloc};
use crate::monde::{Generateur, NIVEAU_MER};
use crate::vue3d::Camera;
use glam::Vec3;
use std::io::Write;

/// Côté de l'image. La largeur est multiple de 64 pour que la relecture du GPU
/// tombe sur des lignes de 256 octets.
pub const LARGEUR: u32 = 640;
pub const HAUTEUR: u32 = 360;
pub const IMAGES: usize = 120;

/// Longueur du trajet, en blocs. Il commence avant le coin et finit après.
const TRAJET: f32 = 620.0;

pub struct Film {
    pub image: usize,
    pub dossier: String,
    pub journal: Vec<Etape>,
}

pub struct Etape {
    pub face: u8,
    pub u: i32,
    pub v: i32,
    pub latitude: f64,
    pub franchissement: bool,
}

impl Film {
    pub fn nouveau(dossier: String) -> Self {
        std::fs::create_dir_all(&dossier).expect("dossier du film");
        Self { image: 0, dossier, journal: Vec::new() }
    }

    /// Pose la caméra au début du trajet, en diagonale vers un coin.
    ///
    /// Le coin est choisi, pas décrété : sur huit, la plupart tombent en pleine
    /// mer, et une étendue d'eau ne montre rien. On prend celui qui a le plus
    /// de terres autour.
    pub fn debut(&self, gen: &Generateur) -> Camera {
        let (face, cu, cv) = meilleur_coin(gen);
        let recul = TRAJET * 0.45 / 2.0f32.sqrt();
        let su = if cu == 0 { 1.0f32 } else { -1.0 };
        let sv = if cv == 0 { 1.0f32 } else { -1.0 };

        let mut cam = Camera {
            face,
            position: Vec3::new(cu as f32 + su * recul, cv as f32 + sv * recul, 0.0),
            // Cap constant vers le coin. Le repliement le fera tourner d'un
            // quart de tour au franchissement ; c'est justement ce qu'on filme.
            lacet: (-sv).atan2(-su),
            tangage: -0.28,
        };
        cam.position.z = altitude(gen, &cam);
        cam
    }

    /// Avance d'une image. Rend `false` quand le film est fini.
    pub fn avancer(&mut self, gen: &Generateur, cam: &mut Camera) -> bool {
        if self.image >= IMAGES {
            return false;
        }

        let pas = TRAJET / IMAGES as f32;
        let cap = Vec3::new(cam.lacet.cos(), cam.lacet.sin(), 0.0);
        cam.position += cap * pas;
        let franchissement = cam.replier();

        // L'altitude suit le relief, mais de loin : un suivi sec ferait
        // tressauter l'image à chaque colline.
        let cible = altitude(gen, cam);
        cam.position.z += (cible - cam.position.z) * 0.18;

        let (f, u, v, _) = replier_bloc(
            cam.face,
            cam.position.x.floor() as i32,
            cam.position.y.floor() as i32,
        );
        self.journal.push(Etape {
            face: f,
            u,
            v,
            latitude: point_sphere(f, u, v)[2].asin().to_degrees(),
            franchissement,
        });
        self.image += 1;
        true
    }

    /// Enregistre l'image courante.
    pub fn poser(&self, octets: &[u8]) {
        let chemin = format!("{}/image{:03}.jpg", self.dossier, self.image);
        let fichier = std::fs::File::create(&chemin).expect("image du film");
        let mut sortie = std::io::BufWriter::new(fichier);

        let rgb: Vec<u8> = octets
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();

        let mut codeur =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut sortie, 82);
        codeur
            .encode(&rgb, LARGEUR, HAUTEUR, image::ExtendedColorType::Rgb8)
            .expect("encodage JPEG");
    }

    /// Le journal du trajet, en JSON, pour que la page qui montre le film
    /// puisse dire où l'on se trouve à chaque image.
    pub fn ecrire_journal(&self) {
        let chemin = format!("{}/trajet.json", self.dossier);
        let mut f = std::fs::File::create(chemin).expect("journal du film");
        writeln!(f, "[").unwrap();
        for (i, e) in self.journal.iter().enumerate() {
            let virgule = if i + 1 == self.journal.len() { "" } else { "," };
            writeln!(
                f,
                r#"  {{"face":"{}","u":{},"v":{},"latitude":{:.1},"franchissement":{}}}{}"#,
                NOMS[e.face as usize],
                e.u,
                e.v,
                e.latitude,
                e.franchissement,
                virgule
            )
            .unwrap();
        }
        writeln!(f, "]").unwrap();
    }
}

/// Le coin du cube le mieux pourvu en terres, parmi les huit.
fn meilleur_coin(gen: &Generateur) -> (u8, i32, i32) {
    let (mut meilleur, mut score) = ((1u8, FACE, 0), -1i32);
    for face in 0..6u8 {
        for (cu, cv) in [(0, 0), (FACE, 0), (0, FACE), (FACE, FACE)] {
            let mut terres = 0;
            for du in (-360..=360).step_by(45) {
                for dv in (-360..=360).step_by(45) {
                    if gen.colonne(face, cu + du, cv + dv).0 > NIVEAU_MER + 3 {
                        terres += 1;
                    }
                }
            }
            if terres > score {
                score = terres;
                meilleur = (face, cu, cv);
            }
        }
    }
    println!(
        "coin retenu : face {}, ({}, {}) — {} points de terre sur 289",
        NOMS[meilleur.0 as usize], meilleur.1, meilleur.2, score
    );
    meilleur
}

fn altitude(gen: &Generateur, cam: &Camera) -> f32 {
    let sol = gen.hauteur(cam.face, cam.position.x as i32, cam.position.y as i32);
    sol.max(NIVEAU_MER as f32) + 48.0
}
