//! Le monde : une grille plate dont les bords sont recolles (D27).
//!
//! Tout accede au monde par [`plier_bloc`]. C'est la discipline qui fait la
//! demonstration : la continuite aux coutures n'est pas un correctif applique
//! apres coup, c'est une propriete du seul chemin d'acces qui existe.
//!
//! Le champ de bruit, lui, vit sur une vraie sphere : les coordonnees de grille
//! sont converties en longitude/latitude puis en un point de la sphere unite,
//! ou le bruit est echantillonne en 3D. Le champ est donc continu partout, y
//! compris aux poles, sans qu'aucune formule spherique ne redescende jamais
//! dans la logique de jeu.

use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

pub const TAILLE_CHUNK: i32 = 32;
pub const HAUTEUR_CHUNK: i32 = 128;

/// Largeur du monde en chunks. Doit etre paire : le recollement polaire decale
/// d'une demi-largeur.
pub const MONDE_W: i32 = 64;
/// Hauteur du monde en chunks, d'un pole a l'autre.
pub const MONDE_H: i32 = 32;

pub const BLOCS_W: i32 = MONDE_W * TAILLE_CHUNK;
pub const BLOCS_H: i32 = MONDE_H * TAILLE_CHUNK;

pub const NIVEAU_MER: i32 = 40;

// --------------------------------------------------------------------------
// Topologie
// --------------------------------------------------------------------------

/// Ramene une position de bloc arbitraire dans le monde canonique, et rend le
/// nombre de plis polaires traverses.
///
/// - Est-ouest : `x` modulo la largeur, a `y` constant.
/// - Nord-sud : franchir un pole replie `y` sur lui-meme et decale `x` d'une
///   demi-largeur. Traverser le pole nord, c'est ressortir sur le meridien
///   oppose en marchant a l'envers — exactement ce que fait une sphere.
pub fn plier_bloc(x: i32, y: i32) -> (i32, i32, u32) {
    plier(x, y, BLOCS_W, BLOCS_H)
}

/// Meme repliement, a l'echelle du chunk.
pub fn plier_chunk(cx: i32, cy: i32) -> (i32, i32, u32) {
    plier(cx, cy, MONDE_W, MONDE_H)
}

fn plier(mut x: i32, mut y: i32, w: i32, h: i32) -> (i32, i32, u32) {
    let mut plis = 0;
    loop {
        if y < 0 {
            y = -1 - y;
            x += w / 2;
            plis += 1;
        } else if y >= h {
            y = 2 * h - 1 - y;
            x += w / 2;
            plis += 1;
        } else {
            break;
        }
    }
    (x.rem_euclid(w), y, plis)
}

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

    /// Couleur en espace lineaire — la cible de rendu est en sRGB et se charge
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
            Biome::Tempere => "tempere",
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

/// Distance a l'equateur, de 0 (equateur) a 1 (pole).
fn latitude01(y: i32) -> f64 {
    let t = (y as f64 + 0.5) / BLOCS_H as f64;
    ((t - 0.5) * 2.0).abs()
}

/// Distance a l'equateur de la bande de prairie : le milieu d'un hemisphere
/// (D24). C'est la que le jeu commence.
pub const LATITUDE_PRAIRIE: f64 = 0.45;

pub fn biome(y: i32) -> Biome {
    let d = latitude01(y);
    if d > 0.86 {
        Biome::Glacier
    } else if d > 0.66 {
        Biome::Neigeux
    } else if (d - LATITUDE_PRAIRIE).abs() < 0.13 {
        Biome::Prairie
    } else {
        Biome::Tempere
    }
}

/// Le point d'apparition : la premiere terre ferme rencontree en parcourant la
/// bande de prairie de l'hemisphere nord. Rien ne garantit qu'une latitude
/// donnee soit emergee — la carte est faite de bruit, pas de promesses.
pub fn point_apparition(gen: &Generateur) -> (i32, i32) {
    let y = ((0.5 - LATITUDE_PRAIRIE / 2.0) * BLOCS_H as f64) as i32;
    for dx in 0..BLOCS_W {
        let x = (BLOCS_W / 4 + dx).rem_euclid(BLOCS_W);
        if gen.hauteur(x, y) > NIVEAU_MER as f32 + 4.0 {
            return (x, y);
        }
    }
    (BLOCS_W / 4, y)
}

// --------------------------------------------------------------------------
// Generation
// --------------------------------------------------------------------------

pub struct Generateur {
    continents: Fbm<Perlin>,
    relief: Fbm<Perlin>,
    detail: Fbm<Perlin>,
}

/// Point de la sphere unite correspondant a une case de la grille.
fn point_sphere(x: f64, y: f64) -> [f64; 3] {
    let lon = x / BLOCS_W as f64 * TAU;
    let lat = (y + 0.5) / BLOCS_H as f64 * PI - FRAC_PI_2;
    let (slat, clat) = lat.sin_cos();
    let (slon, clon) = lon.sin_cos();
    [clat * clon, clat * slon, slat]
}

fn echelle(p: [f64; 3], k: f64) -> [f64; 3] { [p[0] * k, p[1] * k, p[2] * k] }

fn palier(bas: f64, haut: f64, v: f64) -> f64 {
    (((v - bas) / (haut - bas)).clamp(0.0, 1.0)).powi(2) * (3.0 - 2.0 * ((v - bas) / (haut - bas)).clamp(0.0, 1.0))
}

impl Generateur {
    pub fn nouveau(graine: u32) -> Self {
        Self {
            continents: Fbm::<Perlin>::new(graine).set_octaves(3),
            relief: Fbm::<Perlin>::new(graine.wrapping_add(1)).set_octaves(5),
            detail: Fbm::<Perlin>::new(graine.wrapping_add(2)).set_octaves(3),
        }
    }

    /// Altitude du sol, pour une position de bloc quelconque.
    pub fn hauteur(&self, x: i32, y: i32) -> f32 {
        let (wx, wy, _) = plier_bloc(x, y);
        let p = point_sphere(wx as f64, wy as f64);

        let c = self.continents.get(echelle(p, 2.2));
        let terres = palier(-0.06, 0.22, c);
        let r = self.relief.get(echelle(p, 9.0)) * 0.5 + 0.5;
        let d = self.detail.get(echelle(p, 26.0));

        let mut h = NIVEAU_MER as f64 - 9.0 + terres * (13.0 + 38.0 * r) + d * 3.0;

        // D24 : les poles sont des glaciers plats. L'aplatissement sert aussi a
        // masquer la distorsion de la grille la ou les meridiens se rejoignent.
        let polaire = palier(0.84, 0.97, latitude01(wy));
        h = h * (1.0 - polaire) + (NIVEAU_MER as f64 + 5.0) * polaire;

        h as f32
    }

    /// Une colonne : altitude du sol et bloc de surface.
    pub fn colonne(&self, x: i32, y: i32) -> (i32, Biome) {
        let (_, wy, _) = plier_bloc(x, y);
        (
            (self.hauteur(x, y).round() as i32).clamp(1, HAUTEUR_CHUNK - 2),
            biome(wy),
        )
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
