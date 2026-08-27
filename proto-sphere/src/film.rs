//! Le mode film : `proto-sphere --film <dossier>`.
//!
//! Une trajectoire scriptée qui traverse un coin du cube, exportée image par
//! image. Une capture d'écran ne montre pas si un franchissement est doux —
//! seule une suite d'images le montre, et c'est la seule chose que `--diag` ne
//! sait pas dire.
//!
//! La trajectoire passe **à côté du coin**, en diagonale : le chemin quitte
//! une face par une arête puis une seconde, à une douzaine de blocs du point
//! où trois faces se rejoignent. C'est le pire endroit du monde, donc le seul
//! qui vaille d'être filmé.
//!
//! **À côté, et pas au travers**, et le détail compte. Viser l'apex
//! exactement, c'est faire sortir `u` et `v` de la face à la même image : le
//! point de sortie n'est plus déterminé par la géométrie mais par l'ordre dans
//! lequel le repliement les résout. C'est un cas dégénéré, de mesure nulle, et
//! le filmer reviendrait à filmer une convention plutôt qu'un monde.

use crate::cube::{FACE, NOMS, RAYON, point_sphere, replier_bloc};
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

/// De combien le trajet évite l'apex, en blocs. Assez pour que les deux
/// arêtes soient franchies à des instants distincts, assez peu pour qu'on
/// passe bel et bien au coin.
const EVITEMENT: f32 = 12.0;

pub struct Film {
    pub image: usize,
    pub dossier: String,
    /// Viser le point triple exactement, au lieu de le longer.
    pub apex: bool,
    pub journal: Vec<Etape>,
    precedente: Option<glam::Vec3>,
    /// Longueur de chaque pas. Elle n'est pas constante : voir [`cadence`].
    pas: Vec<f32>,
    parcouru: f32,
}

pub struct Etape {
    pub face: u8,
    pub u: i32,
    pub v: i32,
    pub latitude: f64,
    pub franchissement: bool,
    pub distance: f32,
    /// Rotation de la visée depuis l'image précédente, en degrés. C'est la
    /// grandeur dont on discute : autant la mesurer plutôt que la commenter.
    pub rotation: f64,
}

impl Film {
    pub fn nouveau(dossier: String) -> Self {
        std::fs::create_dir_all(&dossier).expect("dossier du film");
        Self {
            image: 0,
            dossier,
            apex: false,
            journal: Vec::new(),
            precedente: None,
            pas: cadence(),
            parcouru: 0.0,
        }
    }

    /// Pose la caméra au début du trajet.
    ///
    /// Le coin est choisi, pas décrété : sur huit, la plupart tombent en pleine
    /// mer, et une étendue d'eau ne montre rien. On prend celui qui a le plus
    /// de terres autour.
    ///
    /// Le départ, lui, se construit **en remontant le trajet à l'envers**. La
    /// marche suit désormais une géodésique et non une ligne de grille : une
    /// distance comptée en coordonnées ne dit plus combien d'images il faudra
    /// pour arriver, et le coin tombait au sixième du film au lieu du milieu.
    /// On part donc de la cible, on s'en éloigne du recul voulu, et on fait
    /// demi-tour.
    pub fn debut(&self, gen: &Generateur) -> Camera {
        let (face, cu, cv) = meilleur_coin(gen);
        let su = if cu == 0 { 1.0f32 } else { -1.0 };
        let sv = if cv == 0 { 1.0f32 } else { -1.0 };

        // La cible : le coin lui-même, ou un point de l'arête à `EVITEMENT`
        // blocs de lui. Le trajet, qui est une géodésique, passe alors à cette
        // distance du point triple.
        let ecart = if self.apex { 0.0 } else { EVITEMENT };

        let mut cam = Camera {
            face,
            position: Vec3::new(
                cu as f32 - su * 1.0,
                cv as f32 + sv * (ecart + 1.0),
                (NIVEAU_MER + 48) as f32,
            ),
            regard: Vec3::X,
            tangage: -0.28,
        };

        // Dos au coin, vers le centre de la face, puis on recule.
        let centre = Vec3::from_array(
            crate::cube::direction(face, FACE as f64 / 2.0, FACE as f64 / 2.0)
                .map(|x| x as f32),
        );
        cam.viser_point(centre);

        let recul = TRAJET * 0.45;
        let pas = 2.0f32;
        for _ in 0..(recul / pas) as usize {
            let (du, dv) = cam.vers_coordonnees(cam.avant_plat() * pas);
            cam.avancer(Vec3::new(du, dv, 0.0));
        }

        // Demi-tour : le regard vit dans le monde, l'inverser suffit.
        cam.regard = -cam.avant_plat();
        cam.position.z = altitude(gen, &cam);
        cam
    }

    /// Avance d'une image. Rend `false` quand le film est fini.
    pub fn avancer(&mut self, gen: &Generateur, cam: &mut Camera) -> bool {
        if self.image >= IMAGES {
            return false;
        }

        let pas = self.pas[self.image];
        self.parcouru += pas;

        // On avance dans le monde, puis on redresse vers les coordonnées.
        let (du, dv) = cam.vers_coordonnees(cam.avant_plat() * pas);
        let franchissement = cam.avancer(Vec3::new(du, dv, 0.0)) > 0;

        // L'altitude suit le relief, mais de loin : un suivi sec ferait
        // tressauter l'image à chaque colline.
        let cible = altitude(gen, cam);
        cam.position.z += (cible - cam.position.z) * 0.18;

        let (f, u, v, _) = replier_bloc(
            cam.face,
            cam.position.x.floor() as i32,
            cam.position.y.floor() as i32,
        );
        // La visée, en 3D : c'est elle qui doit varier doucement, et c'est
        // elle que le lecteur du film pourra regarder image par image.
        let (_, visee, _) = cam.repere_3d(RAYON);
        let rotation = self
            .precedente
            .map(|p| (p.dot(visee) as f64).clamp(-1.0, 1.0).acos().to_degrees())
            .unwrap_or(0.0);
        self.precedente = Some(visee);

        self.journal.push(Etape {
            face: f,
            u,
            v,
            latitude: point_sphere(f, u, v)[2].asin().to_degrees(),
            franchissement,
            distance: self.parcouru,
            rotation,
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
                r#"  {{"face":"{}","u":{},"v":{},"latitude":{:.1},"franchissement":{},"rotation":{:.2},"distance":{:.0}}}{}"#,
                NOMS[e.face as usize],
                e.u,
                e.v,
                e.latitude,
                e.franchissement,
                e.rotation,
                e.distance,
                virgule
            )
            .unwrap();
        }
        writeln!(f, "]").unwrap();
    }
}

/// La longueur de chaque pas, en blocs.
///
/// Elle n'est pas constante, et ce n'est pas de la mise en scène. Près du coin,
/// le cisaillement de la grille fait tourner la visée vite — c'est réel, et
/// documenté par D27. Un pas uniforme de cinq blocs échantillonne ce virage une
/// seule fois et le donne à voir comme un saut, ce qu'il n'est pas. On ralentit
/// donc là où il se passe quelque chose : c'est la seule façon de filmer une
/// chose rapide sans la déformer.
fn cadence() -> Vec<f32> {
    let centre = IMAGES as f32 * 0.47;
    let largeur = IMAGES as f32 * 0.16;

    let mut poids: Vec<f32> = (0..IMAGES)
        .map(|i| {
            let x = (i as f32 - centre) / largeur;
            0.10 + 0.90 * (1.0 - (-x * x).exp())
        })
        .collect();

    let somme: f32 = poids.iter().sum();
    for p in &mut poids {
        *p *= TRAJET / somme;
    }
    poids
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
