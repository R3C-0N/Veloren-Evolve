//! Le terrain : blocs, biomes, altitude.
//!
//! La topologie est dans [`crate::cube`] ; ici on ne fait que la traverser.
//! Tout accès au monde passe par `replier_bloc`, si bien que demander
//! l'altitude d'une case hors de sa face rend l'altitude de la case du cube qui
//! s'y trouve réellement. La continuité aux coutures n'est pas un correctif
//! appliqué après coup : c'est une propriété du seul chemin d'accès qui existe.
//!
//! Le champ de bruit vit sur la sphère circonscrite au cube, échantillonné en
//! 3D. Deux cases voisines du patron, de part et d'autre d'un recollement,
//! tombent sur deux points voisins de la sphère — et les huit coins ne posent
//! aucun problème au bruit, puisque les trois faces y touchent un seul et même
//! point.

use crate::cube::{FACE, point_sphere, replier_bloc};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

pub const TAILLE_CHUNK: i32 = 32;
pub const HAUTEUR_CHUNK: i32 = 128;
pub const NIVEAU_MER: i32 = 40;

// --------------------------------------------------------------------------
// Blocs
// --------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bloc {
    Air,
    Roche,
    Terre,
    Herbe,
    Sable,
    Neige,
    Glace,
    Eau,
}

impl Bloc {
    pub fn plein(self) -> bool { self != Bloc::Air }

    /// Couleur en espace linéaire — la cible de rendu est en sRGB et se charge
    /// de l'encodage.
    pub fn couleur(self) -> [f32; 3] {
        match self {
            Bloc::Air => [0.0, 0.0, 0.0],
            Bloc::Roche => [0.28, 0.28, 0.30],
            Bloc::Terre => [0.24, 0.15, 0.08],
            Bloc::Herbe => [0.16, 0.42, 0.12],
            Bloc::Sable => [0.72, 0.64, 0.36],
            Bloc::Neige => [0.86, 0.88, 0.92],
            Bloc::Glace => [0.62, 0.78, 0.88],
            Bloc::Eau => [0.05, 0.20, 0.45],
        }
    }
}

// --------------------------------------------------------------------------
// Biomes par bande de latitude (D24)
// --------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    Prairie,
    Tempere,
    Neigeux,
    Glacier,
}

impl Biome {
    pub fn nom(self) -> &'static str {
        match self {
            Biome::Prairie => "prairie",
            Biome::Tempere => "tempéré",
            Biome::Neigeux => "neigeux",
            Biome::Glacier => "glacier",
        }
    }

    fn surface(self) -> Bloc {
        match self {
            Biome::Prairie | Biome::Tempere => Bloc::Herbe,
            Biome::Neigeux => Bloc::Neige,
            Biome::Glacier => Bloc::Glace,
        }
    }
}

/// Distance à l'équateur de la bande de prairie : le milieu d'un hémisphère
/// (D24). C'est là que le jeu commence.
pub const LATITUDE_PRAIRIE: f64 = 0.45;

/// Les bandes se prennent de la **latitude vraie**, jamais d'une coordonnée de
/// grille. C'est ce qui fait que les calottes polaires tombent au centre des
/// faces `+Z` et `−Z` au lieu de s'étaler en bandes.
pub fn biome_de(latitude01: f64) -> Biome {
    if latitude01 > 0.86 {
        Biome::Glacier
    } else if latitude01 > 0.66 {
        Biome::Neigeux
    } else if (latitude01 - LATITUDE_PRAIRIE).abs() < 0.13 {
        Biome::Prairie
    } else {
        Biome::Tempere
    }
}

// --------------------------------------------------------------------------
// Génération
// --------------------------------------------------------------------------

pub struct Generateur {
    continents: Fbm<Perlin>,
    relief: Fbm<Perlin>,
    detail: Fbm<Perlin>,
}

fn echelle(p: [f64; 3], k: f64) -> [f64; 3] { [p[0] * k, p[1] * k, p[2] * k] }

fn palier(bas: f64, haut: f64, v: f64) -> f64 {
    let t = ((v - bas) / (haut - bas)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Generateur {
    pub fn nouveau(graine: u32) -> Self {
        Self {
            continents: Fbm::<Perlin>::new(graine).set_octaves(3),
            relief: Fbm::<Perlin>::new(graine.wrapping_add(1)).set_octaves(5),
            detail: Fbm::<Perlin>::new(graine.wrapping_add(2)).set_octaves(3),
        }
    }

    /// Altitude du sol et biome, pour une position de face quelconque —
    /// y compris hors de la face, que `replier_bloc` ramène.
    pub fn colonne(&self, face: u8, u: i32, v: i32) -> (i32, Biome) {
        let (f, u, v, _) = replier_bloc(face, u, v);
        let p = point_sphere(f, u, v);
        // Latitude vraie, ramenée entre 0 (équateur) et 1 (pôle).
        let latitude01 = p[2].asin().abs() / std::f64::consts::FRAC_PI_2;

        let c = self.continents.get(echelle(p, 2.2));
        let terres = palier(-0.06, 0.22, c);
        let r = self.relief.get(echelle(p, 22.0)) * 0.5 + 0.5;
        let d = self.detail.get(echelle(p, 80.0));

        let mut h = NIVEAU_MER as f64 - 9.0 + terres * (13.0 + 46.0 * r) + d * 5.0;

        // D24 : les pôles sont des glaciers plats. Ce sont désormais de vraies
        // calottes, au centre de deux faces opposées.
        let polaire = palier(0.84, 0.97, latitude01);
        h = h * (1.0 - polaire) + (NIVEAU_MER as f64 + 5.0) * polaire;

        (
            (h.round() as i32).clamp(1, HAUTEUR_CHUNK - 2),
            biome_de(latitude01),
        )
    }

    pub fn hauteur(&self, face: u8, u: i32, v: i32) -> f32 {
        self.colonne(face, u, v).0 as f32
    }

    pub fn bloc(&self, sol: i32, biome: Biome, z: i32) -> Bloc {
        if z > sol {
            if z <= NIVEAU_MER { Bloc::Eau } else { Bloc::Air }
        } else if z == sol {
            if sol <= NIVEAU_MER + 1 { Bloc::Sable } else { biome.surface() }
        } else if z > sol - 4 {
            Bloc::Terre
        } else {
            Bloc::Roche
        }
    }
}

/// La bordure qu'on s'interdit : apparaître au bord d'une face, c'est
/// commencer la partie sur une couture.
const RETRAIT: i32 = FACE / 8;

/// Le point d'apparition : la première terre ferme de la bande de prairie,
/// cherchée sur une face équatoriale. Rien ne garantit qu'une latitude donnée
/// soit émergée — la carte est faite de bruit, pas de promesses.
pub fn point_apparition(gen: &Generateur) -> (u8, i32, i32) {
    let pas = 32;
    for face in [1u8, 0, 2, 3] {
        for v in (RETRAIT..FACE - RETRAIT).step_by(pas) {
            for u in (RETRAIT..FACE - RETRAIT).step_by(pas) {
                let (sol, biome) = gen.colonne(face, u, v);
                if biome == Biome::Prairie && sol > NIVEAU_MER + 4 {
                    return (face, u, v);
                }
            }
        }
    }
    (1, FACE / 2, FACE / 2)
}
